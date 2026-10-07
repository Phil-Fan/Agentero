//! Pairing and bearer-token authentication.
//!
//! Pairing is out-of-band TOFU: the node prints a short-lived one-time code; a
//! client exchanges it at `POST /pair` for a long-lived bearer token. Tokens are
//! stored **hashed** on the node. All of this is inert unless the node is started
//! with `--require-auth`.

use std::path::PathBuf;
use std::sync::RwLock;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::paths;
use crate::{NodeError, Result};

/// How long a pairing code stays valid.
const PAIRING_TTL: Duration = Duration::from_secs(600);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenEntry {
    pub token_hash: String,
    pub name: String,
    pub created_at: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TokenFile {
    #[serde(default)]
    tokens: Vec<TokenEntry>,
}

struct Pairing {
    code: String,
    expires_at: Instant,
}

pub struct Security {
    require_auth: bool,
    path: PathBuf,
    tokens: RwLock<Vec<TokenEntry>>,
    pairing: RwLock<Option<Pairing>>,
}

impl Security {
    /// Load persisted tokens (hashes only).
    pub fn load(require_auth: bool) -> Result<Self> {
        let path = paths::tokens_path();
        let tokens = if path.is_file() {
            let text = std::fs::read_to_string(&path)?;
            serde_json::from_str::<TokenFile>(&text)
                .map(|file| file.tokens)
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        Ok(Self {
            require_auth,
            path,
            tokens: RwLock::new(tokens),
            pairing: RwLock::new(None),
        })
    }

    pub fn require_auth(&self) -> bool {
        self.require_auth
    }

    pub fn token_count(&self) -> usize {
        self.tokens.read().unwrap().len()
    }

    /// Ensure a valid pairing code exists and return it (only when auth is on).
    pub fn ensure_pairing_code(&self) -> Option<String> {
        if !self.require_auth {
            return None;
        }
        let mut slot = self.pairing.write().unwrap();
        let still_valid = slot
            .as_ref()
            .map(|p| p.expires_at > Instant::now())
            .unwrap_or(false);
        if !still_valid {
            *slot = Some(Pairing {
                code: six_digit(),
                expires_at: Instant::now() + PAIRING_TTL,
            });
        }
        slot.as_ref().map(|p| p.code.clone())
    }

    /// Exchange a valid pairing code for a fresh bearer token.
    pub fn pair(&self, code: &str, name: &str) -> Result<String> {
        {
            let slot = self.pairing.read().unwrap();
            match slot.as_ref() {
                Some(pairing)
                    if pairing.expires_at > Instant::now() && pairing.code == code.trim() => {}
                _ => return Err(NodeError::msg("invalid or expired pairing code")),
            }
        }
        let token = random_hex(32);
        {
            let mut tokens = self.tokens.write().unwrap();
            tokens.push(TokenEntry {
                token_hash: hash(&token),
                name: {
                    let trimmed = name.trim();
                    if trimmed.is_empty() {
                        "client".to_string()
                    } else {
                        trimmed.to_string()
                    }
                },
                created_at: chrono::Utc::now().to_rfc3339(),
            });
            self.persist(&tokens)?;
        }
        // Single use.
        *self.pairing.write().unwrap() = None;
        Ok(token)
    }

    /// Verify an `Authorization` header value (`Bearer <token>` or a bare token).
    pub fn verify(&self, header: Option<&str>) -> bool {
        if !self.require_auth {
            return true;
        }
        let Some(header) = header else {
            return false;
        };
        let token = header
            .trim()
            .strip_prefix("Bearer ")
            .unwrap_or(header)
            .trim();
        let hashed = hash(token);
        self.tokens
            .read()
            .unwrap()
            .iter()
            .any(|entry| entry.token_hash == hashed)
    }

    fn persist(&self, tokens: &[TokenEntry]) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = TokenFile {
            tokens: tokens.to_vec(),
        };
        std::fs::write(&self.path, serde_json::to_string_pretty(&file)?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }
}

fn hash(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    hex::encode(hasher.finalize())
}

fn random_hex(bytes: usize) -> String {
    let mut buffer = vec![0u8; bytes];
    let _ = getrandom::getrandom(&mut buffer);
    hex::encode(buffer)
}

fn six_digit() -> String {
    let mut buffer = [0u8; 4];
    let _ = getrandom::getrandom(&mut buffer);
    format!("{:06}", u32::from_le_bytes(buffer) % 1_000_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_disabled_accepts_everything() {
        let security = Security::load(false).unwrap();
        assert!(security.verify(None));
        assert!(security.ensure_pairing_code().is_none());
    }

    #[test]
    fn pairing_issues_a_working_token() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("XDG_CONFIG_HOME", dir.path());
        let security = Security::load(true).unwrap();
        let code = security.ensure_pairing_code().unwrap();
        assert!(!security.verify(Some("Bearer nope")));

        let token = security.pair(&code, "test-client").unwrap();
        assert!(security.verify(Some(&format!("Bearer {token}"))));
        assert!(security.verify(Some(&token))); // bare token also accepted
        assert_eq!(security.token_count(), 1);

        // Code is single-use.
        assert!(security.pair(&code, "again").is_err());
        std::env::remove_var("XDG_CONFIG_HOME");
    }
}

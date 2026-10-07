//! Ed25519 node identity.
//!
//! The node id is the SHA-256 fingerprint of the public key (hex). The private
//! key is stored alongside for future pairing/attestation; for now it is only
//! written with `0600` permissions on Unix.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::paths;
use crate::{NodeError, Result};

pub const IDENTITY_VERSION: u32 = 1;
pub const ALGORITHM: &str = "ed25519";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeIdentity {
    pub version: u32,
    /// Full SHA-256 fingerprint of the public key (hex, 64 chars).
    pub node_id: String,
    /// First 12 chars of `node_id`, for display.
    pub short_id: String,
    pub name: String,
    pub algorithm: String,
    /// Ed25519 public key (hex, 64 chars).
    pub public_key: String,
    /// Ed25519 private seed (hex, 64 chars). Sensitive.
    pub private_key: String,
    pub created_at: String,
}

impl NodeIdentity {
    /// Load the identity if it exists.
    pub fn load() -> Result<Option<Self>> {
        let path = paths::identity_path();
        if !path.is_file() {
            return Ok(None);
        }
        let text = std::fs::read_to_string(&path)?;
        let identity: NodeIdentity = serde_json::from_str(&text)?;
        Ok(Some(identity))
    }

    /// Load the identity, generating one on first use.
    pub fn load_or_create() -> Result<Self> {
        match Self::load()? {
            Some(identity) => Ok(identity),
            None => Self::create(None),
        }
    }

    /// Generate a fresh identity and persist it.
    pub fn create(name: Option<String>) -> Result<Self> {
        let mut seed = [0u8; 32];
        getrandom::getrandom(&mut seed)
            .map_err(|e| NodeError::msg(format!("entropy unavailable: {e}")))?;
        let signing = ed25519_dalek::SigningKey::from_bytes(&seed);
        let public_key = hex::encode(signing.verifying_key().to_bytes());
        let node_id = fingerprint(&public_key);
        let short_id = node_id.chars().take(12).collect();
        let identity = NodeIdentity {
            version: IDENTITY_VERSION,
            node_id,
            short_id,
            name: normalize_name(name),
            algorithm: ALGORITHM.to_string(),
            public_key,
            private_key: hex::encode(seed),
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        identity.save()?;
        Ok(identity)
    }

    /// Persist the identity (creating the directory; `0600` on Unix).
    pub fn save(&self) -> Result<()> {
        let path = paths::identity_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, text)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }
}

/// Convenience: load the identity if present (see [`NodeIdentity::load`]).
pub fn load() -> Result<Option<NodeIdentity>> {
    NodeIdentity::load()
}

/// Convenience: load or generate the identity (see [`NodeIdentity::load_or_create`]).
pub fn load_or_create() -> Result<NodeIdentity> {
    NodeIdentity::load_or_create()
}

/// Convenience: generate a fresh identity (see [`NodeIdentity::create`]).
pub fn create(name: Option<String>) -> Result<NodeIdentity> {
    NodeIdentity::create(name)
}

/// SHA-256 fingerprint (hex) of an Ed25519 public key given as hex.
pub fn fingerprint(public_key_hex: &str) -> String {
    let bytes = hex::decode(public_key_hex).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    hex::encode(hasher.finalize())
}

fn normalize_name(name: Option<String>) -> String {
    name.map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(crate::probe::hostname)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_is_stable_hex() {
        let pk = "00".repeat(32);
        let fp = fingerprint(&pk);
        assert_eq!(fp.len(), 64);
        assert!(fp.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(fp, fingerprint(&pk));
    }

    #[test]
    fn create_roundtrips_via_disk() {
        // Isolated config dir so the test never touches the real identity.
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("XDG_CONFIG_HOME", dir.path());
        let created = NodeIdentity::create(Some("test-node".into())).unwrap();
        assert_eq!(created.name, "test-node");
        assert_eq!(created.node_id.len(), 64);
        assert_eq!(created.short_id.len(), 12);
        assert_eq!(created.private_key.len(), 64);

        let loaded = NodeIdentity::load().unwrap().expect("identity persisted");
        assert_eq!(loaded.node_id, created.node_id);
        assert_eq!(loaded.public_key, created.public_key);
        std::env::remove_var("XDG_CONFIG_HOME");
    }
}

//! Vault scanning → manifest (relative path → content hash + stat).

use crate::core::error::AppError;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;
use std::time::UNIX_EPOCH;

/// Immutable remote snapshot: `files` keys are `/`-separated vault-relative paths.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub version: u64,
    #[serde(default)]
    pub files: BTreeMap<String, FileEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    /// Lowercase hex sha256 of the raw file content (blob key).
    pub hash: String,
    pub size: u64,
    pub mtime_ms: i64,
}

/// Directories never entered and files never synced.
/// Must stay a superset of the watcher's ignore rules so sync state and
/// catalog SQLite never travel through the blob store.
pub(crate) fn is_ignored_name(name: &str) -> bool {
    matches!(name, ".agentero" | ".git" | "node_modules" | ".DS_Store") || name.ends_with(".tmp")
}

/// Scan the vault into manifest entries. Files whose `size + mtime` match the
/// base entry reuse its hash instead of re-reading (cheap steady-state scans).
pub fn scan_vault(vault: &Path, base: &Manifest) -> Result<BTreeMap<String, FileEntry>, AppError> {
    let mut out = BTreeMap::new();
    let walker = walkdir::WalkDir::new(vault)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || !is_ignored_name(&e.file_name().to_string_lossy()));
    for entry in walker {
        let entry = entry.map_err(|e| AppError::message(format!("scan vault: {e}")))?;
        if !entry.file_type().is_file() {
            continue;
        }
        let Some(rel) = entry
            .path()
            .strip_prefix(vault)
            .ok()
            .and_then(|p| p.to_str())
            .map(|s| s.replace('\\', "/"))
        else {
            continue; // non-UTF-8 names cannot ride a JSON manifest
        };
        let meta = entry
            .metadata()
            .map_err(|e| AppError::message(e.to_string()))?;
        let size = meta.len();
        let mtime_ms = mtime_millis(&meta);
        if let Some(prev) = base.files.get(&rel) {
            if prev.size == size && prev.mtime_ms == mtime_ms {
                out.insert(rel, prev.clone());
                continue;
            }
        }
        let hash = hash_file(entry.path())?;
        out.insert(
            rel,
            FileEntry {
                hash,
                size,
                mtime_ms,
            },
        );
    }
    Ok(out)
}

pub fn mtime_millis(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn hash_file(path: &Path) -> Result<String, AppError> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

pub fn hash_bytes(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use uuid::Uuid;

    #[test]
    fn scan_skips_internal_dirs_and_reuses_base_hashes() {
        let vault = std::env::temp_dir().join(format!("agentero-scan-{}", Uuid::new_v4()));
        fs::create_dir_all(vault.join(".agentero/sync")).unwrap();
        fs::create_dir_all(vault.join("papers/x")).unwrap();
        fs::write(vault.join(".agentero/catalog.sqlite"), b"db").unwrap();
        fs::write(vault.join(".agentero/sync/base.json"), b"{}").unwrap();
        fs::write(vault.join("papers/x/NOTES.md"), b"# x\n").unwrap();
        fs::write(vault.join("papers/x/.DS_Store"), b"junk").unwrap();

        let files = scan_vault(&vault, &Manifest::default()).unwrap();
        assert_eq!(files.keys().collect::<Vec<_>>(), vec!["papers/x/NOTES.md"]);

        // Unchanged size+mtime → hash reused from base without re-reading.
        let mut base = Manifest::default();
        let mut entry = files["papers/x/NOTES.md"].clone();
        entry.hash = "sentinel".into();
        base.files.insert("papers/x/NOTES.md".into(), entry);
        let again = scan_vault(&vault, &base).unwrap();
        assert_eq!(again["papers/x/NOTES.md"].hash, "sentinel");

        let _ = fs::remove_dir_all(&vault);
    }
}

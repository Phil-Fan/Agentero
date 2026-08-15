//! Sync engine: content-addressed blobs + immutable manifests + one mutable
//! `HEAD` pointer advanced with compare-and-swap.
//!
//! Remote layout under the configured prefix:
//!
//! ```text
//! vault.json                  { vaultId, formatVersion, encryption }
//! HEAD                        { version, manifestKey, updatedAt }   ← CAS
//! manifests/<v>-<nonce>.json.gz
//! blobs/<aa>/<sha256>         gzip(file content)
//! ```
//!
//! One pass = scan → three-way merge against the last synced manifest →
//! apply remote changes locally → upload new blobs → publish manifest →
//! CAS `HEAD`. A lost CAS re-runs the merge against the winner's manifest.

use crate::core::error::AppError;
use crate::features::sync::config::SyncBackendConfig;
use crate::features::sync::local::{self, SyncMeta};
use crate::features::sync::s3::{PutCondition, PutOutcome, S3Client};
use crate::features::sync::snapshot::{self, FileEntry, Manifest};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub const FORMAT_VERSION: u32 = 1;
const HEAD_KEY: &str = "HEAD";
const VAULT_KEY: &str = "vault.json";
const MAX_CAS_RETRIES: usize = 5;
/// Decompressed manifest cap; a vault of 1M files serializes to ~150MB.
const MAX_MANIFEST_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoteVaultInfo {
    vault_id: String,
    format_version: u32,
    encryption: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HeadPointer {
    version: u64,
    manifest_key: String,
    updated_at: String,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncOutcome {
    pub version: u64,
    pub uploaded: usize,
    pub downloaded: usize,
    pub deleted_local: usize,
    pub removed_remote: usize,
    pub conflict_copies: Vec<String>,
}

/// `(phase, current, total)` — totals of 0 mean indeterminate.
pub type Progress<'a> = &'a (dyn Fn(&str, usize, usize) + Send + Sync);

/// Quick credential/bucket check used by `sync_configure`.
pub async fn test_connection(cfg: &SyncBackendConfig) -> Result<(), AppError> {
    S3Client::new(cfg)?.list("", 1).await.map(|_| ())
}

pub async fn sync_vault(
    vault: &Path,
    cfg: &SyncBackendConfig,
    progress: Progress<'_>,
) -> Result<SyncOutcome, AppError> {
    let client = S3Client::new(cfg)?;
    ensure_remote_identity(vault, &client).await?;

    progress("scan", 0, 0);
    let base = local::read_base(vault);
    let mut base_files = base.files.clone();
    let mut local_files = {
        let vault = vault.to_path_buf();
        tokio::task::spawn_blocking(move || snapshot::scan_vault(&vault, &base))
            .await
            .map_err(|e| AppError::message(format!("scan task: {e}")))??
    };

    let mut outcome = SyncOutcome::default();
    for _attempt in 0..MAX_CAS_RETRIES {
        progress("pull", 0, 0);
        let head = client.get(HEAD_KEY).await?;
        let (head_ptr, head_etag) = match &head {
            Some((bytes, etag)) => (
                Some(serde_json::from_slice::<HeadPointer>(bytes)?),
                Some(etag.clone()),
            ),
            None => (None, None),
        };
        let remote_files = match &head_ptr {
            Some(ptr) => {
                let (bytes, _) = client.get(&ptr.manifest_key).await?.ok_or_else(|| {
                    AppError::message(format!("manifest {} missing", ptr.manifest_key))
                })?;
                let manifest: Manifest =
                    serde_json::from_slice(&gunzip_limited(&bytes, MAX_MANIFEST_BYTES)?)?;
                validate_manifest(&manifest)?;
                manifest.files
            }
            None => BTreeMap::new(),
        };

        let plan = merge(&base_files, &local_files, &remote_files);
        apply_local(vault, &client, &plan, &mut outcome, progress).await?;
        let merged = plan.merged;

        // Upload blobs the remote has never referenced. `If-None-Match: *`
        // makes duplicate uploads across devices a cheap no-op.
        let remote_hashes: std::collections::HashSet<&str> =
            remote_files.values().map(|e| e.hash.as_str()).collect();
        let uploads: Vec<(&String, &FileEntry)> = merged
            .iter()
            .filter(|(_, e)| !remote_hashes.contains(e.hash.as_str()))
            .collect();
        let total = uploads.len();
        for (i, (rel, entry)) in uploads.into_iter().enumerate() {
            progress("upload", i + 1, total);
            let raw = fs::read(vault.join(rel))?;
            if snapshot::hash_bytes(&raw) != entry.hash {
                return Err(AppError::message(format!(
                    "{rel} changed during sync; run sync again"
                )));
            }
            client
                .put(
                    &blob_key(&entry.hash),
                    gzip(&raw)?,
                    PutCondition::IfNoneMatch,
                )
                .await?;
            outcome.uploaded += 1;
        }

        progress("finalize", 0, 0);
        let new_version = head_ptr.as_ref().map(|p| p.version).unwrap_or(0) + 1;
        let manifest_key = format!(
            "manifests/{new_version:010}-{}.json.gz",
            &uuid::Uuid::new_v4().simple().to_string()[..8]
        );
        let manifest = Manifest {
            version: new_version,
            files: merged.clone(),
        };
        client
            .put(
                &manifest_key,
                gzip(&serde_json::to_vec(&manifest)?)?,
                PutCondition::IfNoneMatch,
            )
            .await?;
        let next_head = HeadPointer {
            version: new_version,
            manifest_key,
            updated_at: now(),
        };
        let condition = match head_etag {
            Some(etag) => PutCondition::IfMatch(etag),
            None => PutCondition::IfNoneMatch,
        };
        match client
            .put(HEAD_KEY, serde_json::to_vec(&next_head)?, condition)
            .await?
        {
            PutOutcome::Ok => {
                local::write_base(vault, &manifest)?;
                local::write_meta(
                    vault,
                    &SyncMeta {
                        last_sync_at: Some(now()),
                        last_version: new_version,
                    },
                )?;
                refresh_catalog_if_needed(vault, &outcome).await;
                outcome.version = new_version;
                outcome.removed_remote = remote_files
                    .keys()
                    .filter(|p| !merged.contains_key(*p))
                    .count();
                return Ok(outcome);
            }
            PutOutcome::PreconditionFailed => {
                // Another device won the race: their manifest becomes the new
                // remote truth; our applied state becomes the new local truth.
                base_files = remote_files;
                local_files = merged;
            }
        }
    }
    Err(AppError::message(
        "sync contention: too many concurrent updates, try again",
    ))
}

/// First contact: create or verify the remote store identity.
/// A vault that never synced adopts an existing remote id (joining a store);
/// a vault with sync history refuses a foreign store.
async fn ensure_remote_identity(vault: &Path, client: &S3Client) -> Result<String, AppError> {
    let vault_id = local::ensure_vault_id(vault)?;
    match client.get(VAULT_KEY).await? {
        Some((bytes, _)) => {
            let info: RemoteVaultInfo = serde_json::from_slice(&bytes)
                .map_err(|_| AppError::message("remote vault.json is not a sync store"))?;
            if info.format_version > FORMAT_VERSION {
                return Err(AppError::message(
                    "remote sync store uses a newer format; update the app",
                ));
            }
            if info.vault_id == vault_id {
                return Ok(vault_id);
            }
            if local::read_base(vault).files.is_empty() {
                local::set_vault_id(vault, &info.vault_id)?;
                return Ok(info.vault_id);
            }
            Err(AppError::message(
                "remote sync store belongs to a different vault",
            ))
        }
        None => {
            let info = RemoteVaultInfo {
                vault_id: vault_id.clone(),
                format_version: FORMAT_VERSION,
                encryption: "none".into(),
            };
            // Lost create race → re-verify against the winner.
            match client
                .put(
                    VAULT_KEY,
                    serde_json::to_vec_pretty(&info)?,
                    PutCondition::IfNoneMatch,
                )
                .await?
            {
                PutOutcome::Ok => Ok(vault_id),
                PutOutcome::PreconditionFailed => {
                    Box::pin(ensure_remote_identity(vault, client)).await
                }
            }
        }
    }
}

/// Remote manifests are untrusted input: paths feed `vault.join` for
/// write/delete and hashes feed `blob_key`, so reject anything that could
/// escape the vault or index outside `blobs/`.
fn validate_manifest(manifest: &Manifest) -> Result<(), AppError> {
    for (rel, entry) in &manifest.files {
        if rel.is_empty()
            || rel.starts_with('/')
            || rel.contains('\\')
            || rel
                .split('/')
                .any(|c| c.is_empty() || c == ".." || c == ".")
        {
            return Err(AppError::message(format!(
                "remote manifest has an unsafe path: {rel}"
            )));
        }
        if entry.hash.len() != 64
            || !entry
                .hash
                .bytes()
                .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        {
            return Err(AppError::message(format!(
                "remote manifest has an invalid hash for {rel}"
            )));
        }
    }
    Ok(())
}

struct MergePlan {
    merged: BTreeMap<String, FileEntry>,
    downloads: Vec<(String, FileEntry)>,
    delete_local: Vec<String>,
    /// `(original rel, conflict-copy rel)` — copy the local file aside before
    /// the remote version overwrites it.
    preserve_local: Vec<(String, String)>,
}

/// Three-way merge per path. Only-one-side changes win outright; both-changed
/// Markdown keeps the newer version and saves the loser as a conflict copy;
/// everything else (sidecars, marks, binaries) is last-writer-wins by mtime.
/// Delete-vs-modify keeps the modification.
fn merge(
    base: &BTreeMap<String, FileEntry>,
    local: &BTreeMap<String, FileEntry>,
    remote: &BTreeMap<String, FileEntry>,
) -> MergePlan {
    let mut plan = MergePlan {
        merged: BTreeMap::new(),
        downloads: Vec::new(),
        delete_local: Vec::new(),
        preserve_local: Vec::new(),
    };
    let mut paths: Vec<&String> = base
        .keys()
        .chain(local.keys())
        .chain(remote.keys())
        .collect();
    paths.sort();
    paths.dedup();

    for path in paths {
        let b = base.get(path);
        let l = local.get(path);
        let r = remote.get(path);
        let same = |a: Option<&FileEntry>, b: Option<&FileEntry>| match (a, b) {
            (Some(x), Some(y)) => x.hash == y.hash,
            (None, None) => true,
            _ => false,
        };

        if same(l, r) {
            if let Some(entry) = l {
                plan.merged.insert(path.clone(), entry.clone());
            }
        } else if same(r, b) {
            // Local-only change (edit or delete) wins; deletion = omit.
            if let Some(entry) = l {
                plan.merged.insert(path.clone(), entry.clone());
            }
        } else if same(l, b) {
            // Remote-only change wins.
            match r {
                Some(entry) => {
                    plan.downloads.push((path.clone(), entry.clone()));
                    plan.merged.insert(path.clone(), entry.clone());
                }
                None => plan.delete_local.push(path.clone()),
            }
        } else {
            match (l, r) {
                // Divergent edits.
                (Some(le), Some(re)) => {
                    let remote_newer = re.mtime_ms >= le.mtime_ms;
                    if path.ends_with(".md") {
                        let copy = conflict_name(path);
                        if remote_newer {
                            plan.preserve_local.push((path.clone(), copy.clone()));
                            plan.merged.insert(copy, le.clone());
                            plan.downloads.push((path.clone(), re.clone()));
                            plan.merged.insert(path.clone(), re.clone());
                        } else {
                            plan.downloads.push((copy.clone(), re.clone()));
                            plan.merged.insert(copy, re.clone());
                            plan.merged.insert(path.clone(), le.clone());
                        }
                    } else if remote_newer {
                        plan.downloads.push((path.clone(), re.clone()));
                        plan.merged.insert(path.clone(), re.clone());
                    } else {
                        plan.merged.insert(path.clone(), le.clone());
                    }
                }
                // Delete vs modify → keep the modification.
                (Some(le), None) => {
                    plan.merged.insert(path.clone(), le.clone());
                }
                (None, Some(re)) => {
                    plan.downloads.push((path.clone(), re.clone()));
                    plan.merged.insert(path.clone(), re.clone());
                }
                (None, None) => {}
            }
        }
    }
    plan
}

async fn apply_local(
    vault: &Path,
    client: &S3Client,
    plan: &MergePlan,
    outcome: &mut SyncOutcome,
    progress: Progress<'_>,
) -> Result<(), AppError> {
    for (from, to) in &plan.preserve_local {
        fs::copy(vault.join(from), vault.join(to))?;
        outcome.conflict_copies.push(to.clone());
    }
    let total = plan.downloads.len();
    for (i, (rel, entry)) in plan.downloads.iter().enumerate() {
        progress("download", i + 1, total);
        let (bytes, _) = client
            .get(&blob_key(&entry.hash))
            .await?
            .ok_or_else(|| AppError::message(format!("blob missing for {rel}")))?;
        let raw = gunzip_limited(&bytes, entry.size.saturating_add(1 << 20))?;
        if snapshot::hash_bytes(&raw) != entry.hash {
            return Err(AppError::message(format!("blob corrupt for {rel}")));
        }
        write_atomic(&vault.join(rel), &raw)?;
        outcome.downloaded += 1;
    }
    for rel in &plan.delete_local {
        match fs::remove_file(vault.join(rel)) {
            Ok(()) => outcome.deleted_local += 1,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(AppError::message(format!("delete {rel}: {e}"))),
        }
    }
    Ok(())
}

/// Downloaded sidecars / deleted paper folders must be reflected in catalog.
async fn refresh_catalog_if_needed(vault: &Path, outcome: &SyncOutcome) {
    if outcome.downloaded == 0 && outcome.deleted_local == 0 {
        return;
    }
    let vault = vault.to_path_buf();
    let _ = tokio::task::spawn_blocking(move || {
        if let Err(e) = crate::features::catalog::papers::rebuild_from_disk(&vault) {
            log::warn!(target: "agentero::sync", "catalog rebuild after sync: {e}");
        }
        if let Err(e) = crate::features::catalog::papers::prune_missing(&vault) {
            log::warn!(target: "agentero::sync", "catalog prune after sync: {e}");
        }
    })
    .await;
}

fn blob_key(hash: &str) -> String {
    format!("blobs/{}/{hash}", &hash[..2])
}

fn conflict_name(path: &str) -> String {
    let stamp = chrono::Local::now().format("%Y-%m-%d %H%M%S");
    match path.rsplit_once('.') {
        Some((stem, ext)) => format!("{stem} (conflict {stamp}).{ext}"),
        None => format!("{path} (conflict {stamp})"),
    }
}

fn write_atomic(target: &PathBuf, bytes: &[u8]) -> Result<(), AppError> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = target.with_extension(format!(
        "{}.sync.tmp",
        target.extension().and_then(|e| e.to_str()).unwrap_or("")
    ));
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, target).or_else(|_| {
        // Windows may fail rename over an existing file.
        fs::write(target, bytes)
    })?;
    Ok(())
}

fn gzip(raw: &[u8]) -> Result<Vec<u8>, AppError> {
    let mut enc = GzEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(raw)?;
    Ok(enc.finish()?)
}

/// Decompress with an output cap; reading `limit + 1` bytes detects overflow
/// without allocating it.
fn gunzip_limited(bytes: &[u8], limit: u64) -> Result<Vec<u8>, AppError> {
    let mut out = Vec::new();
    let n = GzDecoder::new(bytes)
        .take(limit + 1)
        .read_to_end(&mut out)?;
    if n as u64 > limit {
        return Err(AppError::message(format!(
            "decompressed data exceeds the {limit} byte limit"
        )));
    }
    Ok(out)
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(hash: &str, mtime: i64) -> FileEntry {
        FileEntry {
            hash: hash.into(),
            size: 1,
            mtime_ms: mtime,
        }
    }

    fn map(pairs: &[(&str, FileEntry)]) -> BTreeMap<String, FileEntry> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    #[test]
    fn merge_one_sided_changes() {
        let base = map(&[("a.md", entry("h1", 1)), ("b.pdf", entry("h2", 1))]);
        let local = map(&[("a.md", entry("h1-local", 2)), ("b.pdf", entry("h2", 1))]);
        let remote = map(&[("a.md", entry("h1", 1))]); // remote deleted b.pdf

        let plan = merge(&base, &local, &remote);
        assert_eq!(plan.merged["a.md"].hash, "h1-local");
        assert!(!plan.merged.contains_key("b.pdf"));
        assert_eq!(plan.delete_local, vec!["b.pdf"]);
        assert!(plan.downloads.is_empty());
    }

    #[test]
    fn merge_divergent_md_keeps_both_versions() {
        let base = map(&[("n.md", entry("h0", 1))]);
        let local = map(&[("n.md", entry("hl", 10))]);
        let remote = map(&[("n.md", entry("hr", 20))]);

        let plan = merge(&base, &local, &remote);
        // Remote newer: it takes the path, local survives as conflict copy.
        assert_eq!(plan.merged["n.md"].hash, "hr");
        assert_eq!(plan.preserve_local.len(), 1);
        let copy = &plan.preserve_local[0].1;
        assert!(copy.contains("(conflict"));
        assert_eq!(plan.merged[copy].hash, "hl");
    }

    #[test]
    fn merge_divergent_binary_is_lww() {
        let base = map(&[("m.json", entry("h0", 1))]);
        let local = map(&[("m.json", entry("hl", 30))]);
        let remote = map(&[("m.json", entry("hr", 20))]);

        let plan = merge(&base, &local, &remote);
        assert_eq!(plan.merged["m.json"].hash, "hl");
        assert!(plan.downloads.is_empty());
        assert!(plan.preserve_local.is_empty());
    }

    #[test]
    fn merge_delete_vs_modify_keeps_modification() {
        let base = map(&[("k.md", entry("h0", 1))]);
        let local = map(&[]); // deleted locally
        let remote = map(&[("k.md", entry("hr", 5))]); // modified remotely

        let plan = merge(&base, &local, &remote);
        assert_eq!(plan.merged["k.md"].hash, "hr");
        assert_eq!(plan.downloads.len(), 1);
    }

    #[test]
    fn validate_manifest_rejects_unsafe_paths_and_hashes() {
        let ok_hash = "a".repeat(64);
        let valid = Manifest {
            version: 1,
            files: map(&[("papers/x/NOTES.md", entry(&ok_hash, 1))]),
        };
        assert!(validate_manifest(&valid).is_ok());

        for bad in [
            "../evil.md",
            "/abs.md",
            "a\\b.md",
            "a//b.md",
            "a/./b.md",
            "a/../b.md",
            "",
        ] {
            let m = Manifest {
                version: 1,
                files: map(&[(bad, entry(&ok_hash, 1))]),
            };
            assert!(
                validate_manifest(&m).is_err(),
                "path {bad:?} must be rejected"
            );
        }

        for bad_hash in ["", "abc", &"A".repeat(64), &"g".repeat(64), &"a".repeat(63)] {
            let m = Manifest {
                version: 1,
                files: map(&[("n.md", entry(bad_hash, 1))]),
            };
            assert!(
                validate_manifest(&m).is_err(),
                "hash {bad_hash:?} must be rejected"
            );
        }
    }

    #[test]
    fn gunzip_limited_enforces_cap() {
        let packed = gzip(&vec![0u8; 4096]).unwrap();
        assert_eq!(gunzip_limited(&packed, 4096).unwrap().len(), 4096);
        assert!(gunzip_limited(&packed, 4095).is_err());
    }

    /// Full two-device round trip against a live S3 endpoint.
    ///
    /// ```sh
    /// docker run -d --rm --name minio -p 19000:9000 \
    ///   -e MINIO_ROOT_USER=testkey -e MINIO_ROOT_PASSWORD=testsecret \
    ///   minio/minio server /data
    /// docker exec minio mc alias set local http://127.0.0.1:9000 testkey testsecret
    /// docker exec minio mc mb local/vault-test
    /// cargo test -p agentero --lib features::sync::engine -- --ignored
    /// ```
    #[tokio::test]
    #[ignore = "requires a local MinIO (see doc comment)"]
    async fn two_device_roundtrip_against_minio() {
        use crate::features::sync::config::SyncBackendConfig;
        use uuid::Uuid;

        let cfg = SyncBackendConfig {
            endpoint: std::env::var("AGENTERO_SYNC_TEST_ENDPOINT")
                .unwrap_or_else(|_| "http://127.0.0.1:19000".into()),
            region: "us-east-1".into(),
            bucket: "vault-test".into(),
            prefix: format!("it-{}", Uuid::new_v4()),
            access_key: "testkey".into(),
            secret_key: "testsecret".into(),
            force_path_style: true,
            auto_sync: false,
            interval_minutes: 30,
        };
        let noop: &(dyn Fn(&str, usize, usize) + Send + Sync) = &|_, _, _| {};

        let tmp = std::env::temp_dir().join(format!("agentero-sync-it-{}", Uuid::new_v4()));
        let (a, b) = (tmp.join("a"), tmp.join("b"));
        fs::create_dir_all(a.join("papers/x")).unwrap();
        fs::create_dir_all(&b).unwrap();
        fs::write(a.join("papers/x/NOTES.md"), "# x\n").unwrap();
        fs::write(a.join("papers/x/metadata.json"), r#"{"id":"x"}"#).unwrap();

        // A publishes, empty B joins and receives everything.
        let up = sync_vault(&a, &cfg, noop).await.expect("sync A");
        assert_eq!((up.version, up.uploaded), (1, 2));
        let down = sync_vault(&b, &cfg, noop).await.expect("sync B");
        assert_eq!((down.version, down.downloaded), (2, 2));
        assert_eq!(
            fs::read_to_string(b.join("papers/x/NOTES.md")).unwrap(),
            "# x\n"
        );

        // B edits; A picks it up.
        fs::write(b.join("papers/x/NOTES.md"), "# x\nedited on B\n").unwrap();
        sync_vault(&b, &cfg, noop).await.expect("sync B edit");
        let pull = sync_vault(&a, &cfg, noop).await.expect("sync A pull");
        assert_eq!(pull.downloaded, 1);
        assert!(fs::read_to_string(a.join("papers/x/NOTES.md"))
            .unwrap()
            .contains("edited on B"));

        // Divergent edits on both → conflict copy, then both converge.
        fs::write(a.join("papers/x/NOTES.md"), "# x\nA version\n").unwrap();
        fs::write(b.join("papers/x/NOTES.md"), "# x\nB version\n").unwrap();
        sync_vault(&a, &cfg, noop).await.expect("sync A divergent");
        let conflicted = sync_vault(&b, &cfg, noop).await.expect("sync B divergent");
        assert_eq!(conflicted.conflict_copies.len(), 1);
        sync_vault(&a, &cfg, noop).await.expect("sync A converge");
        // Compare content only: mtimes legitimately differ across devices.
        let hashes = |vault: &Path| -> BTreeMap<String, String> {
            snapshot::scan_vault(vault, &Manifest::default())
                .unwrap()
                .into_iter()
                .map(|(k, v)| (k, v.hash))
                .collect()
        };
        assert_eq!(hashes(&a), hashes(&b), "both devices converge");

        let _ = fs::remove_dir_all(&tmp);
    }
}

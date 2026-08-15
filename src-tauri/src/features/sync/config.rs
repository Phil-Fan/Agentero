//! Sync credentials — XDG `agentero/sync.json`, keyed by vault root path.
//!
//! Secrets stay outside the vault (the vault itself is what gets synced).
//! The secret key is masked with `*` on the way to the WebView, mirroring the
//! translate API-key convention.

use crate::core::error::AppError;
use crate::core::paths;
use crate::features::settings::{is_translate_api_key_mask, mask_translate_api_key};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SyncBackendConfig {
    /// S3-compatible endpoint, e.g. `https://<account>.r2.cloudflarestorage.com`.
    pub endpoint: String,
    #[serde(default = "default_region")]
    pub region: String,
    pub bucket: String,
    /// Optional key prefix inside the bucket (multiple vaults per bucket).
    #[serde(default)]
    pub prefix: String,
    pub access_key: String,
    pub secret_key: String,
    /// `{endpoint}/{bucket}/key` instead of `{bucket}.{endpoint}/key`.
    /// Path style works with R2 / MinIO / AWS alike, so it is the default.
    #[serde(default = "default_true")]
    pub force_path_style: bool,
}

fn default_region() -> String {
    "us-east-1".into()
}

fn default_true() -> bool {
    true
}

impl SyncBackendConfig {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.endpoint.trim().is_empty()
            || self.bucket.trim().is_empty()
            || self.access_key.trim().is_empty()
            || self.secret_key.trim().is_empty()
        {
            return Err(AppError::message(
                "endpoint, bucket, access key and secret key are required",
            ));
        }
        Ok(())
    }

    pub fn normalized(mut self) -> Self {
        self.endpoint = self.endpoint.trim().trim_end_matches('/').to_string();
        self.region = self.region.trim().to_string();
        if self.region.is_empty() {
            self.region = default_region();
        }
        self.bucket = self.bucket.trim().to_string();
        self.prefix = self.prefix.trim().trim_matches('/').to_string();
        self.access_key = self.access_key.trim().to_string();
        self.secret_key = self.secret_key.trim().to_string();
        self
    }

    /// Copy with the secret key replaced by a same-length `*` mask.
    pub fn masked(&self) -> Self {
        let mut out = self.clone();
        out.secret_key = mask_translate_api_key(&out.secret_key);
        out
    }

    /// Restore the previous secret when the UI echoes the mask back.
    pub fn merge_mask(&mut self, previous: Option<&Self>) {
        if is_translate_api_key_mask(&self.secret_key) {
            self.secret_key = previous.map(|p| p.secret_key.clone()).unwrap_or_default();
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct SyncConfigFile {
    #[serde(default)]
    vaults: HashMap<String, SyncBackendConfig>,
}

fn config_path() -> PathBuf {
    paths::agentero_config_dir().join("sync.json")
}

fn read_all() -> HashMap<String, SyncBackendConfig> {
    let path = config_path();
    let Ok(raw) = fs::read_to_string(&path) else {
        return HashMap::new();
    };
    match serde_json::from_str::<SyncConfigFile>(&raw) {
        Ok(file) => file.vaults,
        Err(e) => {
            log::warn!(target: "agentero::sync", "invalid sync.json: {e}");
            HashMap::new()
        }
    }
}

fn write_all(vaults: HashMap<String, SyncBackendConfig>) -> Result<(), AppError> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let raw = serde_json::to_string_pretty(&SyncConfigFile { vaults })?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, raw.as_bytes())?;
    fs::rename(&tmp, &path).or_else(|_| fs::write(&path, raw.as_bytes()))?;
    // Owner-only: this file holds S3 credentials.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

pub fn get(vault_path: &str) -> Option<SyncBackendConfig> {
    read_all().remove(vault_path)
}

pub fn set(vault_path: &str, config: SyncBackendConfig) -> Result<(), AppError> {
    let mut all = read_all();
    all.insert(vault_path.to_string(), config);
    write_all(all)
}

pub fn remove(vault_path: &str) -> Result<(), AppError> {
    let mut all = read_all();
    if all.remove(vault_path).is_some() {
        write_all(all)?;
    }
    Ok(())
}

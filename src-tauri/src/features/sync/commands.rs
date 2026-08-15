//! Sync commands. Long-running `sync_now` broadcasts `sync:state` /
//! `sync:progress` events so the settings pane (and later a status bar
//! indicator) can follow along from any window.

use super::config::{self, SyncBackendConfig};
use super::engine::{self, SyncOutcome};
use super::local;
use super::SyncService;
use crate::core::error::{map_err, ApiResult, AppError};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, State};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncVaultArgs {
    pub vault_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncConfigureArgs {
    pub vault_path: String,
    pub config: SyncBackendConfig,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub configured: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<SyncBackendConfig>,
    pub running: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_sync_at: Option<String>,
    pub last_version: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SyncStateEvent<'a> {
    vault_path: &'a str,
    status: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SyncProgressEvent<'a> {
    vault_path: &'a str,
    phase: &'a str,
    current: usize,
    total: usize,
}

fn vault_dir(vault_path: &str) -> Result<PathBuf, AppError> {
    let trimmed = vault_path.trim();
    if trimmed.is_empty() || trimmed.starts_with("remote:") {
        return Err(AppError::message("sync requires a local vault"));
    }
    let dir = PathBuf::from(trimmed);
    if !dir.is_dir() {
        return Err(AppError::message("vault path is not a directory"));
    }
    Ok(dir)
}

/// Current sync binding + last-pass info for the settings pane.
#[tauri::command]
pub async fn sync_get_status(
    args: SyncVaultArgs,
    service: State<'_, SyncService>,
) -> Result<ApiResult<SyncStatus>, String> {
    let running = service.is_running(args.vault_path.trim());
    Ok(match vault_dir(&args.vault_path) {
        Ok(dir) => {
            let meta = local::read_meta(&dir);
            let config = config::get(args.vault_path.trim());
            ApiResult::ok(SyncStatus {
                configured: config.is_some(),
                config: config.map(|c| c.masked()),
                running,
                last_sync_at: meta.last_sync_at,
                last_version: meta.last_version,
            })
        }
        Err(e) => map_err(e),
    })
}

/// Validate + connection-test + persist the S3 binding for a vault.
#[tauri::command]
pub async fn sync_configure(args: SyncConfigureArgs) -> ApiResult<SyncStatus> {
    let inner = async {
        let dir = vault_dir(&args.vault_path)?;
        let key = args.vault_path.trim().to_string();
        let mut cfg = args.config.normalized();
        cfg.merge_mask(config::get(&key).as_ref());
        cfg.validate()?;
        engine::test_connection(&cfg).await?;
        config::set(&key, cfg.clone())?;
        let meta = local::read_meta(&dir);
        Ok::<_, AppError>(SyncStatus {
            configured: true,
            config: Some(cfg.masked()),
            running: false,
            last_sync_at: meta.last_sync_at,
            last_version: meta.last_version,
        })
    };
    match inner.await {
        Ok(status) => ApiResult::ok(status),
        Err(e) => map_err(e),
    }
}

/// Remove the binding and local sync state (remote data stays untouched).
#[tauri::command]
pub async fn sync_disconnect(args: SyncVaultArgs) -> ApiResult<()> {
    let inner = || {
        let dir = vault_dir(&args.vault_path)?;
        config::remove(args.vault_path.trim())?;
        local::clear(&dir);
        Ok::<_, AppError>(())
    };
    match inner() {
        Ok(()) => ApiResult::ok(()),
        Err(e) => map_err(e),
    }
}

/// One full sync pass (scan → merge → apply → publish).
#[tauri::command]
pub async fn sync_now(
    app: AppHandle,
    service: State<'_, SyncService>,
    args: SyncVaultArgs,
) -> Result<ApiResult<SyncOutcome>, String> {
    use crate::core::log_util::OpTimer;

    let vault_key = args.vault_path.trim().to_string();
    let dir = match vault_dir(&args.vault_path) {
        Ok(dir) => dir,
        Err(e) => return Ok(map_err(e)),
    };
    let Some(cfg) = config::get(&vault_key) else {
        return Ok(map_err(AppError::message("sync is not configured")));
    };
    if !service.try_begin(&vault_key) {
        return Ok(map_err(AppError::message("sync already running")));
    }

    let op = OpTimer::start_with("sync_now", format!("vault={vault_key}"));
    emit_state(&app, &vault_key, "syncing", None);
    let progress_app = app.clone();
    let progress_key = vault_key.clone();
    let progress = move |phase: &str, current: usize, total: usize| {
        let _ = progress_app.emit(
            "sync:progress",
            SyncProgressEvent {
                vault_path: &progress_key,
                phase,
                current,
                total,
            },
        );
    };
    let result = engine::sync_vault(&dir, &cfg, &progress).await;
    service.end(&vault_key);

    Ok(match result {
        Ok(outcome) => {
            emit_state(&app, &vault_key, "idle", None);
            op.finish_ok_extra(format!(
                "version={} up={} down={} conflicts={}",
                outcome.version,
                outcome.uploaded,
                outcome.downloaded,
                outcome.conflict_copies.len()
            ));
            ApiResult::ok(outcome)
        }
        Err(e) => {
            emit_state(&app, &vault_key, "error", Some(e.to_string()));
            op.finish_err(&e);
            map_err(e)
        }
    })
}

fn emit_state(app: &AppHandle, vault_path: &str, status: &str, error: Option<String>) {
    let _ = app.emit(
        "sync:state",
        SyncStateEvent {
            vault_path,
            status,
            error,
        },
    );
}

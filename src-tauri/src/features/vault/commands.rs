use crate::core::blocking::run_blocking;
use crate::core::error::{map_err, ApiResult, AppError};
use crate::core::log_util::{trunc, OpTimer};
use crate::features::vault::tree::VaultTreeNode;
use crate::features::vault::{self, tree, CreateVaultResult};
use std::path::PathBuf;
use tauri::{AppHandle, Runtime, State};
use tauri_plugin_fs::FsExt;

fn vault_path_arg(path: &str) -> Result<std::path::PathBuf, AppError> {
    let p = PathBuf::from(path.trim());
    if p.as_os_str().is_empty() {
        return Err(AppError::message("path is required"));
    }
    Ok(p)
}

/// Create / scaffold a Agentero vault at the given absolute path.
#[tauri::command]
pub async fn vault_create(path: String, locale: Option<String>) -> ApiResult<CreateVaultResult> {
    run_blocking(move || {
        let op = OpTimer::start_with("vault_create", format!("path={}", trunc(&path, 200)));
        let locale = vault::resolve_vault_locale(locale.as_deref().unwrap_or(""));
        match vault_path_arg(&path) {
            Ok(p) => op.finish_result(vault::create_vault(&p, locale)),
            Err(err) => {
                op.finish_err(&err);
                map_err(err)
            }
        }
    })
    .await
}

/// Ensure scaffold, seed missing bundled content, and safely update untouched
/// first-party skills.
///
/// Call on vault open so app updates can ship new `.agents/skills/*` or onboarding
/// content without requiring the user to re-run Create Vault. User-customized
/// files are never overwritten.
#[tauri::command]
pub async fn vault_ensure(path: String, locale: Option<String>) -> ApiResult<CreateVaultResult> {
    run_blocking(move || {
        let op = OpTimer::start_with("vault_ensure", format!("path={}", trunc(&path, 200)));
        let locale = vault::resolve_vault_locale(locale.as_deref().unwrap_or(""));
        match vault_path_arg(&path) {
            Ok(p) => op.finish_result(vault::ensure_vault(&p, locale)),
            Err(err) => {
                op.finish_err(&err);
                map_err(err)
            }
        }
    })
    .await
}

/// Extend the fs-plugin scope so the renderer can read/write this vault dir.
///
/// The dialog plugin grants runtime scope for a picked folder, but that grant
/// is not persisted. On startup restore a vault located outside the static
/// scope (`$HOME/**`, `$DOCUMENT/**`, …) would otherwise fail every fs-plugin
/// call with "forbidden path" until the user re-picks it. Called whenever a
/// local vault becomes active, before the file tree loads. Idempotent.
#[tauri::command]
pub fn vault_allow_fs_scope<R: Runtime>(app: AppHandle<R>, path: String) -> ApiResult<()> {
    let op = OpTimer::start_with(
        "vault_allow_fs_scope",
        format!("path={}", trunc(&path, 200)),
    );
    let p = match vault_path_arg(&path) {
        Ok(p) => p,
        Err(err) => {
            op.finish_err(&err);
            return map_err(err);
        }
    };
    match app.fs_scope().allow_directory(&p, true) {
        Ok(()) => op.finish_result(Ok(())),
        Err(e) => {
            let err = AppError::message(format!("allow fs scope failed: {e}"));
            op.finish_err(&err);
            map_err(err)
        }
    }
}

/// Release Host-side resources held for a vault the app switched away from.
///
/// The catalog connection cache is process-wide and keyed by vault root, and
/// the only existing eviction path is `with_catalog` noticing the database file
/// disappeared. Without this, every vault opened during a session keeps an open
/// SQLite handle (and its WAL) alive until exit. Safe while catalog work is in
/// flight: those callers hold an `Arc` clone, so this only drops the cache entry.
#[tauri::command]
pub async fn vault_release(path: String) -> ApiResult<()> {
    run_blocking(move || {
        let op = OpTimer::start_with("vault_release", format!("path={}", trunc(&path, 200)));
        match vault_path_arg(&path) {
            Ok(p) => {
                crate::features::catalog::evict_catalog_conn(&p);
                op.finish_result(Ok(()))
            }
            Err(err) => {
                op.finish_err(&err);
                map_err(err)
            }
        }
    })
    .await
}

/// Build the whole vault file tree in one pass (single IPC).
#[tauri::command]
pub async fn vault_tree_build(
    vault_path: String,
    caps: State<'_, crate::features::catalog::CapsCache>,
) -> Result<ApiResult<Vec<VaultTreeNode>>, String> {
    let caps = caps.inner().clone();
    Ok(run_blocking(move || {
        let op = OpTimer::start_with(
            "vault_tree_build",
            format!("vault={}", trunc(&vault_path, 200)),
        );
        let root = match crate::core::fs::resolve_vault(&vault_path) {
            Ok(root) => root,
            Err(err) => {
                op.finish_err(&err);
                return map_err(err);
            }
        };
        op.finish_result(Ok(tree::build_tree(&root, &caps)))
    })
    .await)
}

/// List one directory's children (lazy expand / targeted tree refresh).
#[tauri::command]
pub async fn vault_tree_children(
    vault_path: String,
    dir_path: String,
    caps: State<'_, crate::features::catalog::CapsCache>,
) -> Result<ApiResult<Vec<VaultTreeNode>>, String> {
    let caps = caps.inner().clone();
    Ok(run_blocking(move || {
        let op = OpTimer::start_with(
            "vault_tree_children",
            format!("dir={}", trunc(&dir_path, 200)),
        );
        let root = match vault_path_arg(&vault_path) {
            Ok(root) => root,
            Err(err) => {
                op.finish_err(&err);
                return map_err(err);
            }
        };
        let dir = match vault_path_arg(&dir_path) {
            Ok(dir) => dir,
            Err(err) => {
                op.finish_err(&err);
                return map_err(err);
            }
        };
        op.finish_result(tree::list_children(&root, &dir, &caps))
    })
    .await)
}

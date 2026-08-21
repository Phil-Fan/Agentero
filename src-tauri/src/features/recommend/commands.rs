//! Tauri commands for arXiv daily recommendation.

use super::{last_result, recommend, RecommendResult};
use crate::core::blocking::run_blocking;
use crate::core::error::{map_err, ApiResult, AppError};
use crate::features::settings::AppSettingsStore;
use serde::Deserialize;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecommendArxivArgs {
    pub vault_path: String,
    /// Categories to fetch; falls back to the last run, then the defaults.
    #[serde(default)]
    pub categories: Option<Vec<String>>,
    #[serde(default)]
    pub top_n: Option<usize>,
    /// Recompute even when today's stored run covers the same categories.
    #[serde(default)]
    pub force: bool,
}

/// Rank today's arXiv papers against the Vault library.
///
/// Reuses the stored same-day run unless `force` is set, so the vault-open
/// prewarm and repeated page opens stay free.
#[tauri::command]
pub async fn recommend_arxiv(
    app: AppHandle,
    args: RecommendArxivArgs,
) -> ApiResult<RecommendResult> {
    let vault = PathBuf::from(args.vault_path.trim());
    if !vault.is_dir() {
        return map_err(AppError::message("vault path is not a directory"));
    }
    // Read managed state before awaiting: the guard must not cross an await.
    let embedding = app.state::<AppSettingsStore>().embedding_config();
    match recommend(&vault, args.categories, args.top_n, args.force, embedding).await {
        Ok(result) => ApiResult::ok(result),
        Err(e) => map_err(e),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecommendArxivLastArgs {
    pub vault_path: String,
}

/// Stored recommendation run, if any — lets the page render before refreshing.
#[tauri::command]
pub async fn recommend_arxiv_last(
    args: RecommendArxivLastArgs,
) -> ApiResult<Option<RecommendResult>> {
    run_blocking(move || {
        let vault = PathBuf::from(args.vault_path.trim());
        if !vault.is_dir() {
            return map_err(AppError::message("vault path is not a directory"));
        }
        match last_result(&vault) {
            Ok(result) => ApiResult::ok(result),
            Err(e) => map_err(e),
        }
    })
    .await
}

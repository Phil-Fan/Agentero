//! `paper_coolpapers_notes` / `paper_coolpapers_import` — papers.cool commands.

use crate::core::error::{map_err, ApiResult, AppError};
use crate::core::log_util::{trunc, OpTimer};
use crate::features::import::AssetProgressContext;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoolPapersNotesArgs {
    pub vault_path: String,
    /// Vault-relative paper folder.
    pub path: String,
    /// Preferred resolver; falls back to `title` when absent.
    #[serde(default)]
    pub arxiv_id: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoolPapersImportArgs {
    pub vault_path: String,
    /// Vault-relative destination, e.g. `papers` or `papers/nlp`.
    pub parent_dir: String,
    /// papers.cool branch (`arxiv` / `venue`).
    pub branch: String,
    /// Row id, e.g. `36962@AAAI`.
    pub id: String,
    /// Background task to report asset-download progress into.
    #[serde(default)]
    pub task_id: Option<String>,
}

/// Import one papers.cool row straight from its page metadata.
///
/// Bypasses the Translator: the page's `citation_*` fields cover every branch
/// papers.cool aggregates and also carry the PDF URL.
#[tauri::command]
pub async fn paper_coolpapers_import(
    app: tauri::AppHandle,
    args: CoolPapersImportArgs,
) -> Result<ApiResult<crate::features::import::paper_import::PaperCommitResult>, String> {
    let op = OpTimer::start_with(
        "paper_coolpapers_import",
        format!("branch={} id={}", args.branch, trunc(&args.id, 80)),
    );
    let vault = PathBuf::from(args.vault_path.trim());
    if !vault.is_dir() {
        let err = AppError::message("vault path is not a directory");
        op.finish_err(&err);
        return Ok(map_err(err));
    }
    let result = super::page::import_page(super::page::ImportPageArgs {
        vault: &vault,
        parent_dir: &args.parent_dir,
        branch: &args.branch,
        id: &args.id,
        progress: AssetProgressContext {
            app: Some(&app),
            task_id: args.task_id.as_deref(),
        },
    })
    .await;
    Ok(op.finish_result(result))
}

/// Append the papers.cool Kimi analysis for one paper to its NOTES.md.
///
/// Resolves by arXiv id when available, else by exact title match. A paper that
/// cannot be resolved returns `found: false` and writes nothing.
#[tauri::command]
pub async fn paper_coolpapers_notes(
    args: CoolPapersNotesArgs,
) -> Result<ApiResult<super::CoolPapersNotes>, String> {
    let op = OpTimer::start_with(
        "paper_coolpapers_notes",
        format!(
            "path={} arxiv={}",
            trunc(&args.path, 120),
            args.arxiv_id.as_deref().unwrap_or("-")
        ),
    );
    let vault = PathBuf::from(args.vault_path.trim());
    if !vault.is_dir() {
        let err = AppError::message("vault path is not a directory");
        op.finish_err(&err);
        return Ok(map_err(err));
    }
    let result = super::fetch_notes(super::FetchNotesRequest {
        vault: &vault,
        paper_rel: &args.path,
        arxiv_id: args.arxiv_id.as_deref(),
        title: args.title.as_deref(),
    })
    .await;
    Ok(op.finish_result(result))
}

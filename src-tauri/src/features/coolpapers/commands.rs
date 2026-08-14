//! `paper_coolpapers_notes` — fetch a papers.cool Kimi analysis into NOTES.md.

use crate::core::error::{map_err, ApiResult, AppError};
use crate::core::log_util::{trunc, OpTimer};
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

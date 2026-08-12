//! `paper_refs_parse` / `paper_refs_list` / `paper_refs_graph` — reference commands.

use crate::core::error::{map_err, ApiResult, AppError};
use crate::core::log_util::{trunc, OpTimer};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaperRefsParseArgs {
    pub vault_path: String,
    pub path: String,
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaperRefsListArgs {
    pub vault_path: String,
    pub path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaperRefsGraphArgs {
    pub vault_path: String,
    /// Paper folder (or file under it) for neighborhood mode; omit / empty = full library graph.
    #[serde(default)]
    pub center: Option<String>,
    /// Undirected BFS hops over library-local cite edges (default 1).
    #[serde(default)]
    pub depth: Option<u32>,
}

/// Parse (or refresh with `force`) the reference sidecar for one paper. Online
/// reference lookup is always on; local bib/bbl parsing still runs.
#[tauri::command]
pub async fn paper_refs_parse(
    args: PaperRefsParseArgs,
) -> Result<ApiResult<super::CiteSidecar>, String> {
    let op = OpTimer::start_with(
        "paper_refs_parse",
        format!("path={} force={}", trunc(&args.path, 120), args.force),
    );
    let vault = PathBuf::from(args.vault_path.trim());
    if !vault.is_dir() {
        let err = AppError::message("vault path is not a directory");
        op.finish_err(&err);
        return Ok(map_err(err));
    }
    Ok(op.finish_result(super::parse_paper_refs(&vault, &args.path, true, args.force).await))
}

/// Read the existing reference sidecar; `None` when it has not been parsed yet.
#[tauri::command]
pub fn paper_refs_list(args: PaperRefsListArgs) -> ApiResult<Option<super::CiteSidecar>> {
    let op = OpTimer::start_with(
        "paper_refs_list",
        format!("path={}", trunc(&args.path, 120)),
    );
    let vault = PathBuf::from(args.vault_path.trim());
    if !vault.is_dir() {
        let err = AppError::message("vault path is not a directory");
        op.finish_err(&err);
        return map_err(err);
    }
    let rel = match crate::core::fs::sanitize_vault_rel(&args.path) {
        Ok(rel) => rel,
        Err(_) => {
            let err = AppError::message("invalid paper path");
            op.finish_err(&err);
            return map_err(err);
        }
    };
    let sidecar_path = vault.join(rel).join("source").join(super::SIDECAR_FILE);
    op.finish_result(Ok(super::read_sidecar(&sidecar_path)))
}

/// Citation relationship graph from existing reference sidecars + catalog matches.
#[tauri::command]
pub fn paper_refs_graph(args: PaperRefsGraphArgs) -> ApiResult<super::CiteGraphResponse> {
    let center_hint = args
        .center
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("-");
    let op = OpTimer::start_with(
        "paper_refs_graph",
        format!(
            "center={} depth={}",
            trunc(center_hint, 120),
            args.depth.unwrap_or(1)
        ),
    );
    let vault = PathBuf::from(args.vault_path.trim());
    if !vault.is_dir() {
        let err = AppError::message("vault path is not a directory");
        op.finish_err(&err);
        return map_err(err);
    }
    let center = args
        .center
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    op.finish_result(super::build_citation_graph(&vault, center, args.depth))
}

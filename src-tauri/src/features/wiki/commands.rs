use crate::core::error::{map_err, ApiResult, AppError};
use crate::features::wiki::heading_rename::run_heading_rename_transaction;
use crate::features::wiki::models::{
    BacklinksResponse, InternalLinkSyntax, OutgoingLinksResponse, RebuildResult, WikiEmbedResponse,
    WikiRenameHeadingResult, WikiResolveResponse, WikiSearchCandidate, WikiSearchCandidateKind,
};
use crate::features::wiki::WikiIndexState;
use std::path::PathBuf;
use tauri::State;

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WikiRenameHeadingArgs {
    pub vault_path: String,
    pub path: String,
    pub heading_path: Vec<String>,
    pub heading_line: u32,
    pub expected_content: String,
    pub new_text: String,
    #[serde(default)]
    pub dirty_paths: Vec<String>,
}

#[tauri::command]
pub fn graph_get_backlinks(
    index: State<'_, WikiIndexState>,
    vault_path: String,
    path: String,
) -> ApiResult<BacklinksResponse> {
    let mut guard = match index.inner.lock() {
        Ok(g) => g,
        Err(e) => return map_err(AppError::message(format!("wiki index lock: {e}"))),
    };
    if let Err(e) = guard.ensure_vault(&vault_path) {
        return map_err(AppError::message(e));
    }
    ApiResult::ok(guard.get_backlinks(&vault_path, &path))
}

/// Return every explicit occurrence authored by `path`, including unresolved and
/// invalid-fragment diagnostics. This is intentionally separate from Graph, whose
/// file-level projection may deduplicate edges.
#[tauri::command]
pub fn wiki_get_outgoing(
    index: State<'_, WikiIndexState>,
    vault_path: String,
    path: String,
) -> ApiResult<OutgoingLinksResponse> {
    let mut guard = match index.inner.lock() {
        Ok(g) => g,
        Err(e) => return map_err(AppError::message(format!("wiki index lock: {e}"))),
    };
    if let Err(e) = guard.ensure_vault(&vault_path) {
        return map_err(AppError::message(e));
    }
    ApiResult::ok(guard.get_outgoing(&vault_path, &path))
}

#[tauri::command]
pub fn wiki_resolve(
    index: State<'_, WikiIndexState>,
    vault_path: String,
    source_path: String,
    link_text: String,
    syntax: Option<InternalLinkSyntax>,
) -> ApiResult<WikiResolveResponse> {
    let mut guard = match index.inner.lock() {
        Ok(g) => g,
        Err(e) => return map_err(AppError::message(format!("wiki index lock: {e}"))),
    };
    if let Err(e) = guard.ensure_vault(&vault_path) {
        return map_err(AppError::message(e));
    }
    ApiResult::ok(guard.resolve_text(
        &vault_path,
        &source_path,
        &link_text,
        syntax.unwrap_or(InternalLinkSyntax::Wikilink),
    ))
}

/// Resolve and read the exact source projection for one `![[...]]` embed.
#[tauri::command]
pub fn wiki_embed_read(
    index: State<'_, WikiIndexState>,
    vault_path: String,
    source_path: String,
    link_text: String,
) -> ApiResult<WikiEmbedResponse> {
    let mut guard = match index.inner.lock() {
        Ok(g) => g,
        Err(e) => return map_err(AppError::message(format!("wiki index lock: {e}"))),
    };
    if let Err(e) = guard.ensure_vault(&vault_path) {
        return map_err(AppError::message(e));
    }
    match guard.read_embed(&vault_path, &source_path, &link_text) {
        Ok(response) => ApiResult::ok(response),
        Err(error) => map_err(AppError::message(error)),
    }
}

#[tauri::command]
pub fn wiki_search(
    index: State<'_, WikiIndexState>,
    vault_path: String,
    query: String,
    path: Option<String>,
    kind: Option<WikiSearchCandidateKind>,
) -> ApiResult<Vec<WikiSearchCandidate>> {
    let mut guard = match index.inner.lock() {
        Ok(g) => g,
        Err(e) => return map_err(AppError::message(format!("wiki index lock: {e}"))),
    };
    if let Err(e) = guard.ensure_vault(&vault_path) {
        return map_err(AppError::message(e));
    }
    ApiResult::ok(guard.search_scoped(&query, path.as_deref(), kind.as_ref()))
}

/// Explicitly rename one saved heading and rewrite every resolved inbound
/// heading fragment as one rollback-capable local transaction.
#[tauri::command]
pub fn wiki_rename_heading(
    args: WikiRenameHeadingArgs,
    index: State<'_, WikiIndexState>,
) -> ApiResult<WikiRenameHeadingResult> {
    let vault = PathBuf::from(&args.vault_path);
    if !vault.is_dir() {
        return map_err(AppError::message("vault path is not a directory"));
    }
    let mut guard = match index.inner.lock() {
        Ok(guard) => guard,
        Err(error) => return map_err(AppError::message(format!("wiki index lock: {error}"))),
    };
    match run_heading_rename_transaction(
        &vault,
        &mut guard,
        &args.path,
        &args.heading_path,
        args.heading_line,
        &args.expected_content,
        &args.new_text,
        &args.dirty_paths,
    ) {
        Ok(result) => ApiResult::ok(result),
        Err(error) => ApiResult::err_with_details(
            AppError::message(error.to_string()),
            serde_json::json!({
                "code": error.code,
                "rollback": error.rollback,
                "paths": error.paths,
            }),
        ),
    }
}

#[tauri::command]
pub fn graph_rebuild(
    index: State<'_, WikiIndexState>,
    vault_path: String,
) -> ApiResult<RebuildResult> {
    use crate::core::log_util::OpTimer;

    let op = OpTimer::start("graph_rebuild");
    let mut guard = match index.inner.lock() {
        Ok(g) => g,
        Err(e) => {
            let err = AppError::message(format!("wiki index lock: {e}"));
            op.finish_err(&err);
            return map_err(err);
        }
    };
    match guard.rebuild(&vault_path) {
        Ok(r) => {
            op.finish_ok();
            ApiResult::ok(r)
        }
        Err(e) => {
            let err = AppError::message(e);
            op.finish_err(&err);
            map_err(err)
        }
    }
}

/// Internal diagnostic: remove the derived snapshot and rebuild it from Vault files.
#[tauri::command]
pub fn wiki_cache_rebuild(
    index: State<'_, WikiIndexState>,
    vault_path: String,
) -> ApiResult<RebuildResult> {
    let mut guard = match index.inner.lock() {
        Ok(guard) => guard,
        Err(error) => {
            return map_err(AppError::message(format!("wiki index lock: {error}")));
        }
    };
    match guard.rebuild_fresh(&vault_path) {
        Ok(result) => ApiResult::ok(result),
        Err(error) => map_err(AppError::message(error)),
    }
}

//! Magic-wand / identifier import commands.

use crate::core::error::ApiResult;
use crate::core::fs::WriteOpts;
use crate::core::log_util::{trunc, OpTimer};
use crate::features::catalog::CapsCache;
use crate::features::import::pdf_parse::{PaperParseBodyArgs, PaperParseResult};
use crate::features::import::{
    AssetDownloadResult, ImportLocalPdfArgs, ImportLocalPdfResult, LookupImportBatchArgs,
    LookupImportBatchResult, PaperDownloadAssetsArgs, SkillImportResult, StageImportFileArgs,
    StageImportFileResult,
};
use crate::features::remote::{import_bridge, parse_remote_handle, RemoteRegistry};
use serde::Deserialize;
use std::sync::Arc;
use tauri::State;

/// Batch resolve identifiers and write papers into vault.
/// Deduplicates within the batch and against existing catalog entries.
#[tauri::command]
pub async fn lookup_import_batch(
    app: tauri::AppHandle,
    registry: State<'_, Arc<RemoteRegistry>>,
    cache: State<'_, CapsCache>,
    args: LookupImportBatchArgs,
) -> Result<ApiResult<LookupImportBatchResult>, String> {
    let n = args.texts.len();
    let op = OpTimer::start_with("lookup_import_batch", format!("count={n}"));
    let note_mode = crate::features::import::note_mode_from_app(&app);
    if let Some(session_id) = parse_remote_handle(&args.vault_path) {
        let session = match registry.get(session_id).await {
            Ok(s) => s,
            Err(e) => {
                op.finish_err(&e);
                return Ok(crate::core::error::map_err(e));
            }
        };
        let vault_id = std::path::PathBuf::from(&args.vault_path);
        let result =
            import_bridge::import_by_identifier_batch_remote(session, args, note_mode).await;
        if let Ok(r) = &result {
            for paper in &r.imported {
                crate::features::lifecycle::emit_paper_imported(Some(&app), &vault_id, &paper.id);
            }
        }
        return Ok(op.finish_result(result));
    }
    let task_id = args.task_id.clone();
    let result = super::import_by_identifier_batch(args, Some(&app), Some(&cache), note_mode).await;
    if let Some(task_id) = task_id.as_deref() {
        crate::core::background_tasks::finish(task_id);
    }
    Ok(op.finish_result(result))
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillInstallArgs {
    pub vault_path: String,
    pub discovery_id: String,
    #[serde(default)]
    pub selected_names: Vec<String>,
    #[serde(default)]
    pub task_id: Option<String>,
}

#[tauri::command]
pub fn skill_install(args: SkillInstallArgs) -> ApiResult<Vec<SkillImportResult>> {
    let op = OpTimer::start_with(
        "skill_install",
        format!("discovery_id={}", trunc(&args.discovery_id, 40)),
    );
    let result = super::install_discovered_skills(
        std::path::Path::new(&args.vault_path),
        &args.discovery_id,
        &args.selected_names,
    );
    op.finish_result(result)
}

#[tauri::command]
pub fn skill_discard(discovery_id: String) -> ApiResult<()> {
    let op = OpTimer::start_with(
        "skill_discard",
        format!("discovery_id={}", trunc(&discovery_id, 40)),
    );
    op.finish_result(super::discard_skill_discovery(&discovery_id))
}

/// Download PDF (+ arXiv LaTeX) for an existing paper folder that is missing local assets.
/// When no TeX remains after download, also tries liteparse → PAPER.md.
#[tauri::command]
pub async fn paper_download_assets(
    app: tauri::AppHandle,
    registry: State<'_, Arc<RemoteRegistry>>,
    cache: State<'_, CapsCache>,
    args: PaperDownloadAssetsArgs,
) -> Result<ApiResult<AssetDownloadResult>, String> {
    let path = trunc(&args.path, 120);
    let op = OpTimer::start_with("paper_download_assets", format!("path={path}"));
    if let Some(session_id) = parse_remote_handle(&args.vault_path) {
        let session = match registry.get(session_id).await {
            Ok(s) => s,
            Err(e) => {
                op.finish_err(&e);
                return Ok(crate::core::error::map_err(e));
            }
        };
        return Ok(
            op.finish_result(import_bridge::download_paper_assets_remote(session, args).await)
        );
    }
    let task_id = args.task_id.clone();
    let result = super::download_paper_assets_with_progress(args, Some(&app), Some(&cache)).await;
    if let Some(task_id) = task_id.as_deref() {
        crate::core::background_tasks::finish(task_id);
    }
    Ok(op.finish_result(result))
}

/// Import local PDF file(s) into the vault as paper folders (copy + catalog + liteparse).
#[tauri::command]
pub async fn paper_import_local_pdf(
    app: tauri::AppHandle,
    registry: State<'_, Arc<RemoteRegistry>>,
    cache: State<'_, CapsCache>,
    args: ImportLocalPdfArgs,
) -> Result<ApiResult<ImportLocalPdfResult>, String> {
    let n = args.file_paths.len();
    let op = OpTimer::start_with("paper_import_local_pdf", format!("count={n}"));
    let note_mode = crate::features::import::note_mode_from_app(&app);
    let task_id = args.task_id.clone();
    let result = if let Some(session_id) = parse_remote_handle(&args.vault_path) {
        let session = match registry.get(session_id).await {
            Ok(s) => s,
            Err(e) => {
                if let Some(task_id) = task_id.as_deref() {
                    crate::core::background_tasks::finish(task_id);
                }
                op.finish_err(&e);
                return Ok(crate::core::error::map_err(e));
            }
        };
        let vault_id = std::path::PathBuf::from(&args.vault_path);
        let result = import_bridge::import_local_pdfs_remote(session, args, note_mode).await;
        if let Ok(r) = &result {
            for paper in &r.papers {
                crate::features::lifecycle::emit_paper_imported(Some(&app), &vault_id, &paper.id);
            }
        }
        result
    } else {
        super::import_local_pdfs(args, Some(&app), Some(&cache), note_mode).await
    };
    if let Some(task_id) = task_id.as_deref() {
        crate::core::background_tasks::finish(task_id);
    }
    Ok(op.finish_result_ok_extra(result, |r| {
        format!("imported={} errors={}", r.papers.len(), r.errors.len())
    }))
}

/// Parse a paper's local PDF into `PAPER.md` using liteparse.
/// Runs as a standalone background task; `task_id` is used for cancellation.
#[tauri::command]
pub async fn paper_parse_body(
    registry: State<'_, Arc<RemoteRegistry>>,
    cache: State<'_, CapsCache>,
    args: PaperParseBodyArgs,
) -> Result<ApiResult<PaperParseResult>, String> {
    let path = trunc(&args.path, 120);
    let op = OpTimer::start_with("paper_parse_body", format!("path={path}"));

    if let Some(session_id) = parse_remote_handle(&args.vault_path) {
        let session = match registry.get(session_id).await {
            Ok(s) => s,
            Err(e) => {
                if let Some(task_id) = args.task_id.as_deref() {
                    crate::core::background_tasks::finish(task_id);
                }
                op.finish_err(&e);
                return Ok(crate::core::error::map_err(e));
            }
        };
        let task_id = args.task_id.clone();
        let result = parse_remote_body(session, args).await;
        if let Some(task_id) = task_id.as_deref() {
            crate::core::background_tasks::finish(task_id);
        }
        return Ok(op.finish_result(result));
    }

    let task_id = args.task_id.clone();
    let result = crate::features::import::pdf_parse::parse_paper_body(args, Some(&cache)).await;
    if let Some(task_id) = task_id.as_deref() {
        crate::core::background_tasks::finish(task_id);
    }
    Ok(op.finish_result(result))
}

async fn parse_remote_body(
    session: Arc<crate::features::remote::session::RemoteSession>,
    args: PaperParseBodyArgs,
) -> Result<PaperParseResult, crate::core::error::AppError> {
    let path_rel = crate::core::fs::sanitize_vault_rel(&args.path)
        .map_err(|_| crate::core::error::AppError::message("invalid paper path"))?;
    let staging = session.work_root.join(&path_rel);

    let local_args = PaperParseBodyArgs {
        vault_path: session.work_root.to_string_lossy().to_string(),
        path: path_rel.clone(),
        force: args.force,
        task_id: args.task_id.clone(),
    };

    let result = crate::features::import::pdf_parse::parse_paper_body(local_args, None).await?;

    if result.paper_md {
        let paper_md_local = staging.join("PAPER.md");
        if paper_md_local.is_file() {
            let bytes = std::fs::read(&paper_md_local).map_err(|e| {
                crate::core::error::AppError::message(format!("read staged PAPER.md: {e}"))
            })?;
            session
                .fs
                .write(
                    &format!("{path_rel}/PAPER.md"),
                    &bytes,
                    WriteOpts {
                        create_parents: true,
                    },
                )
                .await?;
        }
        let mut cat = session.catalog.lock().await;
        cat.push(session.fs.clone()).await?;
    }

    Ok(result)
}

/// Stage a path-less OS drop (File bytes as base64) into `~/.agentero/import-tmp/`.
#[tauri::command]
pub async fn paper_stage_import_file(
    args: StageImportFileArgs,
) -> ApiResult<StageImportFileResult> {
    crate::core::blocking::run_blocking(move || {
        let name = trunc(&args.file_name, 80);
        let op = OpTimer::start_with("paper_stage_import_file", format!("name={name}"));
        op.finish_result(super::stage_import_file(args))
    })
    .await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaperResolveIdentifierArgs {
    /// DOI / arXiv id / URL text.
    pub text: String,
    #[serde(default)]
    pub translator_base_url: Option<String>,
}

/// Resolve an identifier (DOI/arXiv) to metadata without importing — backs
/// Edit Metadata's identifier refresh.
#[tauri::command]
pub async fn paper_resolve_identifier(
    args: PaperResolveIdentifierArgs,
) -> ApiResult<super::PaperMeta> {
    let text = trunc(args.text.trim(), 60);
    let op = OpTimer::start_with("paper_resolve_identifier", format!("text={text}"));
    let base = args
        .translator_base_url
        .clone()
        .unwrap_or_else(|| super::DEFAULT_TRANSLATOR_BASE_URL.to_string());
    match super::pdf_recognize::resolve_identifier_full(&args.text, &base, None).await {
        Ok((mut meta, _used_translator)) => {
            super::enrich_remote_urls(&mut meta);
            op.finish_ok();
            ApiResult::ok(meta)
        }
        Err(e) => {
            op.finish_err(&e);
            crate::core::error::map_err(e)
        }
    }
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotesTemplateSeedResult {
    pub created: bool,
}

/// Seed `{vault}/.agentero/templates/NOTES.md` with a starting template for
/// the `custom` paper-note mode. Never overwrites an existing template.
#[tauri::command]
pub fn notes_template_seed(vault_path: String) -> ApiResult<NotesTemplateSeedResult> {
    let op = OpTimer::start_with(
        "notes_template_seed",
        format!("vault={}", trunc(&vault_path, 120)),
    );
    let result = crate::core::fs::resolve_vault(&vault_path)
        .and_then(|vault| super::seed_notes_template(&vault))
        .map(|created| NotesTemplateSeedResult { created });
    op.finish_result(result)
}

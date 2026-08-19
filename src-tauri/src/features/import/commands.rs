//! Magic-wand / identifier import commands + catalog export/import via Translator.

use crate::core::error::ApiResult;
use crate::core::fs::WriteOpts;
use crate::core::log_util::{trunc, OpTimer};
use crate::features::catalog::CapsCache;
use crate::features::import::pdf_parse::{PaperParseBodyArgs, PaperParseResult};
use crate::features::import::{
    AssetDownloadResult, ImportLocalPdfArgs, ImportLocalPdfResult, LookupImportBatchArgs,
    LookupImportBatchResult, PaperDownloadAssetsArgs, PaperExportArgs, PaperExportResult,
    PaperImportArgs, PaperImportResult, SkillImportResult, StageImportFileArgs,
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
    if let Some(session_id) = parse_remote_handle(&args.vault_path) {
        let session = match registry.get(session_id).await {
            Ok(s) => s,
            Err(e) => {
                op.finish_err(&e);
                return Ok(crate::core::error::map_err(e));
            }
        };
        let vault_id = std::path::PathBuf::from(&args.vault_path);
        let result = import_bridge::import_by_identifier_batch_remote(session, args).await;
        if let Ok(r) = &result {
            for paper in &r.imported {
                crate::features::lifecycle::emit_paper_imported(Some(&app), &vault_id, &paper.id);
            }
        }
        return Ok(op.finish_result(result));
    }
    let task_id = args.task_id.clone();
    let result = super::import_by_identifier_batch(args, Some(&app), Some(&cache)).await;
    if let Some(task_id) = task_id.as_deref() {
        crate::features::agent::background_tasks::finish(task_id);
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
        crate::features::agent::background_tasks::finish(task_id);
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
    let task_id = args.task_id.clone();
    let result = if let Some(session_id) = parse_remote_handle(&args.vault_path) {
        let session = match registry.get(session_id).await {
            Ok(s) => s,
            Err(e) => {
                if let Some(task_id) = task_id.as_deref() {
                    crate::features::agent::background_tasks::finish(task_id);
                }
                op.finish_err(&e);
                return Ok(crate::core::error::map_err(e));
            }
        };
        let vault_id = std::path::PathBuf::from(&args.vault_path);
        let result = import_bridge::import_local_pdfs_remote(session, args).await;
        if let Ok(r) = &result {
            for paper in &r.papers {
                crate::features::lifecycle::emit_paper_imported(Some(&app), &vault_id, &paper.id);
            }
        }
        result
    } else {
        super::import_local_pdfs(args, Some(&app), Some(&cache)).await
    };
    if let Some(task_id) = task_id.as_deref() {
        crate::features::agent::background_tasks::finish(task_id);
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
                    crate::features::agent::background_tasks::finish(task_id);
                }
                op.finish_err(&e);
                return Ok(crate::core::error::map_err(e));
            }
        };
        let task_id = args.task_id.clone();
        let result = parse_remote_body(session, args).await;
        if let Some(task_id) = task_id.as_deref() {
            crate::features::agent::background_tasks::finish(task_id);
        }
        return Ok(op.finish_result(result));
    }

    let task_id = args.task_id.clone();
    let result = crate::features::import::pdf_parse::parse_paper_body(args, Some(&cache)).await;
    if let Some(task_id) = task_id.as_deref() {
        crate::features::agent::background_tasks::finish(task_id);
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
pub struct PaperProbePdfIdentArgs {
    /// Absolute local PDF paths.
    pub file_paths: Vec<String>,
    #[serde(default)]
    pub translator_base_url: Option<String>,
}

/// Recognize local PDFs (liteparse probe → Zotero recognizer → identifier
/// resolution) to prefill the import confirm dialog. Best-effort: failures
/// return per-file `error` rows instead of failing the whole batch.
#[tauri::command]
pub async fn paper_probe_pdf_ident(
    args: PaperProbePdfIdentArgs,
) -> ApiResult<Vec<super::pdf_recognize::PdfIdentProbe>> {
    let n = args.file_paths.len();
    let op = OpTimer::start_with("paper_probe_pdf_ident", format!("count={n}"));
    let base = args
        .translator_base_url
        .clone()
        .unwrap_or_else(|| super::DEFAULT_TRANSLATOR_BASE_URL.to_string());

    let mut results = Vec::with_capacity(n);
    for file_path in &args.file_paths {
        let path = std::path::PathBuf::from(file_path.trim());
        let probe = if !path.is_file() {
            super::pdf_recognize::PdfIdentProbe::error(file_path, "file not found".into())
        } else {
            super::pdf_recognize::recognize_and_resolve(&path, &base, None).await
        };
        results.push(probe);
    }
    let ok = results.iter().filter(|r| r.status == "ok").count();
    let out = ApiResult::ok(results);
    op.finish_ok_extra(format!("ok={ok} total={n}"));
    out
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
/// the import dialog's Fetch button and Edit Metadata's refresh.
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

/// Export catalog papers via Translator `POST /export` (Zotero JSON array → BibTeX/RIS/…).
#[tauri::command]
pub async fn paper_export(args: PaperExportArgs) -> ApiResult<PaperExportResult> {
    let format = args.format.as_deref().unwrap_or("bibtex");
    let op = OpTimer::start_with("paper_export", format!("format={format}"));
    op.finish_result(super::export_catalog(args).await)
}

/// Import BibTeX/RIS/… via Translator `POST /import`, write papers into vault + catalog.
#[tauri::command]
pub async fn paper_import(
    app: tauri::AppHandle,
    registry: State<'_, Arc<RemoteRegistry>>,
    args: PaperImportArgs,
) -> Result<ApiResult<PaperImportResult>, String> {
    let op = OpTimer::start("paper_import");
    if let Some(session_id) = parse_remote_handle(&args.vault_path) {
        let session = match registry.get(session_id).await {
            Ok(s) => s,
            Err(e) => {
                op.finish_err(&e);
                return Ok(crate::core::error::map_err(e));
            }
        };
        return Ok(op.finish_result_ok_extra(
            import_bridge::import_catalog_remote(session, args).await,
            |r| format!("imported={} skipped={}", r.imported, r.skipped),
        ));
    }
    Ok(
        op.finish_result_ok_extra(super::import_catalog(args, Some(&app)).await, |r| {
            format!("imported={} skipped={}", r.imported, r.skipped)
        }),
    )
}

//! Paper import: identifier lookup, Translator, Zotero migrate, local PDF, PAPER.md parse.
//!
//! @see docs/backend/identifier-lookup.md
//! @see docs/backend/paper-import-pipeline.md

#[cfg(feature = "desktop")]
pub mod commands;
pub mod paper_import;
pub mod pdf_parse;
#[cfg(feature = "desktop")]
pub mod zotero_commands;

mod assets;
pub(crate) mod batch;
pub(crate) mod map;
pub(crate) mod parse;
pub(crate) mod pdf_recognize;
mod skill_import;
pub(crate) mod title_search;
#[cfg(feature = "desktop")]
pub(crate) mod zotero_db;
pub(crate) mod zotero_io;

pub use crate::features::catalog::{has_local_pdf, has_local_tex};
pub use assets::{
    ensure_paper_assets, ensure_paper_assets_with_cookies, ensure_paper_assets_with_progress,
    AssetDownloadResult, AssetProgressContext,
};
pub use map::{enrich_remote_urls, map_zotero_item, PaperMeta};
pub use skill_import::{
    discard_skill_discovery, discover_skill_source, install_discovered_skills, SkillCandidate,
    SkillDiscovery, SkillImportResult,
};
pub use title_search::{PaperSearchCandidate, PaperSearchGroup};
#[cfg(feature = "desktop")]
pub use zotero_db::{
    migrate_zotero, scan_zotero, MigrateProgress, ZoteroMigrateArgs, ZoteroMigrateResult,
    ZoteroScan, ZoteroScanArgs,
};
pub use zotero_io::{
    export_catalog, import_catalog, PaperExportArgs, PaperExportResult, PaperImportArgs,
    PaperImportResult,
};

use crate::core::error::AppError;
use crate::features::catalog::{
    papers::{self, PaperRecord},
    CapsCache,
};
#[cfg(feature = "desktop")]
use crate::features::import::assets::AssetDownloadProgress;
use futures_util::StreamExt;
use map::local_pdf_meta;
use parse::{extract_primary_identifier, IdentifierKind};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
#[cfg(feature = "desktop")]
use tauri::{AppHandle, Emitter};
#[cfg(not(feature = "desktop"))]
pub struct AppHandle;
use tokio::sync::Mutex;

/// Public helper for remote PDF import staging.
#[cfg_attr(not(feature = "desktop"), allow(dead_code))]
pub(crate) fn local_pdf_meta_for_import(id: String, title: String) -> PaperMeta {
    local_pdf_meta(id, title)
}

/// Default Translator Runtime base URL (hosted service).
/// Override via Settings → `translatorBaseUrl` / `LookupImportArgs.translator_base_url`.
pub const DEFAULT_TRANSLATOR_BASE_URL: &str = "https://translator.philfan.cn";

/// Prefix for Zotero tags that are not user-created or are otherwise internal.
pub const ZOTERO_INTERNAL_TAG_PREFIX: &str = "@zotero:";

/// Upper bound for the network asset phase of one paper import.
///
/// Individual requests have shorter reqwest timeouts, but an import may try
/// several PDF fallbacks before fetching the arXiv source. Keep the whole
/// phase bounded so one paper cannot hold an import task indefinitely.
pub const PAPER_ASSET_TIMEOUT: Duration = Duration::from_secs(3 * 60);

pub(crate) fn check_task_not_cancelled(task_id: Option<&str>) -> Result<(), AppError> {
    if task_id.is_some_and(is_background_task_cancelled) {
        return Err(AppError::message("background task cancelled"));
    }
    Ok(())
}

#[cfg(feature = "desktop")]
pub(crate) fn is_background_task_cancelled(task_id: &str) -> bool {
    crate::features::agent::background_tasks::is_cancelled(task_id)
}

#[cfg(not(feature = "desktop"))]
pub(crate) fn is_background_task_cancelled(_task_id: &str) -> bool {
    false
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LookupImportArgs {
    pub vault_path: String,
    /// Vault-relative parent, e.g. `papers` or `papers/nlp`.
    pub parent_dir: String,
    pub text: String,
    /// Optional override; empty → [`DEFAULT_TRANSLATOR_BASE_URL`].
    #[serde(default)]
    pub translator_base_url: Option<String>,
    /// Frontend background-task id for byte-level download progress events.
    #[serde(default)]
    pub task_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaperDownloadAssetsArgs {
    pub vault_path: String,
    /// Vault-relative paper folder, e.g. `papers/1706.03762`.
    pub path: String,
    /// Frontend background-task id for byte-level download progress events.
    #[serde(default)]
    pub task_id: Option<String>,
}

/// Per-file overrides when importing a local PDF (metadata confirm dialog).
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LocalPdfImportEntry {
    pub file_path: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub authors: Option<Vec<String>>,
    #[serde(default)]
    pub year: Option<i32>,
    #[serde(default)]
    pub doi: Option<String>,
    #[serde(default)]
    pub arxiv_id: Option<String>,
    /// Structured fields fetched via identifier resolution in the dialog
    /// (publication/volume/issue/pages/abstract/…). Applied only when present.
    #[serde(default)]
    pub extra: Option<LocalPdfExtraMeta>,
}

/// Non-editable structured metadata carried from the confirm dialog's
/// identifier fetch into the catalog row.
#[derive(Debug, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct LocalPdfExtraMeta {
    #[serde(default)]
    pub publication: Option<String>,
    #[serde(default)]
    pub volume: Option<String>,
    #[serde(default)]
    pub issue: Option<String>,
    #[serde(default)]
    pub pages: Option<String>,
    #[serde(default)]
    pub publisher: Option<String>,
    #[serde(default)]
    pub issn: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub date: Option<String>,
    #[serde(rename = "abstract", default)]
    pub abstract_text: Option<String>,
}

/// Stage a dropped PDF (path-less WKWebView drop) into `~/.agentero/import-tmp/`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StageImportFileArgs {
    /// Original filename (used for stem + safe on-disk name).
    pub file_name: String,
    /// Standard base64 of PDF bytes (no `data:` prefix).
    pub content_base64: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StageImportFileResult {
    /// Absolute path written on disk.
    pub path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportLocalPdfArgs {
    pub vault_path: String,
    /// Vault-relative parent, e.g. `papers` or `papers/nlp`.
    pub parent_dir: String,
    /// Absolute paths to local PDF files (picker). Ignored when `entries` is non-empty.
    #[serde(default)]
    pub file_paths: Vec<String>,
    /// Preferred: path + optional title/authors/year/identifiers from the confirm dialog.
    #[serde(default)]
    pub entries: Vec<LocalPdfImportEntry>,
    /// Frontend background-task id for parse-phase progress.
    #[serde(default)]
    pub task_id: Option<String>,
    /// Translator base URL for identifier resolution during background
    /// recognition (entries without dialog metadata). Empty → default.
    #[serde(default)]
    pub translator_base_url: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportLocalPdfResult {
    /// One entry per successfully imported PDF.
    pub papers: Vec<LookupImportResult>,
    /// `"<file>: <reason>"` for each file that failed to import.
    #[serde(default)]
    pub errors: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LookupImportResult {
    pub paper_dir: String,
    pub path: String,
    pub id: String,
    pub title: String,
    pub used_translator: bool,
    pub translator_base_url: String,
    /// Whether local PDF was present after import download attempt.
    #[serde(default)]
    pub pdf: bool,
    /// Whether local TeX was present after import download attempt.
    #[serde(default)]
    pub tex: bool,
    /// Whether PAPER.md was written (no-TeX liteparse path).
    #[serde(default)]
    pub paper_md: bool,
    /// Download / parse messages (for UI warnings).
    #[serde(default)]
    pub asset_messages: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LookupImportBatchArgs {
    pub vault_path: String,
    pub parent_dir: String,
    pub texts: Vec<String>,
    #[serde(default)]
    pub translator_base_url: Option<String>,
    #[serde(default)]
    pub task_id: Option<String>,
    /// Max concurrent imports; 0 or 1 means sequential.
    #[serde(default)]
    pub concurrency: Option<usize>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedImport {
    pub raw: String,
    pub kind: String,
    pub value: String,
    pub reason: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LookupImportBatchResult {
    pub imported: Vec<LookupImportResult>,
    #[serde(default)]
    pub skills: Vec<SkillImportResult>,
    #[serde(default)]
    pub skill_candidates: Vec<SkillDiscovery>,
    /// Free-text inputs resolved to importable candidates awaiting user choice.
    #[serde(default)]
    pub search_candidates: Vec<PaperSearchGroup>,
    #[serde(default)]
    pub skipped: Vec<SkippedImport>,
    #[serde(default)]
    pub errors: Vec<String>,
}

pub async fn import_by_identifier(args: LookupImportArgs) -> Result<LookupImportResult, AppError> {
    import_by_identifier_with_progress(args, None, None).await
}

pub async fn import_by_identifier_with_progress(
    args: LookupImportArgs,
    app: Option<&AppHandle>,
    cache: Option<&CapsCache>,
) -> Result<LookupImportResult, AppError> {
    use crate::features::import::paper_import::{
        paper_commit, AssetsPolicy, DedupePolicy, PaperCommitOptions,
    };

    let vault = PathBuf::from(args.vault_path.trim());
    if !vault.is_dir() {
        return Err(AppError::message("vault path is not a directory"));
    }

    let base = args
        .translator_base_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_TRANSLATOR_BASE_URL)
        .trim_end_matches('/')
        .to_string();

    let text = args.text.trim();
    if text.is_empty() {
        return Err(AppError::message("identifier text is empty"));
    }

    check_task_not_cancelled(args.task_id.as_deref())?;
    let (mut meta, used_translator) =
        resolve_metadata(text, &base, args.task_id.as_deref()).await?;
    check_task_not_cancelled(args.task_id.as_deref())?;
    enrich_remote_urls(&mut meta);

    let commit = paper_commit(
        meta,
        PaperCommitOptions {
            vault: &vault,
            parent_dir: &args.parent_dir,
            dedupe: DedupePolicy::ByCatalogId,
            assets: AssetsPolicy::SyncDownload {
                cookies: None,
                progress: AssetProgressContext {
                    app,
                    task_id: args.task_id.as_deref(),
                },
            },
            translate_abstract: true,
            fresh_timestamps: false,
            cache,
            app,
        },
    )
    .await?;
    check_task_not_cancelled(args.task_id.as_deref())?;

    Ok(LookupImportResult {
        paper_dir: commit.paper_dir,
        path: commit.path,
        id: commit.id,
        title: commit.title,
        used_translator,
        translator_base_url: base,
        pdf: commit.pdf,
        tex: commit.tex,
        paper_md: commit.paper_md,
        asset_messages: commit.asset_messages,
    })
}

/// Batch import multiple identifiers with deduplication.
/// Progress events are emitted under the same `task_id` so the frontend sees
/// a single background task for the whole batch.
pub async fn import_by_identifier_batch(
    args: LookupImportBatchArgs,
    app: Option<&AppHandle>,
    cache: Option<&CapsCache>,
) -> Result<LookupImportBatchResult, AppError> {
    let vault = PathBuf::from(args.vault_path.trim());
    if !vault.is_dir() {
        return Err(AppError::message("vault path is not a directory"));
    }

    let skills: Vec<SkillImportResult> = Vec::new();
    let mut skill_candidates: Vec<SkillDiscovery> = Vec::new();
    let mut preflight = batch::preflight_identifier_batch(
        &args.texts,
        &vault,
        batch::SkillBatchMode::Collect,
        false,
    );

    for pending in &preflight.skills {
        match discover_skill_source(&vault, &pending.source, app, args.task_id.as_deref()).await {
            Ok(discovery) => skill_candidates.push(discovery),
            Err(e) => preflight.errors.push(format!("{}: {e}", pending.raw)),
        }
    }

    let search_candidates = resolve_search_queries(&preflight.queries, &mut preflight.errors).await;

    let to_import: Vec<(String, LookupImportArgs)> = preflight
        .papers
        .into_iter()
        .map(|pending| {
            let raw = pending.raw;
            (
                raw.clone(),
                LookupImportArgs {
                    vault_path: args.vault_path.clone(),
                    parent_dir: args.parent_dir.clone(),
                    text: raw,
                    translator_base_url: args.translator_base_url.clone(),
                    task_id: args.task_id.clone(),
                },
            )
        })
        .collect();
    let skipped = preflight.skipped;
    let mut errors = preflight.errors;

    let total = to_import.len();
    if total == 0 {
        return Ok(LookupImportBatchResult {
            imported: Vec::new(),
            skills,
            skill_candidates,
            search_candidates,
            skipped,
            errors,
        });
    }

    // Phase 2: run imports with a concurrency limit and emit count progress.
    let concurrency = args.concurrency.unwrap_or(5).max(1);
    let imported = Arc::new(Mutex::new(Vec::new()));
    let counter = Arc::new(AtomicUsize::new(0));

    let stream = futures_util::stream::iter(to_import.into_iter().map(|(raw, single)| {
        let imported = imported.clone();
        let counter = counter.clone();
        let task_id = args.task_id.clone();
        async move {
            let result = import_by_identifier_with_progress(single, app, cache).await;
            let done = counter.fetch_add(1, Ordering::SeqCst) + 1;
            emit_batch_progress(app, task_id.as_deref(), done, total);
            match result {
                Ok(r) => {
                    imported.lock().await.push(r);
                    Ok(())
                }
                Err(e) => Err(format!("{raw}: {e}")),
            }
        }
    }));

    let import_errors: Vec<String> = stream
        .buffer_unordered(concurrency)
        .filter_map(|r| async { r.err() })
        .collect()
        .await;

    errors.extend(import_errors);

    let imported = Arc::try_unwrap(imported)
        .expect("all import futures finished")
        .into_inner();

    Ok(LookupImportBatchResult {
        imported,
        skills,
        skill_candidates,
        search_candidates,
        skipped,
        errors,
    })
}

/// Top-N candidates shown in the magic-wand picker.
const SEARCH_CANDIDATE_LIMIT: usize = 3;

/// Resolve free-text queries to importable candidates. Empty results and search
/// failures become errors so a title that matches nothing is never a silent no-op.
pub(crate) async fn resolve_search_queries(
    queries: &[String],
    errors: &mut Vec<String>,
) -> Vec<PaperSearchGroup> {
    let mut groups = Vec::new();
    for query in queries {
        match title_search::search_papers(query, SEARCH_CANDIDATE_LIMIT).await {
            Ok(candidates) if candidates.is_empty() => {
                errors.push(format!("{query}: no search results"));
            }
            Ok(candidates) => groups.push(PaperSearchGroup {
                query: query.clone(),
                candidates,
            }),
            Err(e) => errors.push(format!("{query}: {e}")),
        }
    }
    groups
}

fn emit_batch_progress(
    app: Option<&AppHandle>,
    task_id: Option<&str>,
    current: usize,
    total: usize,
) {
    #[cfg(not(feature = "desktop"))]
    let _ = (app, task_id, current, total);
    #[cfg(feature = "desktop")]
    {
        let (Some(app), Some(task_id)) = (app, task_id) else {
            return;
        };
        let progress = ((current as f64 / total.max(1) as f64) * 100.0).round() as u8;
        let _ = app.emit(
            "background-task:progress",
            AssetDownloadProgress {
                task_id: task_id.to_string(),
                phase: "import".to_string(),
                downloaded_bytes: 0,
                total_bytes: None,
                progress: Some(progress),
                current_count: Some(current),
                total_count: Some(total),
            },
        );
    }
}

pub(crate) fn identifier_kind_str(kind: IdentifierKind) -> String {
    match kind {
        IdentifierKind::Doi => "doi",
        IdentifierKind::Isbn => "isbn",
        IdentifierKind::Arxiv => "arxiv",
        IdentifierKind::Pmid => "pmid",
        IdentifierKind::AdsBibcode => "ads",
        IdentifierKind::Url => "url",
        IdentifierKind::Skill => "skill",
    }
    .to_string()
}

pub(crate) fn identifier_kind_column(kind: IdentifierKind) -> Option<&'static str> {
    match kind {
        IdentifierKind::Arxiv => Some("arxiv_id"),
        IdentifierKind::Doi => Some("doi"),
        IdentifierKind::Isbn => Some("isbn"),
        IdentifierKind::Pmid => Some("pmid"),
        IdentifierKind::AdsBibcode => Some("id"),
        IdentifierKind::Url | IdentifierKind::Skill => None,
    }
}

/// On-demand download of PDF (+ arXiv LaTeX) for an existing paper folder.
pub async fn download_paper_assets(
    args: PaperDownloadAssetsArgs,
) -> Result<AssetDownloadResult, AppError> {
    download_paper_assets_with_progress(args, None, None).await
}

pub async fn download_paper_assets_with_progress(
    args: PaperDownloadAssetsArgs,
    app: Option<&AppHandle>,
    cache: Option<&CapsCache>,
) -> Result<AssetDownloadResult, AppError> {
    let vault = PathBuf::from(args.vault_path.trim());
    if !vault.is_dir() {
        return Err(AppError::message("vault path is not a directory"));
    }
    let path_rel = crate::core::fs::sanitize_vault_rel(&args.path)
        .map_err(|_| AppError::message("invalid paper path"))?;
    let paper_dir = vault.join(&path_rel);
    if !paper_dir.is_dir() {
        return Err(AppError::message("paper folder not found"));
    }

    let (id, arxiv_id, pdf_url, doi) = if let Ok(Some(row)) = papers::get_by_path(&vault, &path_rel)
    {
        (row.id, row.arxiv_id, row.pdf_url, row.doi)
    } else if let Ok(Some(row)) = papers::ensure_row_for_path(&vault, &path_rel) {
        // Orphaned folder (import failed after shell + folder were written but
        // before the catalog row landed): rebuild the row so the Library sees it.
        crate::features::lifecycle::emit_paper_imported(app, &vault, &row.id);
        (row.id, row.arxiv_id, row.pdf_url, row.doi)
    } else {
        // Fallback: folder name as id; treat as arXiv if it looks like one
        let name = paper_dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("paper")
            .to_string();
        let arxiv = parse::extract_arxiv_id(&name);
        let pdf = arxiv
            .as_ref()
            .map(|a| format!("https://arxiv.org/pdf/{}", a));
        (name, arxiv, pdf, None)
    };

    let result = ensure_paper_assets_with_progress(
        &paper_dir,
        &vault,
        &path_rel,
        &id,
        arxiv_id.as_deref(),
        pdf_url.as_deref(),
        doi.as_deref(),
        None,
        cache,
        AssetProgressContext {
            app,
            task_id: args.task_id.as_deref(),
        },
    )
    .await?;
    check_task_not_cancelled(args.task_id.as_deref())?;

    // When TeX was downloaded into source/, record body_source = "latex" in catalog
    // so the frontend doesn't show "download TeX" even though source/ is lazy‑loaded.
    if result.tex {
        if let Ok(Some(mut row)) = papers::get_by_path(&vault, &path_rel) {
            let changed = row.body_source.as_deref() != Some("latex");
            if changed {
                row.body_source = Some("latex".to_string());
                row.body_quality = Some("high".to_string());
                row.updated_at =
                    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
                let _ = papers::upsert_paper(&vault, &row);
            }
        }
    }

    if result.pdf && !result.tex && !result.paper_md {
        #[cfg(feature = "desktop")]
        crate::features::jobs::spawn_parse_body_after_assets(app, &vault, &path_rel, false);
    }

    crate::features::refs::spawn_parse_after_import(app, &vault, &path_rel);
    Ok(result)
}

/// Write drop payload bytes to `~/.agentero/import-tmp/<stamp>-<name>` and return the path.
/// Used when the webview cannot expose `File.path` (typical on macOS WKWebView).
pub fn stage_import_file(args: StageImportFileArgs) -> Result<StageImportFileResult, AppError> {
    use base64::Engine;

    let raw_name = args.file_name.trim();
    let name = if raw_name.is_empty() {
        "drop.pdf".to_string()
    } else {
        raw_name
            .chars()
            .map(|c| if c == '/' || c == '\\' { '_' } else { c })
            .collect::<String>()
    };
    if !name.to_ascii_lowercase().ends_with(".pdf") {
        return Err(AppError::message("not a PDF file"));
    }

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(args.content_base64.trim())
        .map_err(|e| AppError::message(format!("invalid base64: {e}")))?;
    if bytes.is_empty() {
        return Err(AppError::message("empty file"));
    }
    // Soft cap ~80MB — UI import is for papers, not bulk archives.
    if bytes.len() > 80 * 1024 * 1024 {
        return Err(AppError::message("file too large to stage (max 80MB)"));
    }

    let home =
        dirs::home_dir().ok_or_else(|| AppError::message("cannot resolve home directory"))?;
    let dir = home.join(".agentero").join("import-tmp");
    fs::create_dir_all(&dir)?;
    let stamp = format!(
        "{}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0),
        &uuid::Uuid::new_v4().simple().to_string()[..8]
    );
    let dest = dir.join(format!("{stamp}-{name}"));
    fs::write(&dest, bytes)?;
    Ok(StageImportFileResult {
        path: dest.to_string_lossy().to_string(),
    })
}

/// Import one or more local PDF files as paper folders (copy + catalog + liteparse).
/// Filename-derived title/id by default; optional per-file metadata overrides.
/// Each PDF becomes `{parent}/{slug}/{slug}.pdf`.
pub async fn import_local_pdfs(
    args: ImportLocalPdfArgs,
    app: Option<&AppHandle>,
    cache: Option<&CapsCache>,
) -> Result<ImportLocalPdfResult, AppError> {
    let vault = PathBuf::from(args.vault_path.trim());
    if !vault.is_dir() {
        return Err(AppError::message("vault path is not a directory"));
    }
    let parent_rel = normalize_parent_dir(&args.parent_dir)?;

    let task_id = args.task_id.clone();
    let entries: Vec<LocalPdfImportEntry> = if !args.entries.is_empty() {
        args.entries
    } else {
        args.file_paths
            .into_iter()
            .map(|file_path| LocalPdfImportEntry {
                file_path,
                title: None,
                authors: None,
                year: None,
                doi: None,
                arxiv_id: None,
                extra: None,
            })
            .collect()
    };
    let entries = dedupe_local_pdf_entries(entries);

    let mut papers_out = Vec::new();
    let mut errors = Vec::new();
    for entry in &entries {
        match import_one_local_pdf(
            &vault,
            &parent_rel,
            entry,
            &ImportLocalPdfContext {
                translator_base: args
                    .translator_base_url
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .unwrap_or(DEFAULT_TRANSLATOR_BASE_URL),
                task_id: task_id.as_deref(),
                app,
                cache,
            },
        )
        .await
        {
            Ok(r) => papers_out.push(r),
            Err(e) => {
                let name = Path::new(entry.file_path.trim())
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or(entry.file_path.as_str());
                errors.push(format!("{name}: {e}"));
            }
        }
    }
    // All failed → surface as an error; partial success returns per-file errors.
    if papers_out.is_empty() && !errors.is_empty() {
        return Err(AppError::message(errors.join("; ")));
    }
    Ok(ImportLocalPdfResult {
        papers: papers_out,
        errors,
    })
}

/// Drop repeated source files (same path spelled with `\` vs `/`, or Windows
/// case variants) so one drop/pick never commits the same PDF twice.
fn dedupe_local_pdf_entries(entries: Vec<LocalPdfImportEntry>) -> Vec<LocalPdfImportEntry> {
    let mut seen = std::collections::HashSet::new();
    entries
        .into_iter()
        .filter(|e| {
            let mut key = e.file_path.trim().replace('\\', "/");
            if cfg!(windows) {
                key = key.to_lowercase();
            }
            seen.insert(key)
        })
        .collect()
}

/// Shared per-import context threaded into `import_one_local_pdf`.
struct ImportLocalPdfContext<'a> {
    translator_base: &'a str,
    task_id: Option<&'a str>,
    app: Option<&'a AppHandle>,
    cache: Option<&'a CapsCache>,
}

async fn import_one_local_pdf(
    vault: &Path,
    parent_rel: &str,
    entry: &LocalPdfImportEntry,
    ctx: &ImportLocalPdfContext<'_>,
) -> Result<LookupImportResult, AppError> {
    use crate::features::import::paper_import::{
        paper_commit, AssetsPolicy, DedupePolicy, PaperCommitOptions,
    };

    let src = PathBuf::from(entry.file_path.trim());
    if !src.is_file() {
        return Err(AppError::message("file not found"));
    }
    let is_pdf = src
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("pdf"));
    if !is_pdf {
        return Err(AppError::message("not a PDF file"));
    }

    let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("paper");
    let title = entry
        .title
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| title_from_stem(stem));
    let base_id = entry
        .arxiv_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(slug_from_stem)
        .or_else(|| {
            entry
                .doi
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(map::doi_slug)
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| slug_from_stem(stem));

    let dialog_meta = entry.title.is_some()
        || entry.doi.is_some()
        || entry.arxiv_id.is_some()
        || entry.extra.is_some();

    // Entries straight from the picker (no dialog metadata) run background
    // recognition so DOI/arXiv/title survive renamed files. Best-effort:
    // any failure keeps the filename-derived metadata.
    let mut meta = if dialog_meta {
        local_pdf_meta(base_id, title)
    } else {
        let fallback = local_pdf_meta(base_id.clone(), title.clone());
        match pdf_recognize::recognize_and_resolve(&src, ctx.translator_base, ctx.task_id).await {
            probe if probe.status == "ok" => {
                // Adopt the resolved identifier as the folder id (matches
                // identifier-import naming, e.g. papers/1706.03762).
                let resolved_id = probe
                    .arxiv_id
                    .as_deref()
                    .map(slug_from_stem)
                    .or_else(|| probe.doi.as_deref().map(map::doi_slug))
                    .filter(|s| !s.is_empty())
                    .unwrap_or(base_id);
                let mut m = local_pdf_meta(resolved_id, probe.title.clone().unwrap_or(title));
                m.authors = probe.authors.clone();
                m.year = probe.year;
                m.doi = probe.doi.clone();
                m.arxiv_id = probe.arxiv_id.clone();
                m.abstract_text = probe.abstract_text.clone();
                m.publication = probe.publication.clone();
                m.volume = probe.volume.clone();
                m.issue = probe.issue.clone();
                m.pages = probe.pages.clone();
                m.publisher = probe.publisher.clone();
                m.meta_source = Some("recognize".into());
                m
            }
            probe if probe.status == "title" => {
                let mut m = local_pdf_meta(base_id, probe.title.clone().unwrap_or(title));
                m.authors = probe.authors.clone();
                m.year = probe.year;
                m.doi = probe.doi.clone();
                m.arxiv_id = probe.arxiv_id.clone();
                m.meta_source = Some("recognize".into());
                m
            }
            _ => fallback,
        }
    };
    if let Some(authors) = &entry.authors {
        meta.authors = authors
            .iter()
            .map(|a| a.trim())
            .filter(|a| !a.is_empty())
            .map(|a| a.to_string())
            .collect();
    }
    if let Some(year) = entry.year {
        meta.year = Some(year);
    }
    // Dialog-provided identifiers and fetched fields win over recognition
    // and filename defaults; the user confirmed them, so mark the row manual.
    let mut has_dialog_meta = false;
    if let Some(doi) = entry
        .doi
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        meta.doi = Some(doi.to_string());
        has_dialog_meta = true;
    }
    if let Some(arxiv) = entry
        .arxiv_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        meta.arxiv_id = Some(arxiv.to_string());
        has_dialog_meta = true;
    }
    if let Some(extra) = &entry.extra {
        if extra.publication.is_some()
            || extra.volume.is_some()
            || extra.issue.is_some()
            || extra.pages.is_some()
            || extra.abstract_text.is_some()
        {
            has_dialog_meta = true;
        }
        meta.publication = extra.publication.clone().filter(|s| !s.trim().is_empty());
        meta.volume = extra.volume.clone().filter(|s| !s.trim().is_empty());
        meta.issue = extra.issue.clone().filter(|s| !s.trim().is_empty());
        meta.pages = extra.pages.clone().filter(|s| !s.trim().is_empty());
        meta.publisher = extra.publisher.clone().filter(|s| !s.trim().is_empty());
        meta.issn = extra.issn.clone().filter(|s| !s.trim().is_empty());
        meta.language = extra.language.clone().filter(|s| !s.trim().is_empty());
        if let Some(date) = extra
            .date
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            meta.date = Some(date.to_string());
            if meta.year.is_none() {
                meta.year = date.chars().take(4).collect::<String>().parse().ok();
            }
        }
        meta.abstract_text = extra.abstract_text.clone().filter(|s| !s.trim().is_empty());
    }
    if has_dialog_meta {
        meta.meta_source = Some("manual".into());
    }

    let progress = AssetProgressContext {
        app: ctx.app,
        task_id: ctx.task_id,
    };
    let commit = paper_commit(
        meta,
        PaperCommitOptions {
            vault,
            parent_dir: parent_rel,
            dedupe: DedupePolicy::None,
            assets: AssetsPolicy::CopyPdf {
                src: &src,
                progress,
            },
            translate_abstract: true,
            fresh_timestamps: false,
            cache: ctx.cache,
            app: ctx.app,
        },
    )
    .await?;

    Ok(LookupImportResult {
        paper_dir: commit.paper_dir,
        path: commit.path,
        id: commit.id,
        title: commit.title,
        used_translator: false,
        translator_base_url: String::new(),
        pdf: commit.pdf,
        tex: commit.tex,
        paper_md: commit.paper_md,
        asset_messages: commit.asset_messages,
    })
}

/// Folder-safe slug from a filename stem (alphanumerics + dots; other runs → `-`).
pub(crate) fn slug_from_stem(stem: &str) -> String {
    let mut s = String::new();
    let mut prev_sep = true; // suppress leading separators
    for c in stem.trim().chars() {
        if c.is_ascii_alphanumeric() || c == '.' {
            s.push(c);
            prev_sep = false;
        } else if !prev_sep {
            s.push('-');
            prev_sep = true;
        }
    }
    let s: String = s.chars().take(60).collect();
    let s = s.trim_matches(|c| c == '-' || c == '.').to_string();
    if s.is_empty() {
        "paper".into()
    } else {
        s
    }
}

/// Human title from a filename stem (underscores → spaces, whitespace collapsed).
pub(crate) fn title_from_stem(stem: &str) -> String {
    let spaced: String = stem
        .trim()
        .chars()
        .map(|c| if c == '_' { ' ' } else { c })
        .collect();
    let collapsed = spaced.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        "Untitled".into()
    } else {
        collapsed
    }
}

/// Allocate a free `{parent}/{id}` paper folder, suffixing `-2`, `-3`, … on
/// collision. A path counts as taken when the folder exists on disk **or** the
/// catalog has a row for it (folder may have been deleted externally).
/// Returns `(id, path_rel, absolute_dir)` — callers must adopt the returned id
/// into `meta.id` so folder name and catalog id never diverge.
pub(crate) fn allocate_paper_path(
    vault: &Path,
    parent_rel: &str,
    base_id: &str,
) -> (String, String, PathBuf) {
    let taken = |path_rel: &str| -> bool {
        vault.join(path_rel).exists() || matches!(papers::get_by_path(vault, path_rel), Ok(Some(_)))
    };
    let mut id = base_id.to_string();
    let mut n = 2;
    loop {
        let path_rel = format!("{parent_rel}/{id}").replace('\\', "/");
        if !taken(&path_rel) || n > 999 {
            let dir = vault.join(&path_rel);
            return (id, path_rel, dir);
        }
        id = format!("{base_id}-{n}");
        n += 1;
    }
}

pub(crate) async fn resolve_metadata(
    text: &str,
    translator_base: &str,
    task_id: Option<&str>,
) -> Result<(PaperMeta, bool), AppError> {
    // Prefer Translator Runtime (placeholder URL)
    match translator_fetch(text, translator_base, task_id).await {
        Ok(meta) => {
            check_task_not_cancelled(task_id)?;
            Ok((meta, true))
        }
        Err(e) => {
            // Fall back for arXiv so local dev works without sidecar
            if let Some(aid) = parse::extract_arxiv_id(text) {
                let meta = fetch_arxiv_metadata(&aid, task_id).await?;
                check_task_not_cancelled(task_id)?;
                Ok((meta, false))
            } else {
                Err(AppError::message(format!(
                    "translator unreachable at {translator_base} ({e}); only arXiv fallback is available without Runtime"
                )))
            }
        }
    }
}

async fn translator_fetch(
    text: &str,
    base: &str,
    task_id: Option<&str>,
) -> Result<PaperMeta, AppError> {
    let client = crate::features::network::client_builder()
        .timeout(Duration::from_secs(30))
        .user_agent("agentero-lookup/0.1 (+https://github.com/poco-ai/agentero)")
        .build()
        .map_err(|e| AppError::message(format!("http client: {e}")))?;

    let (endpoint, body) = translator_request(text, base);

    let res = client
        .post(&endpoint)
        .header("Content-Type", "text/plain")
        .body(body)
        .send()
        .await
        .map_err(|e| AppError::message(format!("translator request failed: {e}")))?;
    check_task_not_cancelled(task_id)?;

    let status = res.status();
    let bytes = res
        .bytes()
        .await
        .map_err(|e| AppError::message(format!("translator read body: {e}")))?;
    check_task_not_cancelled(task_id)?;

    if status.as_u16() == 300 {
        return Err(AppError::message(
            "translator returned multiple choices; pick a single paper URL/id",
        ));
    }
    if !status.is_success() {
        let snippet = String::from_utf8_lossy(&bytes);
        let short: String = snippet.chars().take(200).collect();
        return Err(AppError::message(format!(
            "translator HTTP {status}: {short}"
        )));
    }

    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| AppError::message(format!("translator JSON: {e}")))?;

    let item = if value.is_array() {
        value
            .as_array()
            .and_then(|a| a.first())
            .cloned()
            .ok_or_else(|| AppError::message("translator returned empty items array"))?
    } else if value.is_object() {
        // Some servers return a single object
        value
    } else {
        return Err(AppError::message("unexpected translator response shape"));
    };

    map_zotero_item(&item)
}

/// Build a Translator Runtime request from an identifier.
///
/// arXiv's PDF endpoints are binary resources, which the Translator Runtime
/// cannot parse as web pages. Canonicalizing every recognized arXiv form to
/// its abstract page also gives direct IDs and URLs the same metadata path.
fn translator_request(text: &str, base: &str) -> (String, String) {
    if let Some(arxiv_id) = parse::extract_arxiv_id(text) {
        return (
            format!("{base}/web"),
            format!("https://arxiv.org/abs/{arxiv_id}"),
        );
    }

    let ident = extract_primary_identifier(text);
    match &ident {
        Some((IdentifierKind::Url, url)) => (format!("{base}/web"), url.clone()),
        Some((_, value)) => (format!("{base}/search"), value.clone()),
        None => {
            // Treat as search raw text / possible URL.
            if text.starts_with("http://") || text.starts_with("https://") {
                (format!("{base}/web"), text.to_string())
            } else {
                (format!("{base}/search"), text.to_string())
            }
        }
    }
}

pub(crate) async fn fetch_arxiv_metadata(
    arxiv_id: &str,
    task_id: Option<&str>,
) -> Result<PaperMeta, AppError> {
    let bare = regex_lite_strip_version(arxiv_id);
    let api = format!(
        "https://export.arxiv.org/api/query?id_list={}",
        urlencoding_encode(&bare)
    );
    let client = crate::features::network::client_builder()
        .timeout(Duration::from_secs(30))
        .user_agent("agentero-lookup/0.1")
        .build()
        .map_err(|e| AppError::message(format!("http client: {e}")))?;
    let xml = client
        .get(&api)
        .send()
        .await
        .map_err(|e| AppError::message(format!("arXiv API: {e}")))?
        .text()
        .await
        .map_err(|e| AppError::message(format!("arXiv body: {e}")))?;
    check_task_not_cancelled(task_id)?;

    map::map_arxiv_atom(&xml, &bare)
}

fn regex_lite_strip_version(id: &str) -> String {
    let s = id
        .trim()
        .trim_start_matches("arXiv:")
        .trim_start_matches("arxiv:");
    // strip trailing vN
    if let Some(i) = s.rfind('v') {
        if s[i + 1..].chars().all(|c| c.is_ascii_digit()) && i > 0 {
            return s[..i].to_string();
        }
    }
    s.to_string()
}

fn urlencoding_encode(s: &str) -> String {
    // minimal encode for arxiv ids
    s.replace('/', "%2F")
}

pub(crate) fn paper_record_from_meta(path: &str, meta: &PaperMeta) -> PaperRecord {
    PaperRecord {
        path: path.replace('\\', "/"),
        id: meta.id.clone(),
        paper_type: meta.paper_type.clone(),
        title: meta.title.clone(),
        authors: meta.authors.clone(),
        creators: meta.creators.clone(),
        year: meta.year,
        date: meta.date.clone(),
        abstract_text: meta.abstract_text.clone(),
        tags: meta
            .tags
            .iter()
            .map(crate::features::catalog::papers::PaperTag::new)
            .collect(),
        arxiv_id: meta.arxiv_id.clone(),
        doi: meta.doi.clone(),
        isbn: meta.isbn.clone(),
        issn: meta.issn.clone(),
        pmid: meta.pmid.clone(),
        publication: meta.publication.clone(),
        volume: meta.volume.clone(),
        issue: meta.issue.clone(),
        pages: meta.pages.clone(),
        publisher: meta.publisher.clone(),
        place: meta.place.clone(),
        series: meta.series.clone(),
        language: meta.language.clone(),
        pdf_url: meta.pdf_url.clone(),
        html_url: meta.html_url.clone(),
        source_url: meta.source_url.clone(),
        body_source: None,
        body_quality: None,
        bibtex_key: meta.bibtex_key.clone(),
        citation_count: None,
        zotero_item_type: meta.zotero_item_type.clone(),
        meta_source: meta.meta_source.clone(),
        extra: meta.extra.clone(),
        summary: meta.summary.clone(),
        status: meta.status.clone(),
        is_read: false,
        zotero_item_id: None,
        zotero_last_synced: None,
        added_at: meta.added_at.clone(),
        updated_at: meta.updated_at.clone(),
    }
}

/// Write `{paper}/NOTES.md` shell (title + optional abstract blockquote).
/// Abstract is shown in **Chinese** when free-MT race succeeds; when every engine
/// fails the blockquote is omitted (no English stand-in as "translation").
/// Catalog still stores the original `abstract_text`.
///
/// Annotations live in `{paper}/marks/*.json` at runtime (not part of the shell).
#[cfg_attr(not(feature = "desktop"), allow(dead_code))]
pub(crate) async fn write_paper_shell(paper_dir: &Path, meta: &PaperMeta) -> Result<(), AppError> {
    write_paper_shell_opts(paper_dir, meta, true).await
}

/// Same as [`write_paper_shell`], with optional abstract MT.
/// Connector saves must stay under the browser extension's ~15s timeout, so they
/// pass `translate_abstract = false` and fetch assets asynchronously.
pub(crate) async fn write_paper_shell_opts(
    paper_dir: &Path,
    meta: &PaperMeta,
    translate_abstract: bool,
) -> Result<(), AppError> {
    let abstract_block = match meta
        .abstract_text
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(a) => {
            if translate_abstract {
                // Race free-MT engines; omit the blockquote when none succeed
                // (do not fall back to English as a "translation").
                match abstract_for_notes(a).await {
                    Some(display) => format!("> {display}\n\n"),
                    None => String::new(),
                }
            } else {
                format!("> {a}\n\n")
            }
        }
        None => String::new(),
    };
    let body = format!("# {}\n\n{abstract_block}", meta.title);
    let mut aliases = vec![meta.title.clone()];
    if let Some(short) =
        crate::features::doctor::suggest_short_alias(&meta.title, &meta.authors, meta.year)
    {
        aliases.push(short);
    }
    let notes = crate::features::wiki::frontmatter::prepend_new_aliases(&body, &aliases)
        .map_err(AppError::message)?;
    fs::write(paper_dir.join("NOTES.md"), notes)?;
    Ok(())
}

/// Prefer zh-CN translation of the abstract for NOTES.md display.
///
/// - Already mostly CJK → return original.
/// - Else race free-MT engines; `None` when every engine fails (caller omits
///   the abstract block — no untranslated fallback).
async fn abstract_for_notes(text: &str) -> Option<String> {
    use crate::features::translate::{free_mt_to_zh, looks_mostly_cjk};
    if looks_mostly_cjk(text) {
        return Some(text.to_string());
    }
    free_mt_to_zh(text).await
}

pub(crate) fn normalize_parent_dir(raw: &str) -> Result<String, AppError> {
    let s = raw.trim().replace('\\', "/").trim_matches('/').to_string();
    if s.is_empty() {
        return Ok("papers".into());
    }
    if s == "papers" || s.starts_with("papers/") {
        // reject path traversal
        if s.split('/').any(|p| p == ".." || p.is_empty()) {
            return Err(AppError::message("invalid parent_dir"));
        }
        return Ok(s);
    }
    Err(AppError::message(
        "parent_dir must be papers or under papers/",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_from_stem_basic() {
        assert_eq!(
            slug_from_stem("Attention Is All You Need"),
            "Attention-Is-All-You-Need"
        );
        assert_eq!(
            slug_from_stem("vaswani_2017_attention"),
            "vaswani-2017-attention"
        );
        assert_eq!(slug_from_stem("1706.03762"), "1706.03762");
        assert_eq!(slug_from_stem("  spaced  "), "spaced");
        assert_eq!(slug_from_stem("!!!"), "paper");
    }

    #[test]
    fn title_from_stem_basic() {
        assert_eq!(
            title_from_stem("vaswani_2017_attention"),
            "vaswani 2017 attention"
        );
        assert_eq!(title_from_stem("  Hello   World  "), "Hello World");
        assert_eq!(title_from_stem("   "), "Untitled");
    }

    #[test]
    fn dedupe_local_pdf_entries_mixed_separators() {
        let entry = |p: &str| LocalPdfImportEntry {
            file_path: p.to_string(),
            title: None,
            authors: None,
            year: None,
            doi: None,
            arxiv_id: None,
            extra: None,
        };
        let out = dedupe_local_pdf_entries(vec![
            entry(r"C:\Users\me\x.pdf"),
            entry("C:/Users/me/x.pdf"),
            entry("C:/Users/me/y.pdf"),
        ]);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].file_path, r"C:\Users\me\x.pdf");
        assert_eq!(out[1].file_path, "C:/Users/me/y.pdf");
    }

    #[test]
    fn allocate_paper_path_free_and_collision() {
        let vault = std::env::temp_dir().join(format!(
            "agentero-alloc-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ));
        fs::create_dir_all(vault.join("papers")).unwrap();

        // Free path → base id unchanged.
        let (id, rel, dir) = allocate_paper_path(&vault, "papers", "1706.03762");
        assert_eq!(id, "1706.03762");
        assert_eq!(rel, "papers/1706.03762");
        assert_eq!(dir, vault.join("papers/1706.03762"));

        // Folder on disk → suffix -2, and returned id matches the folder name.
        fs::create_dir_all(vault.join("papers/1706.03762")).unwrap();
        let (id, rel, _) = allocate_paper_path(&vault, "papers", "1706.03762");
        assert_eq!(id, "1706.03762-2");
        assert_eq!(rel, "papers/1706.03762-2");

        let _ = fs::remove_dir_all(&vault);
    }

    #[test]
    fn translator_request_canonicalizes_arxiv_to_abs() {
        let base = "https://translator.example";

        for input in [
            "2508.05004",
            "arXiv:2508.05004v2",
            "https://arxiv.org/pdf/2508.05004",
            "https://arxiv.org/pdf/2508.05004.pdf?download=1",
            "https://arxiv.org/html/2508.05004",
        ] {
            assert_eq!(
                translator_request(input, base),
                (
                    "https://translator.example/web".to_string(),
                    "https://arxiv.org/abs/2508.05004".to_string(),
                ),
                "input: {input}"
            );
        }
    }
}

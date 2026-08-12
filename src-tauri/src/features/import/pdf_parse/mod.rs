//! Local PDF → `PAPER.md` via liteparse (no TeX papers).
//!
//! @see docs/backend/data-model.md § PAPER.md
//! @see docs/backend/api.md `paper_parse_body`

use crate::core::error::AppError;
use crate::features::catalog::{papers, probe_paper_caps, CapsCache};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use liteparse::config::{ImageMode, LiteParseConfig, OutputFormat};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use liteparse::LiteParse;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use std::process::Stdio;
use std::sync::{Arc, Mutex, OnceLock};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use std::time::Duration;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

const PAPER_MD: &str = "PAPER.md";
/// Cancellation is a user action, not a parse failure; `PaperParseResult::fail`
/// keys off this to avoid reporting cancelled work as broken.
const CANCELLED_MESSAGE: &str = "background task cancelled";
#[cfg(not(any(target_os = "ios", target_os = "android")))]
const PDF_PARSE_WORKER_ARG: &str = "--agentero-internal-pdf-parse-worker";
#[cfg(not(any(target_os = "ios", target_os = "android")))]
const PDF_PARSE_TIMEOUT: Duration = Duration::from_secs(120);
#[cfg(not(any(target_os = "ios", target_os = "android")))]
const MAX_CONCURRENT_PDF_PARSE: usize = 2;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
const PDFIUM_LIB_PATH_ENV: &str = "PDFIUM_LIB_PATH";

#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug)]
struct PdfParseAdmission {
    key: PathBuf,
    _permit: OwnedSemaphorePermit,
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl Drop for PdfParseAdmission {
    fn drop(&mut self) {
        if let Ok(mut in_flight) = pdf_parse_in_flight().lock() {
            in_flight.remove(&self.key);
        }
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn pdf_parse_limiter() -> &'static Arc<Semaphore> {
    static LIMITER: OnceLock<Arc<Semaphore>> = OnceLock::new();
    LIMITER.get_or_init(|| Arc::new(Semaphore::new(MAX_CONCURRENT_PDF_PARSE)))
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn pdf_parse_in_flight() -> &'static Mutex<HashSet<PathBuf>> {
    static IN_FLIGHT: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
    IN_FLIGHT.get_or_init(|| Mutex::new(HashSet::new()))
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn pdf_parse_key(pdf_path: &Path) -> PathBuf {
    fs::canonicalize(pdf_path).unwrap_or_else(|_| pdf_path.to_path_buf())
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
async fn acquire_pdf_parse_permit(task_id: Option<&str>) -> Result<OwnedSemaphorePermit, AppError> {
    loop {
        if pdf_parse_task_is_cancelled(task_id) {
            return Err(AppError::message(CANCELLED_MESSAGE));
        }
        match pdf_parse_limiter().clone().try_acquire_owned() {
            Ok(permit) => return Ok(permit),
            Err(tokio::sync::TryAcquireError::NoPermits) => {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(tokio::sync::TryAcquireError::Closed) => {
                return Err(AppError::message("PDF parse limiter closed"));
            }
        }
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
async fn enter_pdf_parse(
    pdf_path: &Path,
    task_id: Option<&str>,
) -> Result<Option<PdfParseAdmission>, AppError> {
    let permit = acquire_pdf_parse_permit(task_id).await?;
    if pdf_parse_task_is_cancelled(task_id) {
        return Err(AppError::message(CANCELLED_MESSAGE));
    }
    let key = pdf_parse_key(pdf_path);
    let mut in_flight = pdf_parse_in_flight()
        .lock()
        .map_err(|_| AppError::message("PDF parse in-flight set poisoned"))?;
    if !in_flight.insert(key.clone()) {
        return Ok(None);
    }
    Ok(Some(PdfParseAdmission {
        key,
        _permit: permit,
    }))
}

#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaperParseResult {
    pub paper_md: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_quality: Option<String>,
    /// Set only when the parse genuinely failed, never for a skip. Callers
    /// surface it as an error instead of reporting a silent success.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub messages: Vec<String>,
}

impl PaperParseResult {
    fn fail(&mut self, message: String) {
        if !message.contains(CANCELLED_MESSAGE) {
            self.error = Some(message.clone());
        }
        self.messages.push(message);
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaperParseBodyArgs {
    pub vault_path: String,
    /// Vault-relative paper folder, e.g. `papers/1706.03762`.
    pub path: String,
    /// When true, overwrite existing `PAPER.md`. Default false.
    #[serde(default)]
    pub force: bool,
    /// Frontend background-task id; passed to the isolated parser worker for cancellation.
    #[serde(default)]
    pub task_id: Option<String>,
}

/// After PDF/TeX download: if no TeX and PDF present, generate `PAPER.md` when missing.
pub async fn maybe_generate_paper_md_after_download(
    vault: &Path,
    path_rel: &str,
    paper_dir: &Path,
) -> PaperParseResult {
    maybe_generate_paper_md_after_download_with_task(vault, path_rel, paper_dir, None).await
}

/// Auto-parse variant used by frontend background tasks.
///
/// `task_id` connects frontend cancellation to the parser worker. The parser
/// runs in a killable child process so a stuck PDFium/OCR call cannot keep the
/// import or download command alive indefinitely.
pub async fn maybe_generate_paper_md_after_download_with_task(
    vault: &Path,
    path_rel: &str,
    paper_dir: &Path,
    task_id: Option<&str>,
) -> PaperParseResult {
    parse_paper_body_inner(vault, path_rel, paper_dir, false, task_id, None).await
}

/// Manual / bulk parse entry (command).
pub async fn parse_paper_body(
    args: PaperParseBodyArgs,
    cache: Option<&CapsCache>,
) -> Result<PaperParseResult, AppError> {
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
    Ok(parse_paper_body_inner(
        &vault,
        &path_rel,
        &paper_dir,
        args.force,
        args.task_id.as_deref(),
        cache,
    )
    .await)
}

async fn parse_paper_body_inner(
    vault: &Path,
    path_rel: &str,
    paper_dir: &Path,
    force: bool,
    task_id: Option<&str>,
    cache: Option<&CapsCache>,
) -> PaperParseResult {
    let mut out = PaperParseResult::default();
    let caps = cache
        .map(|c| c.caps_for(vault, path_rel))
        .unwrap_or_else(|| probe_paper_caps(paper_dir));

    if caps.has_tex {
        out.messages.push("skip: local TeX present".into());
        return out;
    }

    if caps.has_paper_md && !force {
        out.paper_md = true;
        out.messages.push("PAPER.md already present".into());
        return out;
    }

    let Some(pdf_path) = caps.pdf_path else {
        out.messages.push("skip: no local PDF".into());
        return out;
    };

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let _admission = match enter_pdf_parse(&pdf_path, task_id).await {
        Ok(Some(admission)) => admission,
        Ok(None) => {
            out.messages
                .push("skip: PDF parse already in flight".into());
            return out;
        }
        Err(e) => {
            out.fail(format!("liteparse failed: {e}"));
            return out;
        }
    };

    match run_liteparse_markdown(&pdf_path, task_id).await {
        Ok((markdown, body_source, body_quality)) => {
            if markdown.trim().is_empty() {
                out.fail("liteparse returned empty text".into());
                return out;
            }
            match fs::write(paper_dir.join(PAPER_MD), &markdown) {
                Ok(()) => {
                    out.paper_md = true;
                    out.body_source = Some(body_source.clone());
                    out.body_quality = Some(body_quality.clone());
                    out.messages.push("PAPER.md written".into());
                    if let Err(e) =
                        update_catalog_body(vault, path_rel, &body_source, &body_quality)
                    {
                        out.messages
                            .push(format!("catalog body fields update failed: {e}"));
                    }
                }
                Err(e) => out.fail(format!("write PAPER.md failed: {e}")),
            }
        }
        Err(e) => out.fail(format!("liteparse failed: {e}")),
    }

    if let Some(c) = cache {
        c.invalidate(vault, path_rel);
    }

    out
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum PdfParseWorkerResponse {
    Ok {
        markdown: String,
        body_source: String,
        body_quality: String,
    },
    Err {
        message: String,
    },
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, PartialEq, Eq)]
struct PdfParseWorkerRequest {
    pdf_path: PathBuf,
    response_path: PathBuf,
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn pdf_parse_worker_request_from_args(
    args: impl IntoIterator<Item = OsString>,
) -> Result<Option<PdfParseWorkerRequest>, String> {
    let mut args = args.into_iter();
    let _executable = args.next();
    let Some(mode) = args.next() else {
        return Ok(None);
    };
    if mode != OsStr::new(PDF_PARSE_WORKER_ARG) {
        return Ok(None);
    }
    let pdf_path = args
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| "PDF parse worker is missing its input path".to_string())?;
    let response_path = args
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| "PDF parse worker is missing its response path".to_string())?;
    if args.next().is_some() {
        return Err("PDF parse worker received unexpected arguments".to_string());
    }
    Ok(Some(PdfParseWorkerRequest {
        pdf_path,
        response_path,
    }))
}

/// Handle the private parser-worker mode before Tauri or CLI initialization.
///
/// Returns `None` for a normal application launch. Desktop entrypoints exit
/// immediately with the returned status code when worker mode is selected.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub fn try_run_pdf_parse_worker() -> Option<i32> {
    let request = match pdf_parse_worker_request_from_args(std::env::args_os()) {
        Ok(Some(request)) => request,
        Ok(None) => return None,
        Err(_) => return Some(2),
    };

    let response = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => match runtime.block_on(run_liteparse_markdown_direct(&request.pdf_path)) {
            Ok((markdown, body_source, body_quality)) => PdfParseWorkerResponse::Ok {
                markdown,
                body_source,
                body_quality,
            },
            Err(error) => PdfParseWorkerResponse::Err {
                message: error.to_string(),
            },
        },
        Err(error) => PdfParseWorkerResponse::Err {
            message: format!("start PDF parse worker runtime: {error}"),
        },
    };

    let result = serde_json::to_vec(&response)
        .map_err(|error| AppError::message(format!("serialize PDF parse result: {error}")))
        .and_then(|bytes| {
            fs::write(&request.response_path, bytes)
                .map_err(|error| AppError::message(format!("write PDF parse result: {error}")))
        });
    Some(if result.is_ok() { 0 } else { 1 })
}

#[cfg(any(target_os = "ios", target_os = "android"))]
pub fn try_run_pdf_parse_worker() -> Option<i32> {
    None
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn pdfium_lib_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "libpdfium.dylib"
    } else if cfg!(target_os = "windows") {
        "pdfium.dll"
    } else {
        "libpdfium.so"
    }
}

/// Directory holding the PDFium shared library shipped with the installed app.
///
/// liteparse `dlopen`s PDFium and `liteparse-pdfium-sys`'s build script bakes
/// the build machine's download cache path into the binary, which does not
/// exist on a user machine. `scripts/prepare-pdfium.mjs` stages the library
/// into the bundle instead; these are the places it lands.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn bundled_pdfium_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;
    let mut candidates = Vec::new();
    if cfg!(target_os = "macos") {
        candidates.push(exe_dir.join("../Frameworks"));
    }
    candidates.push(exe_dir.join("pdfium"));
    // deb / AppImage put bundle resources under /usr/lib/<product>/.
    candidates.push(exe_dir.join("../lib/agentero/pdfium"));
    candidates.push(exe_dir.join("../lib/Agentero/pdfium"));
    candidates.push(exe_dir.to_path_buf());

    let name = pdfium_lib_name();
    candidates.into_iter().find(|dir| dir.join(name).is_file())
}

/// `PDFIUM_LIB_PATH` to hand the worker, unless the caller already set one.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn pdfium_lib_path_override() -> Option<PathBuf> {
    if std::env::var_os(PDFIUM_LIB_PATH_ENV).is_some() {
        return None;
    }
    bundled_pdfium_dir()
}

/// Tail of the worker's stderr, so a panic that never reaches the response file
/// (a missing PDFium library aborts the process with exit code 101) still
/// reaches the user.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn worker_stderr_tail(path: &Path) -> Option<String> {
    const MAX_CHARS: usize = 800;
    let text = fs::read_to_string(path).ok()?;
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let mut tail: Vec<char> = text.chars().rev().take(MAX_CHARS).collect();
    tail.reverse();
    Some(tail.into_iter().collect())
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
async fn run_liteparse_markdown(
    pdf_path: &Path,
    task_id: Option<&str>,
) -> Result<(String, String, String), AppError> {
    if pdf_parse_task_is_cancelled(task_id) {
        return Err(AppError::message(CANCELLED_MESSAGE));
    }
    let executable = std::env::current_exe()
        .map_err(|error| AppError::message(format!("resolve PDF parse worker: {error}")))?;
    let worker_dir = std::env::temp_dir().join(format!(
        "agentero-pdf-parse-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4().simple()
    ));
    fs::create_dir_all(&worker_dir).map_err(|error| {
        AppError::message(format!("create PDF parse worker directory: {error}"))
    })?;
    let response_path = worker_dir.join("response.json");
    let stderr_path = worker_dir.join("stderr.log");
    let stderr_sink = match fs::File::create(&stderr_path) {
        Ok(file) => file,
        Err(error) => {
            let _ = fs::remove_dir_all(&worker_dir);
            return Err(AppError::message(format!(
                "create PDF parse worker log: {error}"
            )));
        }
    };

    let mut command = tokio::process::Command::new(executable);
    command
        .arg(PDF_PARSE_WORKER_ARG)
        .arg(pdf_path)
        .arg(&response_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(stderr_sink))
        .kill_on_drop(true);
    if let Some(dir) = pdfium_lib_path_override() {
        command.env(PDFIUM_LIB_PATH_ENV, dir);
    }

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            let _ = fs::remove_dir_all(&worker_dir);
            return Err(AppError::message(format!(
                "start isolated PDF parser: {error}"
            )));
        }
    };

    let timeout = tokio::time::sleep(PDF_PARSE_TIMEOUT);
    tokio::pin!(timeout);
    let mut cancel_poll = tokio::time::interval(Duration::from_millis(100));
    cancel_poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let status = loop {
        tokio::select! {
            result = child.wait() => {
                break match result {
                    Ok(status) => status,
                    Err(error) => {
                        let _ = fs::remove_dir_all(&worker_dir);
                        return Err(AppError::message(format!(
                            "wait for isolated PDF parser: {error}"
                        )));
                    }
                };
            }
            _ = &mut timeout => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                let _ = fs::remove_dir_all(&worker_dir);
                return Err(AppError::message(format!(
                    "liteparse timed out after {}s; PDF import completed without PAPER.md",
                    PDF_PARSE_TIMEOUT.as_secs()
                )));
            }
            _ = cancel_poll.tick(), if task_id.is_some() => {
                if pdf_parse_task_is_cancelled(task_id) {
                    let _ = child.kill().await;
                    let _ = child.wait().await;
                    let _ = fs::remove_dir_all(&worker_dir);
                    return Err(AppError::message(CANCELLED_MESSAGE));
                }
            }
        }
    };

    let response = fs::read(&response_path)
        .map_err(|error| {
            let tail = worker_stderr_tail(&stderr_path)
                .map(|tail| format!(": {tail}"))
                .unwrap_or_default();
            AppError::message(format!(
                "isolated PDF parser produced no response ({status}, {error}){tail}"
            ))
        })
        .and_then(|bytes| {
            serde_json::from_slice::<PdfParseWorkerResponse>(&bytes).map_err(|error| {
                AppError::message(format!("decode isolated PDF parser response: {error}"))
            })
        });
    let _ = fs::remove_dir_all(&worker_dir);

    match response? {
        PdfParseWorkerResponse::Ok {
            markdown,
            body_source,
            body_quality,
        } => Ok((markdown, body_source, body_quality)),
        PdfParseWorkerResponse::Err { message } => Err(AppError::message(message)),
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn pdf_parse_task_is_cancelled(task_id: Option<&str>) -> bool {
    task_id.is_some_and(crate::features::agent::background_tasks::is_cancelled)
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
async fn run_liteparse_markdown_direct(
    pdf_path: &Path,
) -> Result<(String, String, String), AppError> {
    // Read the PDF via std::fs (Unicode-safe on Windows) and hand PDFium an
    // in-memory buffer. `FPDF_LoadDocument`'s path handling is unreliable for
    // non-ASCII paths on Windows (e.g. a Chinese `文档` segment in a OneDrive
    // vault path), which made Zotero-imported papers silently fail to produce
    // PAPER.md; `FPDF_LoadMemDocument` (used for `PdfInput::Bytes`) has no path
    // step and is immune.
    let data = fs::read(pdf_path).map_err(|e| AppError::message(format!("read pdf: {e}")))?;

    // Prefer native text; OCR is best-effort and must not abort the whole parse.
    let config = LiteParseConfig {
        ocr_enabled: true,
        ocr_failure_fatal: false,
        output_format: OutputFormat::Markdown,
        image_mode: ImageMode::Off,
        quiet: true,
        max_pages: 500,
        extract_links: true,
        ..Default::default()
    };

    let parser = LiteParse::new(config);

    // Complexity pre-pass for quality labeling (cheap text-layer only).
    let needs_ocr = match parser
        .is_complex(liteparse::types::PdfInput::Bytes(data.clone()))
        .await
    {
        Ok(pages) => pages.iter().any(|p| p.needs_ocr),
        Err(_) => false,
    };

    let result = parser
        .parse_input(liteparse::types::PdfInput::Bytes(data))
        .await
        .map_err(|e| AppError::message(format!("liteparse: {e}")))?;

    let (body_source, body_quality) = if needs_ocr {
        ("ocr".to_string(), "low".to_string())
    } else {
        ("pdf".to_string(), "medium".to_string())
    };

    Ok((result.text, body_source, body_quality))
}

#[cfg(any(target_os = "ios", target_os = "android"))]
async fn run_liteparse_markdown(
    _pdf_path: &Path,
    _task_id: Option<&str>,
) -> Result<(String, String, String), AppError> {
    Err(AppError::message(
        "PDF body parsing runs on the paired desktop host",
    ))
}

fn update_catalog_body(
    vault: &Path,
    path_rel: &str,
    body_source: &str,
    body_quality: &str,
) -> Result<(), AppError> {
    let Some(mut row) = papers::get_by_path(vault, path_rel)? else {
        // No catalog row yet — still wrote PAPER.md; skip SQLite.
        return Ok(());
    };
    row.body_source = Some(body_source.to_string());
    row.body_quality = Some(body_quality.to_string());
    row.updated_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    papers::upsert_paper(vault, &row)?;
    Ok(())
}

#[cfg(all(test, not(any(target_os = "ios", target_os = "android"))))]
mod tests {
    use super::*;

    #[test]
    fn fail_records_real_errors_but_not_cancellation() {
        let mut broken = PaperParseResult::default();
        broken.fail("liteparse failed: could not find pdfium".into());
        assert_eq!(
            broken.error.as_deref(),
            Some("liteparse failed: could not find pdfium")
        );
        assert_eq!(broken.messages.len(), 1);

        let mut cancelled = PaperParseResult::default();
        cancelled.fail(format!("liteparse failed: {CANCELLED_MESSAGE}"));
        assert!(cancelled.error.is_none());
        assert_eq!(cancelled.messages.len(), 1);
    }

    #[test]
    fn worker_stderr_tail_skips_empty_and_truncates() {
        let dir = std::env::temp_dir().join(format!("pdf-parse-stderr-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();

        let missing = dir.join("absent.log");
        assert!(worker_stderr_tail(&missing).is_none());

        let blank = dir.join("blank.log");
        fs::write(&blank, "  \n\n").unwrap();
        assert!(worker_stderr_tail(&blank).is_none());

        let long = dir.join("long.log");
        fs::write(&long, format!("{}tail-marker", "x".repeat(4000))).unwrap();
        let tail = worker_stderr_tail(&long).unwrap();
        assert_eq!(tail.chars().count(), 800);
        assert!(tail.ends_with("tail-marker"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn worker_args_are_private_and_exact() {
        let normal = vec![OsString::from("agentero"), OsString::from("paper")];
        assert_eq!(pdf_parse_worker_request_from_args(normal).unwrap(), None);

        let request = pdf_parse_worker_request_from_args(vec![
            OsString::from("agentero"),
            OsString::from(PDF_PARSE_WORKER_ARG),
            OsString::from("input.pdf"),
            OsString::from("response.json"),
        ])
        .unwrap()
        .unwrap();
        assert_eq!(request.pdf_path, PathBuf::from("input.pdf"));
        assert_eq!(request.response_path, PathBuf::from("response.json"));

        let incomplete = vec![
            OsString::from("agentero"),
            OsString::from(PDF_PARSE_WORKER_ARG),
            OsString::from("input.pdf"),
        ];
        assert!(pdf_parse_worker_request_from_args(incomplete).is_err());
    }

    #[test]
    fn worker_response_round_trips_success_and_failure() {
        for response in [
            PdfParseWorkerResponse::Ok {
                markdown: "# Paper".into(),
                body_source: "pdf".into(),
                body_quality: "medium".into(),
            },
            PdfParseWorkerResponse::Err {
                message: "broken PDF".into(),
            },
        ] {
            let encoded = serde_json::to_vec(&response).unwrap();
            let decoded: PdfParseWorkerResponse = serde_json::from_slice(&encoded).unwrap();
            assert_eq!(
                serde_json::to_value(decoded).unwrap(),
                serde_json::to_value(response).unwrap()
            );
        }
    }

    #[tokio::test]
    async fn pdf_parse_admission_rejects_duplicate_until_guard_drops() {
        let pdf_path =
            std::env::temp_dir().join(format!("pdf-parse-admission-{}.pdf", uuid::Uuid::new_v4()));

        let first = enter_pdf_parse(&pdf_path, None)
            .await
            .unwrap()
            .expect("first parse should enter admission");
        let duplicate = enter_pdf_parse(&pdf_path, None).await.unwrap();

        assert!(duplicate.is_none());

        drop(first);

        let next = enter_pdf_parse(&pdf_path, None).await.unwrap();
        assert!(next.is_some());
    }

    #[tokio::test]
    async fn cancelled_task_does_not_enter_pdf_parse_admission() {
        let task_id = format!("pdf-parse-test-{}", uuid::Uuid::new_v4());
        crate::features::agent::background_tasks::cancel(&task_id);

        let result = enter_pdf_parse(Path::new("missing-test-input.pdf"), Some(&task_id)).await;

        crate::features::agent::background_tasks::finish(&task_id);
        assert_eq!(result.unwrap_err().to_string(), "background task cancelled");
    }

    #[tokio::test]
    async fn cancelled_task_does_not_start_parser_worker() {
        let task_id = format!("pdf-parse-test-{}", uuid::Uuid::new_v4());
        crate::features::agent::background_tasks::cancel(&task_id);

        let result =
            run_liteparse_markdown(Path::new("missing-test-input.pdf"), Some(&task_id)).await;

        crate::features::agent::background_tasks::finish(&task_id);
        assert_eq!(result.unwrap_err().to_string(), "background task cancelled");
    }
}

//! Remote PDF layout analysis via the AI Studio hosted PP-StructureV3 OCR
//! service — an asynchronous whole-document job:
//! `POST {base}/api/v2/ocr/jobs` (multipart PDF upload) → poll
//! `GET {base}/api/v2/ocr/jobs/{jobId}` → download the JSONL result.
//!
//! Returns raw layout detection boxes; coordinate conversion stays in the
//! frontend.

use crate::core::error::AppError;
use crate::features::network;
use base64::Engine;
use reqwest::redirect::Policy;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutRemoteBox {
    pub cls_id: i64,
    pub label: String,
    pub score: f64,
    /// `[x1, y1, x2, y2]` in rendered-image pixels (top-left origin).
    pub coordinate: [f64; 4],
}

/// Map raw `layout_det_res.boxes` entries into the API shape.
pub fn parse_det_boxes(boxes: &[Value]) -> Vec<LayoutRemoteBox> {
    let mut out = Vec::with_capacity(boxes.len());
    for b in boxes {
        let Some(coordinate) = b.get("coordinate").and_then(Value::as_array) else {
            continue;
        };
        if coordinate.len() != 4 {
            continue;
        }
        let nums: Vec<f64> = coordinate.iter().filter_map(Value::as_f64).collect();
        if nums.len() != 4 {
            continue;
        }
        out.push(LayoutRemoteBox {
            cls_id: b.get("cls_id").and_then(Value::as_i64).unwrap_or(-1),
            label: b
                .get("label")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            score: b.get("score").and_then(Value::as_f64).unwrap_or(0.0),
            coordinate: [nums[0], nums[1], nums[2], nums[3]],
        });
    }
    out
}

/// Fixed AI Studio PaddleOCR jobs endpoint (not configurable).
const CLOUD_JOBS_URL: &str = "https://paddleocr.aistudio-app.com/api/v2/ocr/jobs";
const CLOUD_MODEL: &str = "PP-StructureV3";
const CLOUD_POLL_INTERVAL: Duration = Duration::from_secs(3);
const CLOUD_JOB_DEADLINE: Duration = Duration::from_secs(600);
const CLOUD_REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
/// Per-request payload cap for the whole-PDF upload (base64 chars).
const MAX_PDF_BASE64_CHARS: usize = 96 * 1024 * 1024;
/// Progress event consumed by the layout runner for the progress bar.
pub const CLOUD_PROGRESS_EVENT: &str = "layout-remote:progress";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutRemoteAnalyzePdfArgs {
    /// Base64-encoded PDF bytes.
    pub pdf_base64: String,
    /// Upload file name (default `paper.pdf`).
    #[serde(default)]
    pub file_name: Option<String>,
    /// Access-token override (probe flow); normally resolved from settings.
    #[serde(default)]
    pub api_key: Option<String>,
    /// Correlates `layout-remote:progress` when several API jobs run at once.
    #[serde(default)]
    pub request_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutRemotePageResult {
    pub boxes: Vec<LayoutRemoteBox>,
    /// Rendered page image size in px when the service reported it;
    /// otherwise null and the frontend assumes a 200 DPI render.
    pub width_px: Option<u32>,
    pub height_px: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutRemoteAnalyzePdfResult {
    pub pages: Vec<LayoutRemotePageResult>,
    /// Per-page rendered pixel sizes reported by the service
    /// (`dataInfo.pages[].width/height`); empty when absent.
    pub rendered_pages: Vec<(u32, u32)>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CloudProgressPayload {
    phase: String,
    extracted_pages: Option<u64>,
    total_pages: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    request_id: Option<String>,
}

/// Decode a base64 JPEG and read its SOF dimensions (best effort).
fn jpeg_dimensions_base64(b64: &str) -> Option<(u32, u32)> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .ok()?;
    let mut i = 2; // skip SOI
    while i + 4 < bytes.len() {
        if bytes[i] != 0xFF {
            return None;
        }
        let marker = bytes[i + 1];
        // SOF0–SOF15 except DHT/JPG/DAC.
        if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
            if i + 9 < bytes.len() {
                let height = u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]) as u32;
                let width = u16::from_be_bytes([bytes[i + 7], bytes[i + 8]]) as u32;
                if width > 0 && height > 0 {
                    return Some((width, height));
                }
            }
            return None;
        }
        if marker == 0xD9 {
            return None; // EOI
        }
        // Standalone markers (RST*, SOI, EOI) carry no length.
        if marker == 0xD8 || (0xD0..=0xD7).contains(&marker) {
            i += 2;
            continue;
        }
        let len = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
        if len < 2 {
            return None;
        }
        i += 2 + len;
    }
    None
}

/// Best-effort rendered-page size: the matching `dataInfo.pages[]` entry,
/// else the inline `inputImage` JPEG header. The `&'static str` names the
/// source (`"none"` → frontend must assume a render DPI).
fn page_rendered_size(
    page: &Value,
    data_info: Option<&Value>,
    rendered_pages: &[(u32, u32)],
    page_index: usize,
) -> (Option<u32>, Option<u32>, &'static str) {
    if let Some(&(w, h)) = rendered_pages.get(page_index) {
        if w > 0 && h > 0 {
            return (Some(w), Some(h), "dataInfo");
        }
    }
    // Legacy / alternate shapes: width & height directly on dataInfo.
    if let Some(info) = data_info {
        let width = info
            .get("width")
            .or_else(|| info.get("pageWidth"))
            .or_else(|| info.get("imageWidth"))
            .and_then(Value::as_u64);
        let height = info
            .get("height")
            .or_else(|| info.get("pageHeight"))
            .or_else(|| info.get("imageHeight"))
            .and_then(Value::as_u64);
        if let (Some(w), Some(h)) = (width, height) {
            if w > 0 && h > 0 {
                return (Some(w as u32), Some(h as u32), "dataInfo");
            }
        }
    }
    if let Some(b64) = page.get("inputImage").and_then(Value::as_str) {
        if let Some((w, h)) = jpeg_dimensions_base64(b64) {
            return (Some(w), Some(h), "inputImage");
        }
    }
    (None, None, "none")
}

/// Parse `dataInfo.pages` → per-page rendered pixel sizes.
fn parse_rendered_pages(data_info: Option<&Value>) -> Vec<(u32, u32)> {
    let Some(pages) = data_info
        .and_then(|d| d.get("pages"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    pages
        .iter()
        .map(|p| {
            let w = p.get("width").and_then(Value::as_u64).unwrap_or(0) as u32;
            let h = p.get("height").and_then(Value::as_u64).unwrap_or(0) as u32;
            (w, h)
        })
        .collect()
}

fn parse_cloud_page(
    page: &Value,
    data_info: Option<&Value>,
    rendered_pages: &[(u32, u32)],
    page_index: usize,
) -> (LayoutRemotePageResult, &'static str) {
    let boxes = page
        .get("prunedResult")
        .and_then(|p| p.get("layout_det_res"))
        .and_then(|d| d.get("boxes"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let (width_px, height_px, source) =
        page_rendered_size(page, data_info, rendered_pages, page_index);
    (
        LayoutRemotePageResult {
            boxes: parse_det_boxes(&boxes),
            width_px,
            height_px,
        },
        source,
    )
}

/// Submit one OCR job (multipart file upload) and return the job id.
async fn submit_job(
    client: &reqwest::Client,
    jobs_url: &str,
    auth: &str,
    file_bytes: Vec<u8>,
    file_name: String,
    mime: &str,
) -> Result<String, AppError> {
    // Skip OCR-heavy sub-pipelines we do not consume (matches the official sample).
    let optional_payload = json!({
        "useDocOrientationClassify": false,
        "useDocUnwarping": false,
        "useChartRecognition": false,
    })
    .to_string();
    let form = reqwest::multipart::Form::new()
        .part(
            "file",
            reqwest::multipart::Part::bytes(file_bytes)
                .file_name(file_name)
                .mime_str(mime)
                .map_err(|e| AppError::message(format!("layout_remote: multipart: {e}")))?,
        )
        .text("model", CLOUD_MODEL)
        .text("optionalPayload", optional_payload);
    let response = client
        .post(jobs_url)
        .header("Authorization", auth)
        .multipart(form)
        .send()
        .await
        .map_err(|e| AppError::message(format!("Paddle job submit failed: {e}")))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|e| AppError::message(format!("Paddle response read failed: {e}")))?;
    if !status.is_success() {
        let snippet: String = text.chars().take(180).collect();
        return Err(AppError::message(format!(
            "Paddle job submit failed (HTTP {status}): {snippet}"
        )));
    }
    let created: Value = serde_json::from_str(&text)
        .map_err(|e| AppError::message(format!("Unexpected Paddle response: {e}")))?;
    created
        .get("data")
        .and_then(|d| d.get("jobId"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| AppError::message("Unexpected Paddle response: missing jobId"))
}

fn resolve_cloud_target(api_key: Option<&str>) -> Result<(String, String), AppError> {
    let api_key = api_key
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .ok_or_else(|| {
            AppError::message("Paddle layout service requires apiKey (Settings → Layout)")
        })?
        .to_string();
    Ok((CLOUD_JOBS_URL.to_string(), format!("bearer {api_key}")))
}

pub async fn analyze_pdf(
    app: &AppHandle,
    args: LayoutRemoteAnalyzePdfArgs,
) -> Result<LayoutRemoteAnalyzePdfResult, AppError> {
    let (jobs_url, auth) = resolve_cloud_target(args.api_key.as_deref())?;
    if args.pdf_base64.trim().is_empty() {
        return Err(AppError::message("layout_remote: PDF is empty"));
    }
    if args.pdf_base64.len() > MAX_PDF_BASE64_CHARS {
        return Err(AppError::message("layout_remote: PDF too large"));
    }
    let pdf_bytes = base64::engine::general_purpose::STANDARD
        .decode(args.pdf_base64.trim())
        .map_err(|e| AppError::message(format!("layout_remote: invalid PDF base64: {e}")))?;

    let client = network::client_builder()
        .timeout(CLOUD_REQUEST_TIMEOUT)
        .redirect(Policy::limited(5))
        .build()
        .map_err(|e| AppError::message(format!("http client: {e}")))?;

    let request_id = args.request_id.clone();
    let emit_progress = |phase: &str, extracted: Option<u64>, total: Option<u64>| {
        let _ = app.emit(
            CLOUD_PROGRESS_EVENT,
            CloudProgressPayload {
                phase: phase.to_string(),
                extracted_pages: extracted,
                total_pages: total,
                request_id: request_id.clone(),
            },
        );
    };

    // 1) Submit the whole-document job (multipart file upload).
    emit_progress("uploading", None, None);
    let file_name = args
        .file_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("paper.pdf")
        .to_string();
    let job_id = submit_job(
        &client,
        &jobs_url,
        &auth,
        pdf_bytes,
        file_name,
        "application/pdf",
    )
    .await?;

    // 2) Poll until done (deadline guards a stuck job).
    let job_url = format!("{}/{}", jobs_url.trim_end_matches('/'), job_id);
    let started = Instant::now();
    let (json_url, data_info) = loop {
        if started.elapsed() > CLOUD_JOB_DEADLINE {
            return Err(AppError::message("Paddle job timed out"));
        }
        tokio::time::sleep(CLOUD_POLL_INTERVAL).await;
        let poll = client
            .get(&job_url)
            .header("Authorization", &auth)
            .send()
            .await
            .map_err(|e| AppError::message(format!("Paddle job poll failed: {e}")))?;
        let poll_status = poll.status();
        let poll_text = poll
            .text()
            .await
            .map_err(|e| AppError::message(format!("Paddle poll read failed: {e}")))?;
        if !poll_status.is_success() {
            let snippet: String = poll_text.chars().take(180).collect();
            return Err(AppError::message(format!(
                "Paddle job poll failed (HTTP {poll_status}): {snippet}"
            )));
        }
        let poll_value: Value = serde_json::from_str(&poll_text)
            .map_err(|e| AppError::message(format!("Unexpected Paddle poll response: {e}")))?;
        let data = poll_value.get("data").cloned().unwrap_or_default();
        let state = data
            .get("state")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let progress = data.get("extractProgress");
        let extracted = progress
            .and_then(|p| p.get("extractedPages"))
            .and_then(Value::as_u64);
        let total = progress
            .and_then(|p| p.get("totalPages"))
            .and_then(Value::as_u64);
        match state.as_str() {
            "done" => {
                let url = data
                    .get("resultUrl")
                    .and_then(|r| r.get("jsonUrl"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| AppError::message("Paddle job done without result URL"))?
                    .to_string();
                emit_progress("done", extracted, total);
                break (url, data.get("dataInfo").cloned());
            }
            "failed" => {
                let msg = data
                    .get("errorMsg")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown error");
                return Err(AppError::message(format!("Paddle job failed: {msg}")));
            }
            _ => emit_progress(&state, extracted, total),
        }
    };

    // 3) Download the JSONL result; each line carries page results.
    emit_progress("downloading", None, None);
    let result_response = client
        .get(&json_url)
        .send()
        .await
        .map_err(|e| AppError::message(format!("Paddle result download failed: {e}")))?;
    let result_status = result_response.status();
    let result_text = result_response
        .text()
        .await
        .map_err(|e| AppError::message(format!("Paddle result read failed: {e}")))?;
    if !result_status.is_success() {
        return Err(AppError::message(format!(
            "Paddle result download failed (HTTP {result_status})"
        )));
    }

    // Keep the raw JSONL for diagnosis (scale/field issues); best effort.
    let debug_path = crate::core::paths::agentero_cache_dir().join("paddle-last-result.jsonl");
    if let Some(parent) = debug_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(&debug_path, result_text.as_bytes()) {
        log::warn!(target: "agentero::layout_remote", "failed to write {debug_path:?}: {e}");
    } else {
        log::info!(target: "agentero::layout_remote", "raw cloud result saved to {debug_path:?}");
    }

    let mut pages: Vec<LayoutRemotePageResult> = Vec::new();
    let mut dim_sources: Vec<&'static str> = Vec::new();
    let mut unknown_diag: Option<String> = None;
    // `dataInfo.pages` covers the whole document; take the first occurrence.
    let mut rendered_pages: Vec<(u32, u32)> = Vec::new();
    for line in result_text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(line_value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let result = line_value
            .get("result")
            .cloned()
            .unwrap_or_else(|| line_value.clone());
        let Some(results) = result.get("layoutParsingResults").and_then(Value::as_array) else {
            continue;
        };
        let info = result.get("dataInfo").or(data_info.as_ref()).cloned();
        if rendered_pages.is_empty() {
            rendered_pages = parse_rendered_pages(info.as_ref());
        }
        for page in results {
            let page_index = pages.len();
            let (parsed, source) =
                parse_cloud_page(page, info.as_ref(), &rendered_pages, page_index);
            if source == "none" && unknown_diag.is_none() {
                // One-time dump so the missing rendered size can be diagnosed:
                // which fields exist, what dataInfo says, whether inputImage is
                // present (and how big), and a sample box.
                let keys: Vec<String> = page
                    .as_object()
                    .map(|o| o.keys().cloned().collect())
                    .unwrap_or_default();
                let data_info_raw: String = info
                    .as_ref()
                    .map(|v| v.to_string())
                    .unwrap_or_default()
                    .chars()
                    .take(300)
                    .collect();
                let input_image_chars = page
                    .get("inputImage")
                    .and_then(Value::as_str)
                    .map(str::len)
                    .unwrap_or(0);
                unknown_diag = Some(format!(
                    "page_keys={keys:?} dataInfo={data_info_raw} inputImage_chars={input_image_chars} first_box={:?}",
                    parsed.boxes.first()
                ));
            }
            dim_sources.push(source);
            pages.push(parsed);
        }
    }
    let unknown = dim_sources.iter().filter(|s| **s == "none").count();
    log::info!(
        target: "agentero::layout_remote",
        "cloud result: pages={} rendered_size_known={} unknown={}{}",
        pages.len(),
        pages.len() - unknown,
        unknown,
        unknown_diag
            .map(|d| format!(" | unknown-size diag: {d}"))
            .unwrap_or_default()
    );
    Ok(LayoutRemoteAnalyzePdfResult {
        pages,
        rendered_pages,
    })
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutRemoteProbeArgs {
    /// Base64-encoded tiny probe image (JPEG).
    pub image_base64: String,
    #[serde(default)]
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutRemoteProbeResult {
    pub job_id: String,
}

/// Connectivity probe: submit a tiny OCR job through the same async path;
/// a jobId means endpoint + token are valid. Runs in the Host so it is not
/// subject to WebView CORS and honors the app proxy.
pub async fn probe(args: LayoutRemoteProbeArgs) -> Result<LayoutRemoteProbeResult, AppError> {
    let (jobs_url, auth) = resolve_cloud_target(args.api_key.as_deref())?;
    if args.image_base64.trim().is_empty() {
        return Err(AppError::message("layout_remote: probe image is empty"));
    }
    let image_bytes = base64::engine::general_purpose::STANDARD
        .decode(args.image_base64.trim())
        .map_err(|e| AppError::message(format!("layout_remote: invalid probe image: {e}")))?;
    let client = network::client_builder()
        .timeout(Duration::from_secs(30))
        .redirect(Policy::limited(5))
        .build()
        .map_err(|e| AppError::message(format!("http client: {e}")))?;
    let job_id = submit_job(
        &client,
        &jobs_url,
        &auth,
        image_bytes,
        "probe.jpg".to_string(),
        "image/jpeg",
    )
    .await?;
    Ok(LayoutRemoteProbeResult { job_id })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jpeg_dimensions_reads_sof0() {
        // Minimal JPEG: SOI, SOF0 (11x7, 1 component), EOI.
        let mut bytes: Vec<u8> = vec![0xFF, 0xD8];
        bytes.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x0B, 0x08]);
        bytes.extend_from_slice(&7u16.to_be_bytes()); // height
        bytes.extend_from_slice(&11u16.to_be_bytes()); // width
        bytes.push(0x01);
        bytes.extend_from_slice(&[0xFF, 0xD9]);
        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        assert_eq!(jpeg_dimensions_base64(&b64), Some((11, 7)));
        assert_eq!(jpeg_dimensions_base64("not-base64!!"), None);
    }
}

pub mod commands;

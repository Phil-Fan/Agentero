//! OpenAI-compatible VLM OCR engine (SiliconFlow preset): render pages to
//! PNG in the isolated PDFium worker, then convert each page to markdown via
//! `POST {base}/chat/completions` with an inline `image_url` data URL.

use super::{BodyParseCtx, BodyParseEngine, BodyParseOutcome};
use crate::core::error::AppError;
use crate::features::network;
use async_trait::async_trait;
use base64::Engine as _;
use futures_util::stream::{self, StreamExt};
use serde_json::{json, Value};
use std::time::Duration;

pub(crate) const DEFAULT_VLM_BASE_URL: &str = "https://api.siliconflow.cn/v1";
pub(crate) const DEFAULT_VLM_MODEL: &str = "PaddlePaddle/PaddleOCR-VL-1.5";
const PAGE_CONCURRENCY: usize = 3;
const PAGE_TIMEOUT: Duration = Duration::from_secs(90);
/// Give up when more than this share of pages still fails after one retry.
const MAX_FAILED_PAGE_RATIO: f64 = 0.3;

pub(crate) struct OpenAiVlmBodyEngine;

#[async_trait]
impl BodyParseEngine for OpenAiVlmBodyEngine {
    fn id(&self) -> &'static str {
        "openaiCompatible"
    }

    async fn parse(&self, ctx: &BodyParseCtx<'_>) -> Result<BodyParseOutcome, AppError> {
        let api_key = ctx
            .credentials
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|k| !k.is_empty())
            .ok_or_else(|| {
                AppError::message("OpenAI-compatible OCR requires apiKey (Settings → PDF)")
            })?
            .to_string();
        let base = ctx
            .credentials
            .base_url
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(DEFAULT_VLM_BASE_URL)
            .trim_end_matches('/')
            .to_string();
        let model = ctx
            .credentials
            .model
            .as_deref()
            .map(str::trim)
            .filter(|m| !m.is_empty())
            .unwrap_or(DEFAULT_VLM_MODEL)
            .to_string();
        let prompt = prompt_for_model(&model);

        // Rendering runs in the killable PDFium worker; the guard keeps the
        // PNG directory alive until every page request finished.
        let (pages, guard) =
            super::super::run_liteparse_render_pngs(ctx.pdf_path, ctx.task_id).await?;
        if pages.is_empty() {
            return Err(AppError::message("PDF rendered no pages"));
        }
        let truncated = pages.len() >= super::super::VLM_MAX_PAGES;

        let client = network::client_builder()
            .timeout(PAGE_TIMEOUT)
            .build()
            .map_err(|e| AppError::message(format!("http client: {e}")))?;

        let total = pages.len();
        let mut page_futures = Vec::with_capacity(total);
        for (index, page) in pages.iter().enumerate() {
            page_futures.push(process_page(
                index,
                page,
                guard.path(),
                ctx,
                &client,
                &base,
                &api_key,
                &model,
                prompt,
            ));
        }
        let results: Vec<(usize, Result<String, AppError>)> = stream::iter(page_futures)
            .buffer_unordered(PAGE_CONCURRENCY)
            .collect()
            .await;

        if ctx.is_cancelled() {
            return Err(AppError::message(super::super::CANCELLED_MESSAGE));
        }

        let mut page_texts: Vec<String> = vec![String::new(); total];
        let mut failed = 0usize;
        for (index, outcome) in results {
            match outcome {
                Ok(text) => page_texts[index] = clean_page_markdown(&text),
                Err(e) => {
                    let msg = e.to_string();
                    if msg.contains(super::super::CANCELLED_MESSAGE) {
                        return Err(e);
                    }
                    failed += 1;
                    page_texts[index] = format!("<!-- page {}: OCR failed -->", index + 1);
                }
            }
        }
        if failed as f64 > total as f64 * MAX_FAILED_PAGE_RATIO {
            return Err(AppError::message(format!(
                "VLM OCR failed on {failed}/{total} pages"
            )));
        }

        let mut markdown = page_texts
            .into_iter()
            .filter(|t| !t.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n\n");
        if truncated {
            markdown.push_str(&format!(
                "\n\n<!-- truncated to the first {} pages -->",
                super::super::VLM_MAX_PAGES
            ));
        }
        Ok(BodyParseOutcome {
            markdown,
            body_source: "vlm".to_string(),
            body_quality: "medium".to_string(),
        })
    }
}

#[allow(clippy::too_many_arguments)]
async fn process_page(
    index: usize,
    page: &super::super::RenderedPngPage,
    dir: &std::path::Path,
    ctx: &BodyParseCtx<'_>,
    client: &reqwest::Client,
    base: &str,
    api_key: &str,
    model: &str,
    prompt: &str,
) -> (usize, Result<String, AppError>) {
    if page.is_solid_fill {
        return (index, Ok(String::new()));
    }
    if ctx.is_cancelled() {
        return (
            index,
            Err(AppError::message(super::super::CANCELLED_MESSAGE)),
        );
    }
    let png = match std::fs::read(dir.join(&page.file)) {
        Ok(bytes) => bytes,
        Err(e) => return (index, Err(AppError::message(format!("read page png: {e}")))),
    };
    let mut outcome = ocr_page(client, base, api_key, model, prompt, &png).await;
    if outcome.is_err() && !ctx.is_cancelled() {
        outcome = ocr_page(client, base, api_key, model, prompt, &png).await;
    }
    (index, outcome)
}

/// Model-specific OCR prompt (SiliconFlow-hosted models need exact prompts).
fn prompt_for_model(model: &str) -> &'static str {
    let m = model.to_ascii_lowercase();
    if m.contains("deepseek-ocr") {
        "<image>\n<|grounding|>Convert the document to markdown."
    } else if m.contains("paddleocr") {
        "OCR:"
    } else {
        "Convert this document page to markdown. Output markdown only."
    }
}

async fn ocr_page(
    client: &reqwest::Client,
    base: &str,
    api_key: &str,
    model: &str,
    prompt: &str,
    png_bytes: &[u8],
) -> Result<String, AppError> {
    let data_url = format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png_bytes)
    );
    let body = json!({
        "model": model,
        "temperature": 0,
        "messages": [{
            "role": "user",
            "content": [
                { "type": "image_url", "image_url": { "url": data_url } },
                { "type": "text", "text": prompt }
            ]
        }]
    });
    let response = client
        .post(format!("{base}/chat/completions"))
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| AppError::message(format!("VLM OCR request failed: {e}")))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|e| AppError::message(format!("VLM OCR response read failed: {e}")))?;
    if !status.is_success() {
        let snippet: String = text.chars().take(180).collect();
        return Err(AppError::message(format!(
            "VLM OCR request failed (HTTP {status}): {snippet}"
        )));
    }
    let value: Value = serde_json::from_str(&text)
        .map_err(|e| AppError::message(format!("Unexpected VLM OCR response: {e}")))?;
    value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| AppError::message("Unexpected VLM OCR response: missing content"))
}

/// Strip DeepSeek-OCR grounding control tokens (keep the referenced text)
/// and unwrap a whole-page markdown code fence.
fn clean_page_markdown(text: &str) -> String {
    let mut out = text.replace("<|ref|>", "").replace("<|/ref|>", "");
    while let Some(start) = out.find("<|det|>") {
        match out[start..].find("<|/det|>") {
            Some(end_rel) => out.replace_range(start..start + end_rel + "<|/det|>".len(), ""),
            None => out.replace_range(start..start + "<|det|>".len(), ""),
        }
    }
    let trimmed = out.trim();
    let unfenced = trimmed
        .strip_prefix("```markdown")
        .or_else(|| trimmed.strip_prefix("```md"))
        .or_else(|| trimmed.strip_prefix("```"))
        .and_then(|rest| rest.strip_suffix("```"))
        .map(str::trim)
        .unwrap_or(trimmed);
    unfenced.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_prompt_by_model() {
        assert_eq!(prompt_for_model("PaddlePaddle/PaddleOCR-VL-1.5"), "OCR:");
        assert_eq!(
            prompt_for_model("deepseek-ai/DeepSeek-OCR"),
            "<image>\n<|grounding|>Convert the document to markdown."
        );
        assert_eq!(
            prompt_for_model("some/other-vlm"),
            "Convert this document page to markdown. Output markdown only."
        );
    }

    #[test]
    fn cleans_grounding_and_fences() {
        assert_eq!(
            clean_page_markdown("<|ref|>Title<|/ref|><|det|>[[1,2,3,4]]<|/det|> body"),
            "Title body"
        );
        assert_eq!(clean_page_markdown("```markdown\n# H1\n```"), "# H1");
        assert_eq!(clean_page_markdown("plain"), "plain");
        assert_eq!(clean_page_markdown("<|det|>dangling"), "dangling");
    }
}

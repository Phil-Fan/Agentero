//! Paddle (PP-StructureV3) body-parse engine: run the shared cloud OCR job
//! and join the per-page markdown carried in the JSONL result.

use super::{BodyParseCtx, BodyParseEngine, BodyParseOutcome};
use crate::core::error::AppError;
use crate::features::layout_remote::paddle::run_paddle_ocr_job;
use async_trait::async_trait;
use serde_json::Value;

pub(crate) struct PaddleBodyEngine;

#[async_trait]
impl BodyParseEngine for PaddleBodyEngine {
    fn id(&self) -> &'static str {
        "paddle"
    }

    async fn parse(&self, ctx: &BodyParseCtx<'_>) -> Result<BodyParseOutcome, AppError> {
        let pdf_bytes =
            std::fs::read(ctx.pdf_path).map_err(|e| AppError::message(format!("read pdf: {e}")))?;
        let file_name = ctx
            .pdf_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("paper.pdf")
            .to_string();
        let (jsonl, _data_info) = run_paddle_ocr_job(
            ctx.credentials.api_key.as_deref(),
            pdf_bytes,
            file_name,
            "application/pdf",
            &|_, _, _| {},
            &|| ctx.is_cancelled(),
        )
        .await?;
        let markdown = markdown_from_jsonl(&jsonl);
        if markdown.trim().is_empty() {
            return Err(AppError::message(
                "Paddle result carried no markdown content",
            ));
        }
        Ok(BodyParseOutcome {
            markdown,
            body_source: "paddle".to_string(),
            body_quality: "high".to_string(),
        })
    }
}

/// Join per-page markdown from the PP-StructureV3 JSONL result. Tolerant to
/// schema drift: `markdown.text`, a plain `markdown` string, then the
/// `prunedResult.parsing_res_list[].block_content` blocks.
fn markdown_from_jsonl(jsonl: &str) -> String {
    let mut pages: Vec<String> = Vec::new();
    for line in jsonl.lines() {
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
        for page in results {
            if let Some(text) = page_markdown(page) {
                let text = text.trim();
                if !text.is_empty() {
                    pages.push(text.to_string());
                }
            }
        }
    }
    pages.join("\n\n")
}

fn page_markdown(page: &Value) -> Option<String> {
    if let Some(text) = page
        .get("markdown")
        .and_then(|m| m.get("text"))
        .and_then(Value::as_str)
    {
        return Some(text.to_string());
    }
    if let Some(text) = page.get("markdown").and_then(Value::as_str) {
        return Some(text.to_string());
    }
    let blocks = page
        .get("prunedResult")
        .and_then(|p| p.get("parsing_res_list"))
        .and_then(Value::as_array)?;
    let joined = blocks
        .iter()
        .filter_map(|b| b.get("block_content").and_then(Value::as_str))
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    (!joined.is_empty()).then_some(joined)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_markdown_text_pages() {
        let jsonl = concat!(
            r##"{"result":{"layoutParsingResults":[{"markdown":{"text":"# Page 1"}},{"markdown":{"text":"Page 2 body"}}]}}"##,
            "\n",
            r##"{"result":{"layoutParsingResults":[{"markdown":{"text":"  "}}]}}"##,
        );
        assert_eq!(markdown_from_jsonl(jsonl), "# Page 1\n\nPage 2 body");
    }

    #[test]
    fn falls_back_to_plain_markdown_string_and_blocks() {
        let plain = r#"{"result":{"layoutParsingResults":[{"markdown":"plain md"}]}}"#;
        assert_eq!(markdown_from_jsonl(plain), "plain md");

        let blocks = r#"{"result":{"layoutParsingResults":[{"prunedResult":{"parsing_res_list":[{"block_label":"text","block_content":"Alpha"},{"block_label":"text","block_content":"Beta"}]}}]}}"#;
        assert_eq!(markdown_from_jsonl(blocks), "Alpha\n\nBeta");
    }

    #[test]
    fn ignores_invalid_lines_and_missing_fields() {
        let jsonl = "not-json\n{\"result\":{}}\n{\"result\":{\"layoutParsingResults\":[{}]}}";
        assert_eq!(markdown_from_jsonl(jsonl), "");
    }
}

//! MinerU body-parse engine: run the shared cloud extract and read the
//! `full.md` markdown from the result zip.

use super::{BodyParseCtx, BodyParseEngine, BodyParseOutcome};
use crate::core::error::AppError;
use crate::features::layout_remote::engine::ProviderCredentials;
use crate::features::layout_remote::mineru::{read_zip_entry_by_suffix, run_mineru_extract};
use async_trait::async_trait;

pub(crate) struct MineruBodyEngine;

#[async_trait]
impl BodyParseEngine for MineruBodyEngine {
    fn id(&self) -> &'static str {
        "mineru"
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
        let credentials = ProviderCredentials {
            api_key: ctx.credentials.api_key.clone(),
            base_url: ctx.credentials.base_url.clone(),
        };
        let zip_bytes =
            run_mineru_extract(&credentials, pdf_bytes, &file_name, &|_, _, _| {}, &|| {
                ctx.is_cancelled()
            })
            .await?;
        let markdown = read_zip_entry_by_suffix(&zip_bytes, "full.md")?;
        Ok(BodyParseOutcome {
            markdown,
            body_source: "mineru".to_string(),
            body_quality: "high".to_string(),
        })
    }
}

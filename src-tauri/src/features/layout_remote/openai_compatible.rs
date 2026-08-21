//! OpenAI-compatible provider registered for the settings connectivity
//! probe only — PDF body OCR runs in `import::pdf_parse::engines`, and this
//! provider has no layout-analysis capability.

use crate::core::error::AppError;
use crate::features::layout_remote::engine::{AnalyzeCtx, ProviderCredentials, RemoteLayoutEngine};
use crate::features::layout_remote::{
    LayoutRemoteAnalyzePdfResult, LayoutRemoteProbeArgs, LayoutRemoteProbeResult,
};
use crate::features::network;
use async_trait::async_trait;
use std::time::Duration;

const DEFAULT_BASE_URL: &str = "https://api.siliconflow.cn/v1";

pub struct OpenAiCompatibleEngine;

#[async_trait]
impl RemoteLayoutEngine for OpenAiCompatibleEngine {
    fn id(&self) -> &'static str {
        "openaiCompatible"
    }

    async fn analyze_pdf(
        &self,
        _ctx: AnalyzeCtx,
    ) -> Result<LayoutRemoteAnalyzePdfResult, AppError> {
        Err(AppError::message(
            "openaiCompatible is not a layout-analysis provider",
        ))
    }

    /// Validate key + endpoint with the standard `GET {base}/models` listing.
    async fn probe(
        &self,
        credentials: &ProviderCredentials,
        _args: LayoutRemoteProbeArgs,
    ) -> Result<LayoutRemoteProbeResult, AppError> {
        let api_key = credentials
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|k| !k.is_empty())
            .ok_or_else(|| AppError::message("OpenAI-compatible OCR requires apiKey"))?;
        let base = credentials
            .base_url
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(DEFAULT_BASE_URL)
            .trim_end_matches('/');
        let client = network::client_builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| AppError::message(format!("http client: {e}")))?;
        let response = client
            .get(format!("{base}/models"))
            .bearer_auth(api_key)
            .send()
            .await
            .map_err(|e| AppError::message(format!("OpenAI-compatible probe failed: {e}")))?;
        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            let snippet: String = text.chars().take(180).collect();
            return Err(AppError::message(format!(
                "OpenAI-compatible probe failed (HTTP {status}): {snippet}"
            )));
        }
        Ok(LayoutRemoteProbeResult {
            job_id: "openai-compatible-probe-ok".to_string(),
        })
    }
}

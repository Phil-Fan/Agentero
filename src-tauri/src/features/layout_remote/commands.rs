//! Tauri command shell for the remote PP-StructureV3 layout provider
//! (AI Studio async OCR job).

use crate::core::error::{map_err, ApiResult};
use crate::features::layout_remote::{
    self, LayoutRemoteAnalyzePdfArgs, LayoutRemoteAnalyzePdfResult, LayoutRemoteProbeArgs,
    LayoutRemoteProbeResult,
};
use crate::features::settings::{is_translate_api_key_mask, AppSettingsStore};
use tauri::{AppHandle, Manager};

/// Inject the stored paddle access token before any `.await`
/// (managed state must not be held across await).
fn inject_paddle_credentials(app: &AppHandle, api_key: &mut Option<String>) {
    let store = app.state::<AppSettingsStore>();
    let needs_stored_key = api_key
        .as_deref()
        .map(|k| {
            let t = k.trim();
            t.is_empty() || is_translate_api_key_mask(t)
        })
        .unwrap_or(true);
    if needs_stored_key {
        if let Some(key) = store.layout_api_key("paddle") {
            *api_key = Some(key);
        } else if api_key.as_deref().is_some_and(is_translate_api_key_mask) {
            *api_key = None;
        }
    }
}

#[tauri::command]
pub async fn layout_remote_analyze_pdf(
    app: AppHandle,
    mut args: LayoutRemoteAnalyzePdfArgs,
) -> ApiResult<LayoutRemoteAnalyzePdfResult> {
    use crate::core::log_util::OpTimer;

    // Host keeps the real access token; the WebView sends a `*`-mask or nothing.
    inject_paddle_credentials(&app, &mut args.api_key);

    let op = OpTimer::start_with(
        "layout_remote_analyze_pdf",
        format!("pdf_chars={}", args.pdf_base64.len()),
    );
    match layout_remote::analyze_pdf(&app, args).await {
        Ok(r) => {
            op.finish_ok();
            ApiResult::ok(r)
        }
        Err(e) => {
            op.finish_err(&e);
            map_err(e)
        }
    }
}

#[tauri::command]
pub async fn layout_remote_probe(
    app: AppHandle,
    mut args: LayoutRemoteProbeArgs,
) -> ApiResult<LayoutRemoteProbeResult> {
    use crate::core::log_util::OpTimer;

    inject_paddle_credentials(&app, &mut args.api_key);

    let op = OpTimer::start("layout_remote_probe");
    match layout_remote::probe(args).await {
        Ok(r) => {
            op.finish_ok();
            ApiResult::ok(r)
        }
        Err(e) => {
            op.finish_err(&e);
            map_err(e)
        }
    }
}

use crate::core::error::{map_err, ApiResult};

use super::{load_system_cjk_font, ExportFontPayload};

/// Read a system font suitable for embedding selectable CJK text in PDF export.
#[tauri::command]
pub fn export_system_cjk_font() -> ApiResult<ExportFontPayload> {
    match load_system_cjk_font() {
        Ok(payload) => ApiResult::ok(payload),
        Err(e) => map_err(e),
    }
}

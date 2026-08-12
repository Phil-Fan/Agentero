//! Zotero migration commands: scan a Zotero data directory and migrate its
//! library into the catalog. Fully local (no Translator).

use crate::core::error::ApiResult;
use crate::features::import::{
    migrate_zotero, scan_zotero, MigrateProgress, ZoteroMigrateArgs, ZoteroMigrateResult,
    ZoteroScan, ZoteroScanArgs,
};
use tauri::ipc::Channel;

/// Read-only preview of a Zotero data directory (item + local-PDF counts).
#[tauri::command]
pub fn zotero_scan(args: ZoteroScanArgs) -> ApiResult<ZoteroScan> {
    use crate::core::log_util::{trunc, OpTimer};

    let op = OpTimer::start_with(
        "zotero_scan",
        format!("path={}", trunc(&args.zotero_dir, 160)),
    );
    op.finish_result(scan_zotero(args))
}

/// Migrate a Zotero library into `papers/…` + catalog; optionally copy PDFs.
/// Streams `{current,total,phase}` progress to the UI via `on_progress`.
#[tauri::command]
pub async fn zotero_migrate(
    app: tauri::AppHandle,
    args: ZoteroMigrateArgs,
    on_progress: Channel<MigrateProgress>,
) -> ApiResult<ZoteroMigrateResult> {
    use crate::core::log_util::{trunc, OpTimer};

    let op = OpTimer::start_with(
        "zotero_migrate",
        format!("path={}", trunc(&args.zotero_dir, 160)),
    );
    let report = move |current, total, phase: &str| {
        let _ = on_progress.send(MigrateProgress {
            current,
            total,
            phase: phase.to_string(),
        });
    };
    op.finish_result_ok_extra(migrate_zotero(args, report, Some(&app)).await, |r| {
        format!("imported={} skipped={}", r.imported, r.skipped)
    })
}

//! JobCenter runners for the import domain (`ParseBody` / `DownloadAssets`).
//!
//! Registered at app startup (see `app::run`) so the JobCenter stays a pure
//! scheduler with no edges into import. The `DownloadAssets` runner owns the
//! post-download follow-up orchestration (PAPER.md backfill + layout pass).

use crate::features::catalog::CapsCache;
use crate::features::jobs::{
    emit_job_changed, JobCenter, JobKind, JobLane, RunOutcome, StartOutcome, StartedJob,
};
use std::sync::Arc;
use tauri::Manager;

/// Register the import job runners with the JobCenter.
pub fn register_job_runners(center: &JobCenter) {
    center.register_runner(JobKind::ParseBody, Arc::new(parse_body_runner));
    center.register_runner(JobKind::DownloadAssets, Arc::new(download_assets_runner));
}

/// Runner for [`JobKind::ParseBody`]: generate `PAPER.md` from PDF/TeX.
fn parse_body_runner(
    center: JobCenter,
    app: tauri::AppHandle,
    started: StartedJob,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
    center.run_job(app, started, |_center, app, started| async move {
        let StartedJob {
            snapshot,
            vault_path: vault,
            paper_path: path,
            force,
            task_id,
        } = started;
        let task_id = task_id.unwrap_or_else(|| snapshot.id.clone());
        let cache = app.state::<CapsCache>();
        let result = crate::features::import::pdf_parse::parse_paper_body(
            crate::features::import::pdf_parse::PaperParseBodyArgs {
                vault_path: vault.to_string_lossy().to_string(),
                path,
                force,
                task_id: Some(task_id.clone()),
            },
            Some(&cache),
        )
        .await;
        crate::features::agent::background_tasks::finish(&task_id);
        // A skipped or successful parse returns Ok with no error; a real
        // liteparse failure also returns Ok, carrying the reason.
        match result {
            Ok(parsed) => match parsed.error {
                Some(message) => RunOutcome::Failed(Some(message)),
                None => RunOutcome::Succeeded,
            },
            Err(e) => RunOutcome::Failed(Some(e.to_string())),
        }
    })
}

/// Runner for [`JobKind::DownloadAssets`]: download PDF/TeX for a paper, then
/// backfill `PAPER.md` + layout for the freshly-downloaded assets. Byte-level
/// progress flows via `background-task:progress` (task_id defaults to the job
/// id) to the projected "download" row.
fn download_assets_runner(
    center: JobCenter,
    app: tauri::AppHandle,
    started: StartedJob,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
    center.run_job(app, started, |center, app, started| async move {
        let StartedJob {
            snapshot,
            vault_path: vault,
            paper_path: path,
            task_id,
            ..
        } = started;
        let task_id = task_id.unwrap_or_else(|| snapshot.id.clone());
        let cache = app.state::<CapsCache>();
        let args = crate::features::import::PaperDownloadAssetsArgs {
            vault_path: vault.to_string_lossy().to_string(),
            path: path.clone(),
            task_id: Some(task_id),
        };
        let result = crate::features::import::download_paper_assets_with_progress(
            args,
            Some(&app),
            Some(&cache),
        )
        .await;
        // Assets changed on disk: drop the stale capability bits.
        cache.invalidate(&vault, &path);

        match result {
            Ok(_) => {
                // Follow-ups for the freshly-downloaded PDF: PAPER.md + layout.
                if cache.caps_for(&vault, &path).needs_paper_md() {
                    let snap = center
                        .enqueue_parse_body(&vault, &path, JobLane::Normal, false, None)
                        .await;
                    emit_job_changed(&app, snap.clone());
                    if let StartOutcome::Started(started) = center.try_start(&snap.id).await {
                        center.spawn_runner(&app, started);
                    }
                }
                let backend = app
                    .state::<crate::features::settings::AppSettingsStore>()
                    .layout_backend();
                center.apply_layout_backend(&backend).await;
                let lsnap = center
                    .enqueue_layout_analyze(&vault, &path, JobLane::Normal, false)
                    .await;
                emit_job_changed(&app, lsnap.clone());
                if let StartOutcome::Started(started) = center.try_start(&lsnap.id).await {
                    center.spawn_runner(&app, started);
                }
                RunOutcome::Succeeded
            }
            Err(e) => RunOutcome::Failed(Some(e.to_string())),
        }
    })
}

//! Tauri commands for the device-local activity log.

use crate::core::error::{map_err, ApiResult};
use crate::features::settings::AppSettingsStore;
use crate::features::usage::events::{
    clear_default, list_default, record_default, summarize_default, ListFilter, UsageEvent,
    UsageKindCount, UsageRecord,
};
use serde::Deserialize;
use tauri::State;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityRecordArgs {
    pub events: Vec<UsageRecord>,
}

#[tauri::command]
pub fn activity_record_events(
    store: State<'_, AppSettingsStore>,
    args: ActivityRecordArgs,
) -> ApiResult<usize> {
    match store.get() {
        Ok(snapshot) if !snapshot.settings.usage_tracking_enabled => {
            return ApiResult::ok(0);
        }
        Ok(_) => {}
        Err(e) => return map_err(e),
    }
    match record_default(&args.events) {
        Ok(n) => ApiResult::ok(n),
        Err(e) => map_err(e),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageListArgs {
    #[serde(default)]
    pub vault: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub since: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[tauri::command]
pub fn usage_list(args: UsageListArgs) -> ApiResult<Vec<UsageEvent>> {
    let filter = ListFilter {
        vault: args.vault,
        kind: args.kind,
        path_prefix: args.path,
        since: args.since,
        limit: args.limit.unwrap_or(100),
    };
    match list_default(&filter) {
        Ok(rows) => ApiResult::ok(rows),
        Err(e) => map_err(e),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSummaryArgs {
    #[serde(default)]
    pub vault: Option<String>,
    #[serde(default)]
    pub since: Option<String>,
}

#[tauri::command]
pub fn usage_summary(args: UsageSummaryArgs) -> ApiResult<Vec<UsageKindCount>> {
    match summarize_default(args.vault.as_deref(), args.since.as_deref()) {
        Ok(rows) => ApiResult::ok(rows),
        Err(e) => map_err(e),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageClearArgs {
    #[serde(default)]
    pub vault: Option<String>,
}

#[tauri::command]
pub fn usage_clear(args: UsageClearArgs) -> ApiResult<u64> {
    match clear_default(args.vault.as_deref()) {
        Ok(n) => ApiResult::ok(n),
        Err(e) => map_err(e),
    }
}

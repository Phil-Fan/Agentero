//! App settings commands — durable XDG config file.

use crate::core::error::{map_err, ApiResult};
#[cfg(not(target_os = "ios"))]
use crate::features::connector::ConnectorController;
use crate::features::settings::{AppSettings, AppSettingsStore, SettingsGetResult};
#[cfg(not(target_os = "ios"))]
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub fn settings_get(store: State<'_, AppSettingsStore>) -> ApiResult<SettingsGetResult> {
    match store.get() {
        Ok(r) => ApiResult::ok(r),
        Err(e) => map_err(e),
    }
}

/// Return unique system font family names (sorted). Empty on mobile / failure.
///
/// Scanning system fonts is slow (disk walk on Windows), so it runs in
/// `run_blocking` and the result is cached for the process lifetime — the
/// installed-font set does not change while the app is running.
#[tauri::command]
pub async fn list_system_fonts() -> ApiResult<Vec<String>> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    {
        ApiResult::ok(Vec::new())
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        use crate::core::blocking::run_blocking;
        use std::sync::OnceLock;

        static FONTS: OnceLock<Vec<String>> = OnceLock::new();
        if let Some(names) = FONTS.get() {
            return ApiResult::ok(names.clone());
        }
        run_blocking(move || {
            let names = FONTS.get_or_init(|| {
                use std::collections::BTreeSet;
                let mut db = fontdb::Database::new();
                db.load_system_fonts();
                let mut names = BTreeSet::new();
                for face in db.faces() {
                    for (family, _) in &face.families {
                        let t = family.trim();
                        if !t.is_empty() {
                            names.insert(t.to_string());
                        }
                    }
                }
                names.into_iter().collect()
            });
            ApiResult::ok(names.clone())
        })
        .await
    }
}

#[tauri::command]
#[cfg(not(target_os = "ios"))]
pub fn settings_set(
    app: AppHandle,
    store: State<'_, AppSettingsStore>,
    agents: State<'_, crate::features::agent::AgentRegistry>,
    connector: State<'_, Arc<ConnectorController>>,
    settings: AppSettings,
) -> ApiResult<AppSettings> {
    if let Err(e) = crate::features::network::configure_proxy(
        settings.network_proxy_enabled,
        &settings.network_proxy_url,
    ) {
        return map_err(e);
    }
    match store.set(settings) {
        Ok(s) => {
            let _ = agents.set_proxy(s.network_proxy_enabled, s.network_proxy_url.clone());
            // `set_port` is async (it may rebind the listener); the result was
            // always discarded here and the controller emits its own status
            // event, so run it on the runtime instead of blocking this handler.
            let ctrl = Arc::clone(&connector);
            let port = s.connector_port;
            tauri::async_runtime::spawn(async move {
                let _ = ctrl.set_port(port).await;
            });
            // Keep every window's settings cache fresh (settings window, main windows).
            let _ = app.emit("settings:changed", &s);
            ApiResult::ok(s)
        }
        Err(e) => map_err(e),
    }
}

/// iOS has no local Connector process. Settings remain durable and are still
/// broadcast to the WebView, but no desktop-only port update is attempted.
#[tauri::command]
#[cfg(target_os = "ios")]
pub fn settings_set(
    app: AppHandle,
    store: State<'_, AppSettingsStore>,
    agents: State<'_, crate::features::agent::AgentRegistry>,
    settings: AppSettings,
) -> ApiResult<AppSettings> {
    if let Err(e) = crate::features::network::configure_proxy(
        settings.network_proxy_enabled,
        &settings.network_proxy_url,
    ) {
        return map_err(e);
    }
    match store.set(settings) {
        Ok(s) => {
            let _ = agents.set_proxy(s.network_proxy_enabled, s.network_proxy_url.clone());
            let _ = app.emit("settings:changed", &s);
            ApiResult::ok(s)
        }
        Err(e) => map_err(e),
    }
}

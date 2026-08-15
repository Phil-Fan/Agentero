//! Process-wide network configuration shared by Host HTTP clients.

use crate::core::error::AppError;
use std::sync::{OnceLock, RwLock};
use std::time::Duration;

static PROXY_URL: OnceLock<RwLock<Option<String>>> = OnceLock::new();
static SHARED_CLIENT: OnceLock<RwLock<Option<CachedClient>>> = OnceLock::new();

struct CachedClient {
    proxy: Option<String>,
    client: reqwest::Client,
}

fn proxy_slot() -> &'static RwLock<Option<String>> {
    PROXY_URL.get_or_init(|| RwLock::new(None))
}

/// Configure the proxy used by every Host-created reqwest client.
pub fn configure_proxy(enabled: bool, url: &str) -> Result<(), AppError> {
    let normalized = url.trim().to_string();
    let next = if enabled {
        if normalized.is_empty() {
            return Err(AppError::message("network proxy URL is required"));
        }
        reqwest::Proxy::all(&normalized)
            .map_err(|e| AppError::message(format!("invalid network proxy URL: {e}")))?;
        Some(normalized)
    } else {
        None
    };

    let mut guard = proxy_slot()
        .write()
        .map_err(|_| AppError::message("network proxy lock poisoned"))?;
    *guard = next;
    drop(guard);
    if let Some(slot) = SHARED_CLIENT.get() {
        if let Ok(mut cached) = slot.write() {
            *cached = None;
        }
    }
    Ok(())
}

fn cached_slot() -> &'static RwLock<Option<CachedClient>> {
    SHARED_CLIENT.get_or_init(|| RwLock::new(None))
}

/// A process-wide reqwest client so TLS sessions and HTTP keep-alive survive
/// across plaza / Cool Papers requests. Rebuilt when the proxy setting changes.
pub fn shared_client() -> Result<reqwest::Client, AppError> {
    let proxy = proxy_slot().read().ok().and_then(|guard| guard.clone());
    {
        let guard = cached_slot()
            .read()
            .map_err(|_| AppError::message("network client lock poisoned"))?;
        if let Some(cached) = guard.as_ref() {
            if cached.proxy == proxy {
                return Ok(cached.client.clone());
            }
        }
    }
    let client = client_builder()
        .pool_idle_timeout(Duration::from_secs(90))
        .pool_max_idle_per_host(8)
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|e| AppError::message(format!("http client: {e}")))?;
    let mut guard = cached_slot()
        .write()
        .map_err(|_| AppError::message("network client lock poisoned"))?;
    if let Some(cached) = guard.as_ref() {
        if cached.proxy == proxy {
            return Ok(cached.client.clone());
        }
    }
    *guard = Some(CachedClient {
        proxy,
        client: client.clone(),
    });
    Ok(client)
}

/// Build a reqwest client builder with the current process-wide proxy.
pub fn client_builder() -> reqwest::ClientBuilder {
    let proxy = proxy_slot().read().ok().and_then(|guard| guard.clone());
    let builder = reqwest::Client::builder();
    match proxy {
        Some(url) => match reqwest::Proxy::all(&url) {
            Ok(proxy) => builder.proxy(proxy),
            Err(error) => {
                log::error!(target: "agentero::network", "invalid configured proxy: {error}");
                builder
            }
        },
        None => builder,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_supported_proxy_urls() {
        for url in [
            "http://127.0.0.1:7890",
            "https://proxy.example.test:8443",
            "socks5h://127.0.0.1:1080",
        ] {
            configure_proxy(true, url).expect("proxy URL should be accepted");
        }
        configure_proxy(false, "").expect("proxy should be disabled");
    }

    #[test]
    fn rejects_enabled_empty_proxy() {
        let error = configure_proxy(true, " ").expect_err("empty proxy should fail");
        assert!(error.to_string().contains("proxy URL is required"));
    }
}

//! Process-wide HTTP plumbing shared by Host features and the headless CLI:
//! proxy configuration, client factories, the product User-Agent, and
//! error-body truncation.

use crate::core::error::AppError;
use std::sync::{OnceLock, RwLock};
use std::time::Duration;

/// Product User-Agent sent by Host HTTP clients by default.
///
/// The repo + mailto contacts keep Crossref / Semantic Scholar requests in
/// their polite pools.
pub const USER_AGENT: &str = concat!(
    "Agentero/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/poco-ai/agentero; mailto:agentero@users.noreply.github.com)"
);

/// Browser-like UA for endpoints that reject non-browser agents with HTTP 403
/// (PLOS / IEEE / Springer publisher PDFs, free web-MT endpoints). Use only
/// where a browser is deliberately impersonated; prefer [`USER_AGENT`]
/// everywhere else.
pub const BROWSER_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36";

/// Redirect cap applied by [`client`] (reqwest's own default is 10).
pub const DEFAULT_REDIRECT_LIMIT: usize = 5;

/// How many chars of an HTTP error body [`http_err_snippet`] keeps.
const ERROR_SNIPPET_CHARS: usize = 180;

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
        .redirect(reqwest::redirect::Policy::limited(DEFAULT_REDIRECT_LIMIT))
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
///
/// Prefer [`client`] / [`client_with`]; reach for this directly only when a
/// flow must deviate from their defaults (e.g. no timeout at all).
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

/// Standard Host HTTP client: [`USER_AGENT`], the configured proxy, `timeout`,
/// and at most [`DEFAULT_REDIRECT_LIMIT`] redirects.
pub fn client(timeout: Duration) -> Result<reqwest::Client, AppError> {
    client_with(timeout, DEFAULT_REDIRECT_LIMIT, USER_AGENT)
}

/// [`client`] with an explicit redirect cap and User-Agent — for deeper
/// redirect chains (model / asset downloads) or browser impersonation
/// ([`BROWSER_USER_AGENT`]).
pub fn client_with(
    timeout: Duration,
    redirect_limit: usize,
    user_agent: &str,
) -> Result<reqwest::Client, AppError> {
    client_builder()
        .timeout(timeout)
        .user_agent(user_agent)
        .redirect(reqwest::redirect::Policy::limited(redirect_limit))
        .build()
        .map_err(|e| AppError::message(format!("http client: {e}")))
}

/// First [`ERROR_SNIPPET_CHARS`] chars of an HTTP response body, for embedding
/// in error messages.
pub fn http_err_snippet(text: &str) -> String {
    text.chars().take(ERROR_SNIPPET_CHARS).collect()
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

    #[test]
    fn snippet_truncates_long_bodies() {
        let body = "x".repeat(ERROR_SNIPPET_CHARS + 40);
        assert_eq!(http_err_snippet(&body).len(), ERROR_SNIPPET_CHARS);
        assert_eq!(http_err_snippet("short body"), "short body");
    }
}

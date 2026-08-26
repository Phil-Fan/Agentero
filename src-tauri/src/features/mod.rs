//! Domain features (feature-first layout, aligned with frontend `src/lib`).
//!
//! Each submodule owns its service logic and thin `commands` shells.
//! The headless CLI may import non-agent features; BYOA (`agent`) is desktop-only.

#[cfg(feature = "desktop")]
pub mod agent;
#[cfg(feature = "desktop")]
pub mod arxiv_proxy;
#[cfg(feature = "desktop")]
pub mod bridge;
pub mod catalog;
#[cfg(feature = "desktop")]
pub mod cli_install;
#[cfg(feature = "desktop")]
pub mod connector;
#[cfg(feature = "desktop")]
pub mod coolpapers;
pub mod doctor;
#[cfg(feature = "desktop")]
pub mod export;
pub mod feeds;
pub mod import;
#[cfg(feature = "desktop")]
pub mod jobs;
#[cfg(feature = "desktop")]
pub mod layout_model;
#[cfg(feature = "desktop")]
pub mod layout_remote;
pub mod lifecycle;
#[cfg(feature = "desktop")]
pub mod mcp;
#[cfg(feature = "desktop")]
pub mod modelscope_proxy;
pub mod open_request;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod pdf_locate;
#[cfg(feature = "desktop")]
pub mod recommend;
pub mod refs;
#[cfg(feature = "desktop")]
pub mod remote;
pub mod rename;
#[cfg(feature = "desktop")]
pub mod search;
#[cfg(feature = "desktop")]
pub mod settings;
#[cfg(feature = "desktop")]
pub mod site_proxy;
#[cfg(feature = "desktop")]
pub mod sync;
#[cfg(all(
    feature = "desktop",
    not(any(target_os = "android", target_os = "ios"))
))]
pub mod telemetry;
#[cfg(feature = "desktop")]
pub mod terminal;
pub mod translate;
pub mod trash;
pub mod usage;
pub mod vault;
#[cfg(feature = "desktop")]
pub mod watcher;
pub mod wiki;
#[cfg(feature = "desktop")]
pub mod window;
pub mod zotero;
#[cfg(feature = "desktop")]
pub mod zotero_sync;

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
pub mod import;
#[cfg(feature = "desktop")]
pub mod jobs;
#[cfg(feature = "desktop")]
pub mod layout_model;
#[cfg(feature = "desktop")]
pub mod layout_remote;
pub mod network;
pub mod open_request;
pub mod refs;
#[cfg(feature = "desktop")]
pub mod remote;
#[cfg(feature = "desktop")]
pub mod search;
#[cfg(feature = "desktop")]
pub mod settings;
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
#[cfg(feature = "desktop")]
pub mod zotero_sync;

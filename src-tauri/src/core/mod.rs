//! Cross-cutting foundations shared by Host features and the headless CLI.

#[cfg(feature = "desktop")]
pub mod blocking;
pub mod error;
pub mod fs;
pub mod install_dirs;
pub mod log_util;
pub mod paths;

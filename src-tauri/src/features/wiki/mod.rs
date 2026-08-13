mod cache;
#[cfg(feature = "desktop")]
pub mod commands;
pub mod embed;
pub mod extract;
pub mod frontmatter;
#[cfg(feature = "desktop")]
mod heading_rename;
pub mod index;
pub mod models;
pub mod rename;
pub mod resolve;

pub use index::WikiIndexState;
pub use rename::ExternalRenameRepairStore;

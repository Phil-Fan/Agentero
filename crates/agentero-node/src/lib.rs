//! Headless Agentero node.
//!
//! A small, dependency-light daemon that turns any machine into a discoverable
//! peer of the Agentero fabric:
//!
//! * [`identity`] — an Ed25519 node identity, persisted under the config dir.
//! * [`probe`] — a snapshot of the host (os/arch/cpu/mem/gpu) for capability
//!   advertisement.
//! * [`card`] — an A2A-shaped Agent Card plus a client that fetches a peer's
//!   card over HTTP.
//! * [`registry`] — the machine-local list of peers discovered so far.
//! * [`server`] — a loopback HTTP server exposing the card + health.
//!
//! This crate deliberately does **not** depend on `agentero-core`: it is meant
//! to be copied to a bare server and built standalone, so the remote build does
//! not drag in the desktop dependency tree (rusqlite / PDFium / …).

pub mod acp;
pub mod auth;
pub mod card;
pub mod client;
pub mod error;
pub mod executor;
pub mod identity;
pub mod ops;
pub mod paths;
pub mod probe;
pub mod queue;
pub mod registry;
pub mod rpc;
pub mod server;
pub mod store;
pub mod task;

pub use error::{NodeError, Result};

/// Test-only helpers shared across modules.
#[cfg(test)]
pub(crate) mod test_support {
    use std::sync::Mutex;

    /// Serializes tests that mutate process-wide env (`XDG_CONFIG_HOME`).
    pub static ENV_LOCK: Mutex<()> = Mutex::new(());
}

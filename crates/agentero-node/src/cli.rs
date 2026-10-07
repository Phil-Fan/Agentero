//! Command-line surface for the node daemon.

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "agentero-node",
    version,
    about = "Headless Agentero node — identity, Agent Card, discovery, and server",
    long_about = "Turn this machine into a discoverable peer of the Agentero fabric.\n\
                  Mint an Ed25519 identity, advertise an A2A Agent Card, serve it on\n\
                  loopback, and remember other nodes you discover."
)]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Cmd,
}

#[derive(Debug, Subcommand)]
pub enum Cmd {
    /// Create this machine's node identity (Ed25519) if it does not exist.
    Init {
        /// Human-friendly node name (default: hostname).
        #[arg(long, value_name = "NAME")]
        name: Option<String>,
    },
    /// Print this node's A2A Agent Card as JSON.
    Card {
        /// A2A endpoint URL to advertise (default: http://127.0.0.1:8766/a2a).
        #[arg(long, value_name = "URL")]
        url: Option<String>,
    },
    /// Serve the Agent Card and /healthz endpoints until Ctrl-C.
    Serve {
        /// Bind address (loopback by default; never expose without a tunnel).
        #[arg(long, value_name = "ADDR", default_value = "127.0.0.1:8766")]
        listen: String,
        /// Override the A2A URL advertised in the card.
        #[arg(long, value_name = "URL")]
        url: Option<String>,
    },
    /// Fetch and print another node's Agent Card.
    Probe {
        /// Base URL (`http://host:8766`) or full card URL.
        target: String,
    },
    /// Probe a node and remember it in the local registry.
    Add {
        /// Base URL (`http://host:8766`) or full card URL.
        target: String,
        /// Override the display name stored in the registry.
        #[arg(long, value_name = "NAME")]
        name: Option<String>,
    },
    /// List nodes remembered in the local registry.
    List,
    /// Forget a remembered node by id (full or short).
    Remove { node_id: String },
}

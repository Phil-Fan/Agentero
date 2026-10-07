//! Command-line surface for the node daemon.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "agentero-node",
    version,
    about = "Headless Agentero node — identity, Agent Card, discovery, and server",
    long_about = "Turn this machine into a discoverable peer of the Agentero fabric.\n\
                  Mint an Ed25519 identity, advertise an A2A Agent Card, serve A2A tasks\n\
                  on loopback, and remember other nodes you discover."
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
    /// Serve the Agent Card, /healthz, and the A2A endpoint until Ctrl-C.
    Serve {
        /// Bind address (loopback by default; never expose without a tunnel).
        #[arg(long, value_name = "ADDR", default_value = "127.0.0.1:8766")]
        listen: String,
        /// Override the A2A URL advertised in the card.
        #[arg(long, value_name = "URL")]
        url: Option<String>,
        /// Enable the `run` skill (execute shell commands). Off by default.
        #[arg(long)]
        allow_run: bool,
        /// Working directory for `run` commands (default: current dir).
        #[arg(long, value_name = "DIR")]
        workdir: Option<PathBuf>,
        /// Require a paired bearer token for /a2a (prints a pairing code).
        #[arg(long)]
        require_auth: bool,
        /// Command template for the `agent` skill, e.g. "claude -p {prompt}".
        /// Enabling this advertises the `agent` skill.
        #[arg(long, value_name = "CMD")]
        agent_command: Option<String>,
        /// Maximum number of tasks executed concurrently (worker pool size).
        #[arg(long, default_value_t = 4)]
        max_parallel: usize,
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
    /// Exchange a pairing code for a token and remember it for this node.
    Pair {
        target: String,
        /// One-time pairing code printed by the node's `serve`.
        code: String,
        /// Client name recorded by the node.
        #[arg(long, value_name = "NAME")]
        name: Option<String>,
    },
    /// Send one A2A JSON-RPC request and print the response.
    Call {
        target: String,
        #[arg(long, value_name = "METHOD", default_value = "message/send")]
        method: String,
        /// JSON params object.
        #[arg(long, value_name = "JSON")]
        params: Option<String>,
    },
    /// Run a shell command on a node and stream its status to completion.
    Run {
        target: String,
        /// Shell command to execute.
        command: String,
        /// Skill to invoke (default: run).
        #[arg(long, value_name = "SKILL", default_value = "run")]
        skill: String,
        /// Working directory on the node.
        #[arg(long, value_name = "DIR")]
        cwd: Option<String>,
    },
    /// List recent tasks on a node.
    Tasks {
        target: String,
        /// Maximum rows to print.
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
}

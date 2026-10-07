//! `agentero-node` — headless Agentero node CLI.
//!
//! Turns a machine into a discoverable peer: mint an identity, print its Agent
//! Card, serve it on loopback, and remember other nodes.
//!
//! ```text
//! agentero-node init [--name N]     # mint this machine's Ed25519 identity
//! agentero-node card [--url URL]    # print this node's Agent Card
//! agentero-node serve [--listen A]  # serve card + /healthz on loopback
//! agentero-node probe <target>      # fetch another node's Agent Card
//! agentero-node add <target>        # probe + remember in the local registry
//! agentero-node list                # list remembered nodes
//! agentero-node remove <node-id>    # forget a node
//! ```

mod cli;

use agentero_node::{card, identity, paths, registry, server, NodeError, Result};
use clap::Parser;
use cli::{Cli, Cmd};
use serde_json::json;

#[tokio::main]
async fn main() -> std::process::ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    match run(Cli::parse()).await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<()> {
    match cli.cmd {
        Cmd::Init { name } => init(name),
        Cmd::Card { url } => {
            let value = card::build(url.as_deref())?;
            println!("{}", serde_json::to_string_pretty(&value)?);
            Ok(())
        }
        Cmd::Serve { listen, url } => {
            let addr: std::net::SocketAddr = listen
                .parse()
                .map_err(|e| NodeError::msg(format!("invalid --listen '{listen}': {e}")))?;
            server::serve(addr, url).await
        }
        Cmd::Probe { target } => {
            let value = card::fetch(&target).await?;
            println!("{}", serde_json::to_string_pretty(&value)?);
            Ok(())
        }
        Cmd::Add { target, name } => add(&target, name).await,
        Cmd::List => {
            let reg = registry::load()?;
            println!("{}", serde_json::to_string_pretty(&reg)?);
            Ok(())
        }
        Cmd::Remove { node_id } => {
            let mut reg = registry::load()?;
            let removed = reg.remove(&node_id);
            reg.save()?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({ "removed": removed, "nodeId": node_id }))?
            );
            Ok(())
        }
    }
}

fn init(name: Option<String>) -> Result<()> {
    let identity = match identity::load()? {
        Some(mut existing) => {
            if let Some(name) = name {
                let trimmed = name.trim();
                if !trimmed.is_empty() && trimmed != existing.name {
                    existing.name = trimmed.to_string();
                    existing.save()?;
                }
            }
            existing
        }
        None => identity::create(name)?,
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "nodeId": identity.node_id,
            "shortId": identity.short_id,
            "name": identity.name,
            "identityPath": paths::identity_path().display().to_string(),
        }))?
    );
    Ok(())
}

async fn add(target: &str, name: Option<String>) -> Result<()> {
    let card = card::fetch(target).await?;
    let mut reg = registry::load()?;
    let node = registry::RegisteredNode::from_card(target, &card, name);
    let node_id = node.node_id.clone();
    let node_name = node.name.clone();
    reg.upsert(node);
    reg.save()?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "added": node_id,
            "name": node_name,
            "registryPath": paths::registry_path().display().to_string(),
        }))?
    );
    Ok(())
}

//! `agentero-node` — headless Agentero node CLI.
//!
//! Turns a machine into a discoverable peer: mint an identity, print its Agent
//! Card, serve A2A tasks on loopback, remember other nodes, and call them.
//!
//! ```text
//! agentero-node init [--name N]         # mint this machine's Ed25519 identity
//! agentero-node card [--url URL]        # print this node's Agent Card
//! agentero-node serve [--allow-run]     # serve card + /healthz + /a2a
//! agentero-node probe <target>          # fetch another node's Agent Card
//! agentero-node add <target>            # probe + remember in the local registry
//! agentero-node list                    # list remembered nodes
//! agentero-node remove <node-id>        # forget a node
//! agentero-node call <target> [--method M --params JSON]
//! agentero-node run <target> <command>  # run a command and stream status
//! ```

mod cli;

use std::sync::Arc;
use std::time::Duration;

use agentero_node::client;
use agentero_node::executor::{
    AgentExecutor, EchoExecutor, Executor, ExecutorRegistry, RunExecutor,
};
use agentero_node::server::ServeOptions;
use agentero_node::{card, identity, paths, registry, server, NodeError, Result};
use clap::Parser;
use cli::{Cli, Cmd};
use serde_json::{json, Value};

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
            let value = card::build(url.as_deref(), Vec::new())?;
            println!("{}", serde_json::to_string_pretty(&value)?);
            Ok(())
        }
        Cmd::Serve {
            listen,
            url,
            allow_run,
            workdir,
            require_auth,
            agent_command,
            max_parallel,
        } => {
            let addr: std::net::SocketAddr = listen
                .parse()
                .map_err(|e| NodeError::msg(format!("invalid --listen '{listen}': {e}")))?;
            let mut executors: Vec<Arc<dyn Executor>> = vec![
                Arc::new(EchoExecutor),
                Arc::new(RunExecutor {
                    allowed: allow_run,
                    workdir: workdir.clone(),
                }),
            ];
            if let Some(template) = agent_command.filter(|s| !s.trim().is_empty()) {
                executors.push(Arc::new(AgentExecutor {
                    command_template: template,
                    workdir,
                }));
            }
            let registry = ExecutorRegistry::new(executors, "echo");
            server::serve(
                ServeOptions {
                    listen: addr,
                    card_url: url,
                    require_auth,
                    max_parallel,
                },
                registry,
            )
            .await
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
        Cmd::Pair { target, code, name } => pair(&target, &code, name).await,
        Cmd::Call {
            target,
            method,
            params,
        } => {
            let params = match params {
                Some(text) => serde_json::from_str::<Value>(&text)
                    .map_err(|e| NodeError::msg(format!("invalid --params JSON: {e}")))?,
                None => json!({}),
            };
            let response =
                client::call(&target, &method, params, token_for(&target).as_deref()).await?;
            println!("{}", serde_json::to_string_pretty(&response)?);
            Ok(())
        }
        Cmd::Run {
            target,
            command,
            skill,
            cwd,
        } => run_command(&target, &command, &skill, cwd).await,
        Cmd::Tasks { target, limit } => list_tasks(&target, limit).await,
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

/// Look up a stored bearer token for a target URL.
fn token_for(target: &str) -> Option<String> {
    let reg = registry::load().ok()?;
    reg.find_by_url(target).and_then(|node| node.token.clone())
}

/// Exchange a pairing code for a token and persist it for this node.
async fn pair(target: &str, code: &str, name: Option<String>) -> Result<()> {
    let client_name = name.unwrap_or_else(|| "agentero-node".to_string());
    let response = client::pair(target, code, &client_name).await?;
    let token = response
        .get("token")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if token.is_empty() {
        return Err(NodeError::msg("pairing response missing token"));
    }
    let mut reg = registry::load()?;
    let node = match reg.find_by_url(target).cloned() {
        Some(mut node) => {
            node.token = Some(token);
            node.last_seen = chrono::Utc::now().to_rfc3339();
            node
        }
        None => {
            let card = card::fetch(target).await.unwrap_or(Value::Null);
            let mut node =
                registry::RegisteredNode::from_card(target, &card, Some(client_name.clone()));
            node.token = Some(token);
            node
        }
    };
    let node_id = node.node_id.clone();
    reg.upsert(node);
    reg.save()?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "paired": node_id,
            "name": client_name,
            "registryPath": paths::registry_path().display().to_string(),
        }))?
    );
    Ok(())
}

/// Submit a `run` task and poll `tasks/get` until it reaches a terminal state.
async fn run_command(target: &str, command: &str, skill: &str, cwd: Option<String>) -> Result<()> {
    let token = token_for(target);
    let mut data = serde_json::Map::new();
    data.insert("command".to_string(), json!(command));
    if let Some(cwd) = cwd {
        data.insert("cwd".to_string(), json!(cwd));
    }
    let params = json!({
        "message": {
            "role": "user",
            "parts": [
                { "kind": "text", "text": command },
                { "kind": "data", "data": data }
            ]
        },
        "metadata": { "skill": skill }
    });

    let response = client::call(target, "message/send", params, token.as_deref()).await?;
    let task_id = response
        .pointer("/result/id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if task_id.is_empty() {
        println!("{}", serde_json::to_string_pretty(&response)?);
        return Ok(());
    }
    println!("task {task_id}");

    let mut last = String::new();
    loop {
        let got = client::call(
            target,
            "tasks/get",
            json!({ "id": task_id }),
            token.as_deref(),
        )
        .await?;
        let task = got.get("result").cloned().unwrap_or(Value::Null);
        let state = task
            .pointer("/status/state")
            .and_then(Value::as_str)
            .unwrap_or("?")
            .to_string();
        let note = task
            .pointer("/status/message/parts/0/text")
            .and_then(Value::as_str)
            .map(str::to_string);
        match note {
            Some(text) if text != last => {
                println!("[{state}] {text}");
                last = text;
            }
            None if state != last => {
                println!("[{state}]");
                last = state.clone();
            }
            _ => {}
        }
        if matches!(
            state.as_str(),
            "completed" | "failed" | "canceled" | "rejected"
        ) {
            if let Some(artifacts) = task.get("artifacts").and_then(Value::as_array) {
                for artifact in artifacts {
                    let name = artifact
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("artifact");
                    if let Some(text) = artifact.pointer("/parts/0/text").and_then(Value::as_str) {
                        println!("--- {name} ---\n{text}");
                    }
                }
            }
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(400)).await;
    }
}

/// Fetch and print recent task summaries from a node.
async fn list_tasks(target: &str, limit: usize) -> Result<()> {
    let token = token_for(target);
    let got = client::call(target, "tasks/list", json!({}), token.as_deref()).await?;
    let items = got
        .pointer("/result/tasks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let rows: Vec<Value> = items.into_iter().take(limit).collect();
    println!("{}", serde_json::to_string_pretty(&rows)?);
    Ok(())
}

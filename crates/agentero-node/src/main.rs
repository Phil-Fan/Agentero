//! `agentero-node` — headless Agentero node CLI.
//!
//! Turns a machine into a discoverable peer: mint an identity, print its Agent
//! Card, serve A2A tasks on loopback, remember other nodes, and call them.
//! Client-side operations are shared with `agentero-cli` via [`agentero_node::ops`].
//!
//! ```text
//! agentero-node init | card | serve | probe | add | list | remove | pair | call | run | tasks | batch
//! ```

mod cli;

use std::sync::Arc;

use agentero_node::acp::AcpExecutor;
use agentero_node::executor::{
    AgentExecutor, EchoExecutor, Executor, ExecutorRegistry, RunExecutor,
};
use agentero_node::server::ServeOptions;
use agentero_node::{card, identity, ops, paths, server, NodeError, Result};
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
            print_json(&serde_json::to_value(value)?)
        }
        Cmd::Serve {
            listen,
            url,
            allow_run,
            workdir,
            require_auth,
            agent_command,
            acp_command,
            acp_approve,
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
            if let Some(command) = acp_command.filter(|s| !s.trim().is_empty()) {
                let mut parts = command.split_whitespace();
                let program = parts.next().unwrap_or_default().to_string();
                let args = parts.map(str::to_string).collect();
                executors.push(Arc::new(AcpExecutor {
                    program,
                    args,
                    workdir: workdir.clone(),
                    auto_approve: acp_approve,
                }));
            } else if let Some(template) = agent_command.filter(|s| !s.trim().is_empty()) {
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
        Cmd::Probe { target } => print_json(&ops::probe(&target).await?),
        Cmd::Add { target, name } => print_json(&ops::add(&target, name).await?),
        Cmd::List => print_json(&ops::list()?),
        Cmd::Remove { node_id } => print_json(&ops::remove(&node_id)?),
        Cmd::Pair { target, code, name } => print_json(&ops::pair(&target, &code, name).await?),
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
            print_json(&ops::call(&target, &method, params).await?)
        }
        Cmd::Run {
            target,
            command,
            skill,
            cwd,
        } => {
            ops::run(&target, &command, &skill, cwd).await?;
            Ok(())
        }
        Cmd::Tasks {
            target,
            limit,
            context,
        } => {
            let items = ops::tasks(&target, context).await?;
            let rows: Vec<Value> = items
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .take(limit)
                .collect();
            print_json(&Value::Array(rows))
        }
        Cmd::Batch {
            target,
            skill,
            template,
            values,
            values_file,
            context,
            cwd,
            wait,
        } => {
            ops::batch(
                &target,
                &skill,
                &template,
                values,
                values_file,
                context,
                cwd,
                wait,
            )
            .await?;
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
    print_json(&json!({
        "nodeId": identity.node_id,
        "shortId": identity.short_id,
        "name": identity.name,
        "identityPath": paths::identity_path().display().to_string(),
    }))
}

fn print_json(value: &Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

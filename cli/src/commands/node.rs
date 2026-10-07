//! `agentero node *` — discover, pair with, and call remote Agentero nodes.
//!
//! Thin wrapper over [`agentero_node::ops`], shared with the `agentero-node`
//! binary. `run` / `batch` stream text progress on stdout.

use crate::error::CliError;
use crate::resolve::GlobalOpts;
use agentero_node::ops;
use clap::Subcommand;
use serde_json::{json, Value};

#[derive(Debug, Subcommand)]
pub enum NodeCmd {
    /// Fetch and print another node's Agent Card.
    Probe { target: String },
    /// Probe a node and remember it in the local registry.
    Add {
        target: String,
        #[arg(long, value_name = "NAME")]
        name: Option<String>,
    },
    /// List nodes remembered in the local registry.
    List,
    /// Forget a remembered node by id.
    Remove { node_id: String },
    /// Exchange a pairing code for a token and remember it.
    Pair {
        target: String,
        code: String,
        #[arg(long, value_name = "NAME")]
        name: Option<String>,
    },
    /// Send one A2A JSON-RPC request and print the response.
    Call {
        target: String,
        #[arg(long, value_name = "METHOD", default_value = "message/send")]
        method: String,
        #[arg(long, value_name = "JSON")]
        params: Option<String>,
    },
    /// List recent tasks on a node.
    Tasks {
        target: String,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long, value_name = "ID")]
        context: Option<String>,
    },
    /// Run a shell command on a node, streaming status to completion.
    Run {
        target: String,
        command: String,
        #[arg(long, value_name = "SKILL", default_value = "run")]
        skill: String,
        #[arg(long, value_name = "DIR")]
        cwd: Option<String>,
    },
    /// Submit a parameter sweep (one task per value, shared context).
    Batch {
        target: String,
        #[arg(long, default_value = "run")]
        skill: String,
        #[arg(long, value_name = "TEMPLATE")]
        template: String,
        #[arg(long, value_name = "LIST")]
        values: Option<String>,
        #[arg(long, value_name = "FILE")]
        values_file: Option<std::path::PathBuf>,
        #[arg(long, value_name = "ID")]
        context: Option<String>,
        #[arg(long, value_name = "DIR")]
        cwd: Option<String>,
        #[arg(long)]
        wait: bool,
    },
}

pub async fn run(cmd: NodeCmd, _globals: &GlobalOpts) -> Result<Value, CliError> {
    match cmd {
        NodeCmd::Probe { target } => Ok(ops::probe(&target).await?),
        NodeCmd::Add { target, name } => Ok(ops::add(&target, name).await?),
        NodeCmd::List => Ok(ops::list()?),
        NodeCmd::Remove { node_id } => Ok(ops::remove(&node_id)?),
        NodeCmd::Pair { target, code, name } => Ok(ops::pair(&target, &code, name).await?),
        NodeCmd::Call {
            target,
            method,
            params,
        } => {
            let params = match params {
                Some(text) => serde_json::from_str::<Value>(&text)
                    .map_err(|e| CliError::usage(format!("invalid --params JSON: {e}")))?,
                None => json!({}),
            };
            Ok(ops::call(&target, &method, params).await?)
        }
        NodeCmd::Tasks {
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
            Ok(Value::Array(rows))
        }
        NodeCmd::Run {
            target,
            command,
            skill,
            cwd,
        } => {
            ops::run(&target, &command, &skill, cwd).await?;
            Ok(Value::Null)
        }
        NodeCmd::Batch {
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
            Ok(Value::Null)
        }
    }
}

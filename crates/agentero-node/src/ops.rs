//! Client-side node operations shared by the `agentero-node` binary and
//! `agentero-cli`. Each returns a JSON value for the caller to render.
//!
//! `run` and `batch` stream progress to stdout as tasks execute.

use std::path::PathBuf;
use std::time::Duration;

use serde_json::{json, Value};

use crate::{card, client, paths, registry, NodeError, Result};

/// Look up a stored bearer token for a target URL.
pub fn token_for(target: &str) -> Option<String> {
    let reg = registry::load().ok()?;
    reg.find_by_url(target).and_then(|node| node.token.clone())
}

/// Fetch and return another node's Agent Card.
pub async fn probe(target: &str) -> Result<Value> {
    card::fetch(target).await
}

/// Probe a node and remember it in the local registry.
pub async fn add(target: &str, name: Option<String>) -> Result<Value> {
    let card = card::fetch(target).await?;
    let mut reg = registry::load()?;
    let node = registry::RegisteredNode::from_card(target, &card, name);
    let node_id = node.node_id.clone();
    let node_name = node.name.clone();
    reg.upsert(node);
    reg.save()?;
    Ok(json!({
        "added": node_id,
        "name": node_name,
        "registryPath": paths::registry_path().display().to_string(),
    }))
}

/// List remembered nodes.
pub fn list() -> Result<Value> {
    let reg = registry::load()?;
    Ok(serde_json::to_value(&reg)?)
}

/// Forget a remembered node by id (full or short).
pub fn remove(node_id: &str) -> Result<Value> {
    let mut reg = registry::load()?;
    let removed = reg.remove(node_id);
    reg.save()?;
    Ok(json!({ "removed": removed, "nodeId": node_id }))
}

/// Exchange a pairing code for a token and persist it for this node.
pub async fn pair(target: &str, code: &str, name: Option<String>) -> Result<Value> {
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
    Ok(json!({
        "paired": node_id,
        "name": client_name,
        "registryPath": paths::registry_path().display().to_string(),
    }))
}

/// Send one raw A2A JSON-RPC request (attaching the stored token).
pub async fn call(target: &str, method: &str, params: Value) -> Result<Value> {
    client::call(target, method, params, token_for(target).as_deref()).await
}

/// List task summaries (optionally filtered by context). Returns the array.
pub async fn tasks(target: &str, context: Option<String>) -> Result<Value> {
    let params = match context {
        Some(context) => json!({ "contextId": context }),
        None => json!({}),
    };
    let response = client::call(target, "tasks/list", params, token_for(target).as_deref()).await?;
    Ok(response
        .pointer("/result/tasks")
        .cloned()
        .unwrap_or_else(|| json!([])))
}

/// Submit a `run` task and poll to completion, printing status transitions.
pub async fn run(target: &str, command: &str, skill: &str, cwd: Option<String>) -> Result<Value> {
    let token = token_for(target);
    let mut data = serde_json::Map::new();
    data.insert("command".to_string(), json!(command));
    data.insert("prompt".to_string(), json!(command));
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
    let task_id = task_id_of(&response);
    if task_id.is_empty() {
        return Ok(response);
    }
    println!("task {task_id}");

    let mut last = String::new();
    loop {
        let task = client::call(
            target,
            "tasks/get",
            json!({ "id": task_id }),
            token.as_deref(),
        )
        .await?;
        let task = task.get("result").cloned().unwrap_or(Value::Null);
        let state = state_of(&task);
        let note = note_of(&task);
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
        if is_terminal(&state) {
            print_artifacts(&task);
            return Ok(task);
        }
        tokio::time::sleep(Duration::from_millis(400)).await;
    }
}

/// Submit a parameter sweep sharing a context id, optionally waiting.
#[allow(clippy::too_many_arguments)]
pub async fn batch(
    target: &str,
    skill: &str,
    template: &str,
    values: Option<String>,
    values_file: Option<PathBuf>,
    context: Option<String>,
    cwd: Option<String>,
    wait: bool,
) -> Result<Value> {
    let values = collect_values(values, values_file)?;
    if values.is_empty() {
        return Err(NodeError::msg(
            "no values provided (use --values or --values-file)",
        ));
    }
    let token = token_for(target);
    let context_id = context.unwrap_or_else(|| format!("sweep-{}", crate::task::new_id()));

    let mut ids = Vec::new();
    for value in &values {
        let rendered = template.replace("{value}", value);
        let mut data = serde_json::Map::new();
        data.insert("command".to_string(), json!(rendered));
        data.insert("prompt".to_string(), json!(rendered));
        if let Some(cwd) = &cwd {
            data.insert("cwd".to_string(), json!(cwd));
        }
        let params = json!({
            "contextId": context_id,
            "message": {
                "role": "user",
                "parts": [
                    { "kind": "text", "text": rendered },
                    { "kind": "data", "data": data }
                ]
            },
            "metadata": { "skill": skill }
        });
        let response = client::call(target, "message/send", params, token.as_deref()).await?;
        ids.push(task_id_of(&response));
    }

    println!("context {context_id}: submitted {} tasks", ids.len());
    for (value, id) in values.iter().zip(ids.iter()) {
        println!("  {value} -> {}", short_id(id));
    }
    if wait {
        poll_all(target, token.as_deref(), &ids).await?;
    }
    Ok(json!({ "contextId": context_id, "taskIds": ids }))
}

async fn poll_all(target: &str, token: Option<&str>, ids: &[String]) -> Result<()> {
    loop {
        let mut done = true;
        let mut states = Vec::new();
        for id in ids {
            let got = client::call(target, "tasks/get", json!({ "id": id }), token).await?;
            let state = got
                .pointer("/result/status/state")
                .and_then(Value::as_str)
                .unwrap_or("?")
                .to_string();
            if !is_terminal(&state) {
                done = false;
            }
            states.push((short_id(id), state));
        }
        if done {
            for (id, state) in states {
                println!("  {id} {state}");
            }
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

fn collect_values(values: Option<String>, values_file: Option<PathBuf>) -> Result<Vec<String>> {
    let mut out = Vec::new();
    if let Some(list) = values {
        out.extend(
            list.split(',')
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_string),
        );
    }
    if let Some(path) = values_file {
        let text = std::fs::read_to_string(&path)?;
        out.extend(
            text.lines()
                .map(str::trim)
                .filter(|v| !v.is_empty() && !v.starts_with('#'))
                .map(str::to_string),
        );
    }
    Ok(out)
}

fn task_id_of(response: &Value) -> String {
    response
        .pointer("/result/id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn state_of(task: &Value) -> String {
    task.pointer("/status/state")
        .and_then(Value::as_str)
        .unwrap_or("?")
        .to_string()
}

fn note_of(task: &Value) -> Option<String> {
    task.pointer("/status/message/parts/0/text")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn is_terminal(state: &str) -> bool {
    matches!(state, "completed" | "failed" | "canceled" | "rejected")
}

fn print_artifacts(task: &Value) {
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
}

fn short_id(id: &str) -> String {
    id.chars().take(8).collect()
}

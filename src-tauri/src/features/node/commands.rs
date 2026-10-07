//! Tauri commands backing the「远端会话」panel.
//!
//! Thin wrappers over [`agentero_node::ops`]. Each command returns a JSON string
//! so the IPC contract stays a simple `String` (the frontend parses it); this
//! keeps the typed bindings small and stable.

use agentero_node::{ops, NodeError};

fn json(value: serde_json::Value) -> Result<String, String> {
    serde_json::to_string(&value).map_err(|e| e.to_string())
}

fn err(error: NodeError) -> String {
    error.to_string()
}

/// List nodes in the local registry.
#[tauri::command]
#[specta::specta]
pub async fn node_list() -> Result<String, String> {
    json(ops::list().map_err(err)?)
}

/// Fetch a node's Agent Card.
#[tauri::command]
#[specta::specta]
pub async fn node_probe(target: String) -> Result<String, String> {
    json(ops::probe(&target).await.map_err(err)?)
}

/// Probe a node and remember it.
#[tauri::command]
#[specta::specta]
pub async fn node_add(target: String, name: Option<String>) -> Result<String, String> {
    json(ops::add(&target, name).await.map_err(err)?)
}

/// Forget a remembered node.
#[tauri::command]
#[specta::specta]
pub async fn node_remove(node_id: String) -> Result<String, String> {
    json(ops::remove(&node_id).map_err(err)?)
}

/// Exchange a pairing code for a token.
#[tauri::command]
#[specta::specta]
pub async fn node_pair(
    target: String,
    code: String,
    name: Option<String>,
) -> Result<String, String> {
    json(ops::pair(&target, &code, name).await.map_err(err)?)
}

/// List task summaries (optionally filtered by context).
#[tauri::command]
#[specta::specta]
pub async fn node_tasks(target: String, context: Option<String>) -> Result<String, String> {
    json(ops::tasks(&target, context).await.map_err(err)?)
}

/// Fetch one full task (with artifacts).
#[tauri::command]
#[specta::specta]
pub async fn node_task(target: String, id: String) -> Result<String, String> {
    json(ops::task(&target, &id).await.map_err(err)?)
}

/// Submit one task (no polling).
#[tauri::command]
#[specta::specta]
pub async fn node_submit(
    target: String,
    input: String,
    skill: String,
    cwd: Option<String>,
) -> Result<String, String> {
    json(
        ops::submit(&target, &input, &skill, cwd)
            .await
            .map_err(err)?,
    )
}

/// Cancel a task.
#[tauri::command]
#[specta::specta]
pub async fn node_cancel(target: String, id: String) -> Result<String, String> {
    json(ops::cancel(&target, &id).await.map_err(err)?)
}

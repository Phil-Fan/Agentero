//! JSON-RPC 2.0 dispatch for the A2A methods.
//!
//! Supported: `message/send`, `message/stream`, `tasks/get`, `tasks/cancel`
//! (plus the `v1` PascalCase aliases). Streaming is returned to the server as a
//! [`RpcOutcome::Stream`] so it can be framed as SSE.

use std::sync::Arc;

use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::broadcast;

use crate::auth::Security;
use crate::card::AgentCard;
use crate::executor::{Executor, ExecutorRegistry, TaskContext};
use crate::store::{CreatedTask, TaskStore};
use crate::task::{new_id, Message, StreamEvent, Task};

pub struct ServerState {
    pub store: Arc<TaskStore>,
    pub executors: ExecutorRegistry,
    pub card: AgentCard,
    pub security: Arc<Security>,
}

#[derive(Debug, Deserialize)]
pub struct RpcRequest {
    #[serde(default)]
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Option<Value>,
}

/// A task stream to be framed as SSE.
pub struct StreamPayload {
    pub id: Value,
    pub task: Task,
    pub receiver: broadcast::Receiver<StreamEvent>,
}

/// Either a complete JSON-RPC response, or a task stream to be framed as SSE.
pub enum RpcOutcome {
    Json(Value),
    Stream(Box<StreamPayload>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Method {
    Send,
    Stream,
    Get,
    Cancel,
    List,
    Unknown,
}

fn normalize(method: &str) -> Method {
    match method {
        "message/send" | "SendMessage" | "sendMessage" => Method::Send,
        "message/stream" | "SendStreamingMessage" | "sendStreamingMessage" => Method::Stream,
        "tasks/get" | "GetTask" | "getTask" => Method::Get,
        "tasks/cancel" | "CancelTask" | "cancelTask" => Method::Cancel,
        "tasks/list" | "ListTasks" | "listTasks" => Method::List,
        _ => Method::Unknown,
    }
}

pub async fn dispatch(state: &Arc<ServerState>, request: RpcRequest) -> RpcOutcome {
    let id = request.id.clone().unwrap_or(Value::Null);
    match normalize(&request.method) {
        Method::Send => match start_task(state, request.params.as_ref()) {
            Ok((created, executor, message)) => {
                spawn_task(state, &created, executor, message);
                RpcOutcome::Json(ok(
                    id,
                    serde_json::to_value(created.task).unwrap_or(Value::Null),
                ))
            }
            Err(error) => RpcOutcome::Json(fail(id, error)),
        },
        Method::Stream => match start_task(state, request.params.as_ref()) {
            Ok((created, executor, message)) => {
                let task = created.task.clone();
                spawn_task(state, &created, executor, message);
                RpcOutcome::Stream(Box::new(StreamPayload {
                    id,
                    task,
                    receiver: created.receiver,
                }))
            }
            Err(error) => RpcOutcome::Json(fail(id, error)),
        },
        Method::Get => RpcOutcome::Json(match task_id_param(request.params.as_ref()) {
            Some(task_id) => match state.store.get(&task_id) {
                Some(task) => ok(id, serde_json::to_value(task).unwrap_or(Value::Null)),
                None => error(id, -32001, format!("task '{task_id}' not found")),
            },
            None => error(id, -32602, "missing task id"),
        }),
        Method::Cancel => RpcOutcome::Json(match task_id_param(request.params.as_ref()) {
            Some(task_id) => match state.store.cancel(&task_id) {
                Some(task) => ok(id, serde_json::to_value(task).unwrap_or(Value::Null)),
                None => error(id, -32001, format!("task '{task_id}' not found")),
            },
            None => error(id, -32602, "missing task id"),
        }),
        Method::List => {
            let tasks = state.store.list();
            RpcOutcome::Json(ok(id, serde_json::json!({ "tasks": tasks })))
        }
        Method::Unknown => RpcOutcome::Json(error(
            id,
            -32601,
            format!("method '{}' not found", request.method),
        )),
    }
}

/// Validate params, resolve the executor, and create the task.
fn start_task(
    state: &Arc<ServerState>,
    params: Option<&Value>,
) -> Result<(CreatedTask, Arc<dyn Executor>, Message), String> {
    let params = params.ok_or("missing params")?;
    let message_value = params.get("message").ok_or("missing 'message'")?;
    let message: Message = serde_json::from_value(message_value.clone())
        .map_err(|e| format!("invalid message: {e}"))?;
    let skill = skill_of(params, &message);
    let executor = state
        .executors
        .resolve(skill.as_deref())
        .map_err(|e| e.to_string())?;
    let context_id = params
        .get("contextId")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| message.context_id.clone())
        .unwrap_or_else(new_id);
    let created = state.store.create(context_id, message.clone());
    Ok((created, executor, message))
}

fn spawn_task(
    state: &Arc<ServerState>,
    created: &CreatedTask,
    executor: Arc<dyn Executor>,
    message: Message,
) {
    let store = state.store.clone();
    let task_id = created.task.id.clone();
    let context_id = created.task.context_id.clone();
    let cancel = created.cancel.clone();
    tokio::spawn(async move {
        let ctx = TaskContext {
            store: store.clone(),
            task_id: task_id.clone(),
            context_id,
            message,
            cancel,
        };
        match executor.execute(ctx).await {
            Ok(()) => store.finish_ok(&task_id),
            Err(error) => store.finish_err(&task_id, error.to_string()),
        }
    });
}

fn skill_of(params: &Value, message: &Message) -> Option<String> {
    params
        .get("metadata")
        .and_then(|m| m.get("skill"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            message
                .data()
                .and_then(|d| d.get("skill"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
}

fn task_id_param(params: Option<&Value>) -> Option<String> {
    let params = params?;
    params
        .get("id")
        .or_else(|| params.get("taskId"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

pub fn ok(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

pub fn error(id: Value, code: i64, message: impl Into<String>) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message.into() }
    })
}

fn fail(id: Value, message: impl Into<String>) -> Value {
    error(id, -32602, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_both_bindings() {
        assert_eq!(normalize("message/send"), Method::Send);
        assert_eq!(normalize("SendMessage"), Method::Send);
        assert_eq!(normalize("message/stream"), Method::Stream);
        assert_eq!(normalize("SendStreamingMessage"), Method::Stream);
        assert_eq!(normalize("tasks/get"), Method::Get);
        assert_eq!(normalize("CancelTask"), Method::Cancel);
        assert_eq!(normalize("tasks/list"), Method::List);
        assert_eq!(normalize("nope"), Method::Unknown);
    }

    #[test]
    fn extracts_task_id_from_both_keys() {
        assert_eq!(
            task_id_param(Some(&json!({ "id": "t1" }))).as_deref(),
            Some("t1")
        );
        assert_eq!(
            task_id_param(Some(&json!({ "taskId": "t2" }))).as_deref(),
            Some("t2")
        );
        assert_eq!(task_id_param(Some(&json!({}))), None);
    }
}

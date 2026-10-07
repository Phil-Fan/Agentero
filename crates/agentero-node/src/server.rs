//! Loopback HTTP server: Agent Card, health, pairing, and the A2A endpoint.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::State;
use axum::http::{header::AUTHORIZATION, HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::stream::{self, StreamExt as _};
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::broadcast;

use crate::auth::Security;
use crate::card::{self, AgentCard, AgentSkill};
use crate::executor::{ExecutorRegistry, TaskContext};
use crate::queue::{Job, JobQueue};
use crate::rpc::{self, RpcOutcome, RpcRequest, ServerState};
use crate::store::TaskStore;
use crate::task::{Message, StreamEvent, Task, TaskState};
use crate::Result;

pub struct ServeOptions {
    pub listen: SocketAddr,
    pub card_url: Option<String>,
    pub require_auth: bool,
    /// Maximum number of tasks executed concurrently (worker pool size).
    pub max_parallel: usize,
}

/// Serve the node endpoints until Ctrl-C.
pub async fn serve(options: ServeOptions, executors: ExecutorRegistry) -> Result<()> {
    let skills = executors
        .ids()
        .into_iter()
        .map(|id| {
            let description = executors
                .get(&id)
                .map(|e| e.description().to_string())
                .unwrap_or_default();
            AgentSkill::new(&id, &description)
        })
        .collect();

    let advertised = options
        .card_url
        .unwrap_or_else(|| format!("http://{}/a2a", options.listen));
    let card = card::build(Some(advertised.as_str()), skills)?;
    let security = Arc::new(Security::load(options.require_auth)?);
    let store = match TaskStore::persisted(crate::paths::tasks_dir()) {
        Ok(store) => store,
        Err(error) => {
            log::warn!(target: "agentero_node", "task persistence disabled: {error}");
            TaskStore::new()
        }
    };
    let state = Arc::new(ServerState {
        store,
        executors,
        queue: JobQueue::new(),
        card,
        security,
    });

    // Worker pool + re-enqueue tasks that were queued before a restart.
    let worker_count = options.max_parallel.max(1);
    for _ in 0..worker_count {
        tokio::spawn(worker(state.clone()));
    }
    requeue(&state);

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route(card::CARD_PATH, get(agent_card))
        .route("/pair", post(pair))
        .route("/a2a", post(a2a))
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind(options.listen)
        .await
        .map_err(|e| crate::NodeError::msg(format!("cannot bind {}: {e}", options.listen)))?;

    log::info!(target: "agentero_node", "listening on http://{}", options.listen);
    println!("agentero-node listening on http://{}", options.listen);
    if options.require_auth {
        if let Some(code) = state.security.ensure_pairing_code() {
            println!("pairing code: {code}");
            println!(
                "  pair a client with: agentero-node pair http://<host>:{} {code}",
                options.listen.port()
            );
        }
    }

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|e| crate::NodeError::msg(format!("node server: {e}")))?;
    Ok(())
}

async fn healthz(State(state): State<Arc<ServerState>>) -> Json<Value> {
    Json(serde_json::json!({
        "status": "ok",
        "nodeId": state.card.agentero.node_id,
        "name": state.card.name,
        "version": state.card.version,
        "requireAuth": state.security.require_auth(),
        "pairedClients": state.security.token_count(),
        "queued": state.queue.depth(),
    }))
}

async fn agent_card(State(state): State<Arc<ServerState>>) -> Json<AgentCard> {
    Json(state.card.clone())
}

#[derive(Deserialize)]
struct PairRequest {
    code: String,
    #[serde(default)]
    name: Option<String>,
}

async fn pair(State(state): State<Arc<ServerState>>, Json(request): Json<PairRequest>) -> Response {
    let name = request.name.unwrap_or_default();
    match state.security.pair(&request.code, &name) {
        Ok(token) => Json(serde_json::json!({
            "token": token,
            "nodeId": state.card.agentero.node_id,
        }))
        .into_response(),
        Err(error) => (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": error.to_string() })),
        )
            .into_response(),
    }
}

async fn a2a(State(state): State<Arc<ServerState>>, headers: HeaderMap, body: String) -> Response {
    let bearer = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok());
    if !state.security.verify(bearer) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "unauthorized" })),
        )
            .into_response();
    }

    let request: RpcRequest = match serde_json::from_str(&body) {
        Ok(request) => request,
        Err(error) => {
            return Json(rpc::error(
                Value::Null,
                -32700,
                format!("parse error: {error}"),
            ))
            .into_response()
        }
    };
    match rpc::dispatch(&state, request).await {
        RpcOutcome::Json(value) => Json(value).into_response(),
        RpcOutcome::Stream(payload) => {
            let rpc::StreamPayload { id, task, receiver } = *payload;
            stream_response(id, task, receiver)
        }
    }
}

/// Frame a task's event stream as SSE: an initial `task` snapshot, then
/// `status-update` / `artifact-update` events until the final status.
fn stream_response(id: Value, task: Task, receiver: broadcast::Receiver<StreamEvent>) -> Response {
    let initial = {
        let id = id.clone();
        stream::once(
            async move { Ok::<Event, Infallible>(sse_event(&StreamEvent::Task(task), &id)) },
        )
    };

    // Consume the broadcast channel until the terminal status, then end the
    // stream so the SSE response closes.
    let updates = {
        let id = id.clone();
        stream::unfold((receiver, false), move |(mut rx, done)| {
            let id = id.clone();
            async move {
                if done {
                    return None;
                }
                loop {
                    match rx.recv().await {
                        Ok(event) => {
                            let terminal = matches!(
                                &event,
                                StreamEvent::StatusUpdate { is_final, .. } if *is_final
                            );
                            let item = Ok::<Event, Infallible>(sse_event(&event, &id));
                            return Some((item, (rx, terminal)));
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(broadcast::error::RecvError::Closed) => return None,
                    }
                }
            }
        })
    };

    Sse::new(initial.chain(updates))
        .keep_alive(KeepAlive::default())
        .into_response()
}

fn sse_event(event: &StreamEvent, id: &Value) -> Event {
    let payload = serde_json::json!({ "jsonrpc": "2.0", "id": id, "result": event });
    Event::default().data(payload.to_string())
}

/// A worker pulls one job at a time and executes it. Cancelled-while-queued jobs
/// are skipped (the store already marked them terminal).
async fn worker(state: Arc<ServerState>) {
    loop {
        let job = state.queue.pop().await;
        if job.cancel.is_cancelled() {
            continue;
        }
        let task_id = job.task_id.clone();
        let ctx = TaskContext {
            store: state.store.clone(),
            task_id: task_id.clone(),
            context_id: job.context_id,
            message: job.message,
            cancel: job.cancel,
        };
        match job.executor.execute(ctx).await {
            Ok(()) => state.store.finish_ok(&task_id),
            Err(error) => state.store.finish_err(&task_id, error.to_string()),
        }
    }
}

/// Re-enqueue `Submitted` tasks loaded from disk, re-resolving their executor
/// from the recorded skill.
fn requeue(state: &Arc<ServerState>) {
    for id in state.store.task_ids_in_state(TaskState::Submitted) {
        let Some(task) = state.store.get(&id) else {
            continue;
        };
        let executor = match state.executors.resolve(task.skill().as_deref()) {
            Ok(executor) => executor,
            Err(error) => {
                state
                    .store
                    .finish_err(&id, format!("cannot resume after restart: {error}"));
                continue;
            }
        };
        let message = task
            .history
            .first()
            .cloned()
            .unwrap_or_else(|| Message::user(""));
        let cancel = state.store.ensure_cancel(&id);
        state.queue.push(Job {
            task_id: id,
            context_id: task.context_id.clone(),
            message,
            executor,
            cancel,
        });
    }
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    log::info!(target: "agentero_node", "shutdown signal received");
}

//! Loopback HTTP server exposing the Agent Card and a health probe.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{extract::State, routing::get, Json, Router};

use crate::card::{self, AgentCard};
use crate::Result;

/// Serve the node endpoints until Ctrl-C.
///
/// `card_url` overrides the A2A endpoint advertised in the card (useful when the
/// node is reached through an SSH tunnel / reverse proxy at a different URL).
pub async fn serve(listen: SocketAddr, card_url: Option<String>) -> Result<()> {
    let advertised = card_url.unwrap_or_else(|| format!("http://{listen}/a2a"));
    let card = Arc::new(card::build(Some(&advertised))?);

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route(card::CARD_PATH, get(agent_card))
        .with_state(card);

    let listener = tokio::net::TcpListener::bind(listen)
        .await
        .map_err(|e| crate::NodeError::msg(format!("cannot bind {listen}: {e}")))?;

    log::info!(target: "agentero_node", "listening on http://{listen}");
    println!("agentero-node listening on http://{listen}");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|e| crate::NodeError::msg(format!("node server: {e}")))?;
    Ok(())
}

async fn healthz(State(card): State<Arc<AgentCard>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "nodeId": card.agentero.node_id,
        "name": card.name,
        "version": card.version,
    }))
}

async fn agent_card(State(card): State<Arc<AgentCard>>) -> Json<AgentCard> {
    Json((*card).clone())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    log::info!(target: "agentero_node", "shutdown signal received");
}

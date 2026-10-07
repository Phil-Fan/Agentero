//! A2A-shaped Agent Card + a client to fetch a peer's card.
//!
//! The card carries the standard A2A discovery fields plus an `agentero`
//! extension block with the node id / public key / host probe. We stay on the
//! A2A `v0.3` card shape (`url` at the top level) for broad client
//! compatibility; the extension is additive and ignorable.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::probe::HostCapabilities;
use crate::{identity, probe, Result};

/// Default loopback endpoint for the node server.
pub const DEFAULT_PORT: u16 = 8766;
pub const DEFAULT_A2A_URL: &str = "http://127.0.0.1:8766/a2a";
pub const CARD_PATH: &str = "/.well-known/agent-card.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCard {
    pub protocol_version: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub url: String,
    pub preferred_transport: String,
    pub capabilities: CardCapabilities,
    pub default_input_modes: Vec<String>,
    pub default_output_modes: Vec<String>,
    pub skills: Vec<AgentSkill>,
    pub agentero: AgenteroExtension,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CardCapabilities {
    pub streaming: bool,
    pub push_notifications: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSkill {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgenteroExtension {
    pub node_id: String,
    pub short_id: String,
    pub public_key: String,
    pub host: HostCapabilities,
}

/// Build this machine's card. `url` defaults to the loopback A2A endpoint.
pub fn build(url: Option<&str>) -> Result<AgentCard> {
    let identity = identity::load_or_create()?;
    let host = probe::probe();
    Ok(AgentCard {
        protocol_version: "0.3.0".to_string(),
        name: format!("agentero-node-{}", identity.short_id),
        description: "Agentero headless node".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        url: url.unwrap_or(DEFAULT_A2A_URL).to_string(),
        preferred_transport: "JSONRPC".to_string(),
        capabilities: CardCapabilities {
            streaming: true,
            push_notifications: false,
        },
        default_input_modes: vec!["text".to_string()],
        default_output_modes: vec!["text".to_string()],
        skills: Vec::new(),
        agentero: AgenteroExtension {
            node_id: identity.node_id,
            short_id: identity.short_id,
            public_key: identity.public_key,
            host,
        },
    })
}

/// Fetch a peer's Agent Card.
///
/// `target` may be a base URL (`http://host:8766`) or a full card URL
/// (`http://host:8766/.well-known/agent-card.json`).
pub async fn fetch(target: &str) -> Result<Value> {
    let trimmed = target.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(crate::NodeError::msg("empty node target"));
    }
    let url = if trimmed.ends_with(".json") {
        trimmed.to_string()
    } else {
        format!("{trimmed}{CARD_PATH}")
    };
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| crate::NodeError::Http(e.to_string()))?;
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| crate::NodeError::Http(format!("{url}: {e}")))?;
    if !response.status().is_success() {
        return Err(crate::NodeError::Http(format!(
            "{url}: HTTP {}",
            response.status()
        )));
    }
    let value = response
        .json::<Value>()
        .await
        .map_err(|e| crate::NodeError::Http(format!("{url}: {e}")))?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_roundtrips_through_json() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("XDG_CONFIG_HOME", dir.path());
        let card = build(None).unwrap();
        assert_eq!(card.url, DEFAULT_A2A_URL);
        assert_eq!(card.protocol_version, "0.3.0");
        assert_eq!(card.agentero.node_id.len(), 64);
        let text = serde_json::to_string(&card).unwrap();
        let back: AgentCard = serde_json::from_str(&text).unwrap();
        assert_eq!(back.agentero.node_id, card.agentero.node_id);
        std::env::remove_var("XDG_CONFIG_HOME");
    }
}

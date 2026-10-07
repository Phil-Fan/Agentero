//! Machine-local registry of discovered peers.
//!
//! Stored as JSON at [`crate::paths::registry_path`]. The raw card is retained
//! so the registry is forward-compatible with card fields we do not model yet.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::paths;
use crate::Result;

pub const REGISTRY_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeRegistry {
    pub version: u32,
    pub nodes: Vec<RegisteredNode>,
}

impl Default for NodeRegistry {
    fn default() -> Self {
        Self {
            version: REGISTRY_VERSION,
            nodes: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisteredNode {
    pub node_id: String,
    pub short_id: String,
    pub name: String,
    /// `url` | `ssh` | `local`.
    pub transport: String,
    pub url: Option<String>,
    pub ssh_host: Option<String>,
    pub public_key: Option<String>,
    pub last_seen: String,
    /// Long-lived bearer token (paired). Sensitive; registry file is `0600`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// Raw card as fetched, kept verbatim for forward compatibility.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card: Option<Value>,
}

impl RegisteredNode {
    /// Build a registry entry from a fetched card.
    pub fn from_card(target: &str, card: &Value, name_override: Option<String>) -> Self {
        let ext = card.get("agentero");
        let pick = |key: &str| {
            ext.and_then(|e| e.get(key))
                .and_then(Value::as_str)
                .map(str::to_string)
        };
        let public_key = pick("publicKey");
        let mut node_id = pick("nodeId").unwrap_or_default();
        if node_id.is_empty() {
            // Foreign card without the extension: derive a stable id from the
            // public key if present, else hash the target URL.
            node_id = match &public_key {
                Some(pk) => crate::identity::fingerprint(pk),
                None => short_hash(target),
            };
        }
        let short_id = node_id.chars().take(12).collect();
        let name = name_override
            .map(|n| n.trim().to_string())
            .filter(|n| !n.is_empty())
            .or_else(|| card.get("name").and_then(Value::as_str).map(str::to_string))
            .unwrap_or_else(|| node_id.clone());
        Self {
            node_id,
            short_id,
            name,
            transport: "url".to_string(),
            url: Some(target.to_string()),
            ssh_host: None,
            public_key,
            last_seen: chrono::Utc::now().to_rfc3339(),
            token: None,
            card: Some(card.clone()),
        }
    }
}

fn short_hash(input: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    hex::encode(hasher.finalize())
}

/// Load the registry, returning an empty one when the file is absent.
pub fn load() -> Result<NodeRegistry> {
    let path = paths::registry_path();
    if !path.is_file() {
        return Ok(NodeRegistry::default());
    }
    let text = std::fs::read_to_string(&path)?;
    let registry: NodeRegistry = serde_json::from_str(&text)?;
    Ok(registry)
}

impl NodeRegistry {
    pub fn save(&self) -> Result<()> {
        let path = paths::registry_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, text)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }

    /// Find a remembered node by its base URL (trailing slash insensitive).
    pub fn find_by_url(&self, target: &str) -> Option<&RegisteredNode> {
        let normalized = target.trim().trim_end_matches('/');
        self.nodes.iter().find(|node| {
            node.url
                .as_deref()
                .map(|url| url.trim_end_matches('/') == normalized)
                .unwrap_or(false)
        })
    }

    /// Insert or replace a node by id.
    pub fn upsert(&mut self, node: RegisteredNode) {
        match self.nodes.iter_mut().find(|n| n.node_id == node.node_id) {
            Some(existing) => *existing = node,
            None => self.nodes.push(node),
        }
    }

    /// Remove a node by id (or short id). Returns whether something was removed.
    pub fn remove(&mut self, id_or_short: &str) -> bool {
        let before = self.nodes.len();
        self.nodes
            .retain(|n| n.node_id != id_or_short && n.short_id != id_or_short);
        self.nodes.len() != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn from_card_extracts_extension() {
        let card = json!({
            "name": "agentero-node-abc",
            "agentero": { "nodeId": "deadbeef", "publicKey": "00".repeat(32) }
        });
        let node = RegisteredNode::from_card("http://host:8766", &card, None);
        assert_eq!(node.node_id, "deadbeef");
        assert_eq!(node.name, "agentero-node-abc");
        assert_eq!(node.transport, "url");
        assert!(node.card.is_some());
    }

    #[test]
    fn upsert_and_remove_roundtrip() {
        let mut reg = NodeRegistry::default();
        let card = json!({ "agentero": { "nodeId": "n1" } });
        reg.upsert(RegisteredNode::from_card(
            "http://a",
            &card,
            Some("A".into()),
        ));
        reg.upsert(RegisteredNode::from_card(
            "http://a2",
            &card,
            Some("A2".into()),
        ));
        assert_eq!(reg.nodes.len(), 1);
        assert_eq!(reg.nodes[0].name, "A2");
        assert!(reg.remove("n1"));
        assert!(!reg.remove("n1"));
    }
}

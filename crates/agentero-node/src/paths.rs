//! Node-local data directories (XDG layout).
//!
//! Mirrors `agentero-core::paths` for the two paths the node daemon owns so the
//! daemon stays buildable without the core crate. Keep the layout in sync:
//!
//! | Path | Contents |
//! |------|----------|
//! | `$XDG_CONFIG_HOME/agentero/node/identity.json` | Ed25519 node identity |
//! | `$XDG_CONFIG_HOME/agentero/nodes.json` | discovered-peer registry |

use std::path::PathBuf;

fn env_dir(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

fn config_home() -> PathBuf {
    if let Some(p) = env_dir("XDG_CONFIG_HOME") {
        return p;
    }
    #[cfg(any(windows, target_os = "ios"))]
    {
        dirs::config_dir().unwrap_or_else(|| PathBuf::from("."))
    }
    #[cfg(not(any(windows, target_os = "ios")))]
    {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".config")
    }
}

/// `$XDG_CONFIG_HOME/agentero` (created on demand by callers).
pub fn config_dir() -> PathBuf {
    config_home().join("agentero")
}

/// `…/agentero/node` — this machine's node identity lives here.
pub fn node_dir() -> PathBuf {
    config_dir().join("node")
}

/// `…/agentero/node/identity.json`.
pub fn identity_path() -> PathBuf {
    node_dir().join("identity.json")
}

/// `…/agentero/nodes.json` — locally remembered peers.
pub fn registry_path() -> PathBuf {
    config_dir().join("nodes.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_and_node_dirs_are_nested() {
        assert_eq!(
            config_dir().file_name().and_then(|s| s.to_str()),
            Some("agentero")
        );
        assert_eq!(
            node_dir().file_name().and_then(|s| s.to_str()),
            Some("node")
        );
        assert_eq!(node_dir().parent(), Some(config_dir().as_path()));
    }

    #[test]
    fn identity_and_registry_paths() {
        assert_eq!(
            identity_path().file_name().and_then(|s| s.to_str()),
            Some("identity.json")
        );
        assert_eq!(identity_path().parent(), Some(node_dir().as_path()));
        assert_eq!(
            registry_path().file_name().and_then(|s| s.to_str()),
            Some("nodes.json")
        );
    }
}

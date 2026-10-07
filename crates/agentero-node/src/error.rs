//! Error type for the node crate.
//!
//! Kept separate from `agentero-core::error::AppError` so the node daemon can be
//! built standalone on a bare server.

/// Result alias used across the node crate.
pub type Result<T> = std::result::Result<T, NodeError>;

#[derive(Debug, thiserror::Error)]
pub enum NodeError {
    #[error("{0}")]
    Message(String),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("json: {0}")]
    Json(#[from] serde_json::Error),

    #[error("http: {0}")]
    Http(String),
}

impl NodeError {
    pub fn msg(message: impl Into<String>) -> Self {
        Self::Message(message.into())
    }
}

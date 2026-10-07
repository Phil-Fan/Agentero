//! A2A task model: messages, tasks, artifacts, and stream events.
//!
//! Shapes follow the Agent2Agent protocol (`v0.3` naming) closely enough to be
//! interoperable while staying small. The JSON is the contract — see
//! `docs/development/remote-node.md`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Random 32-hex id (task / message / context / artifact).
pub fn new_id() -> String {
    let mut bytes = [0u8; 16];
    let _ = getrandom::getrandom(&mut bytes);
    hex::encode(bytes)
}

fn default_role() -> String {
    "user".to_string()
}

/// One unit of communication.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    #[serde(default = "new_id")]
    pub message_id: String,
    #[serde(default = "default_role")]
    pub role: String,
    #[serde(default)]
    pub parts: Vec<Part>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
}

impl Message {
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            message_id: new_id(),
            role: "user".to_string(),
            parts: vec![Part::Text { text: text.into() }],
            context_id: None,
            task_id: None,
        }
    }

    pub fn agent(text: impl Into<String>) -> Self {
        Self {
            message_id: new_id(),
            role: "agent".to_string(),
            parts: vec![Part::Text { text: text.into() }],
            context_id: None,
            task_id: None,
        }
    }

    /// Concatenated text of every `text` part.
    pub fn text_content(&self) -> String {
        self.parts
            .iter()
            .filter_map(|p| match p {
                Part::Text { text } => Some(text.as_str()),
                Part::Data { .. } => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// First `data` part, if any.
    pub fn data(&self) -> Option<&Value> {
        self.parts.iter().find_map(|p| match p {
            Part::Data { data } => Some(data),
            Part::Text { .. } => None,
        })
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Part {
    Text { text: String },
    Data { data: Value },
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TaskState {
    Submitted,
    Working,
    InputRequired,
    Completed,
    Failed,
    Canceled,
    Rejected,
}

impl TaskState {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Canceled | Self::Rejected
        )
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TaskStatus {
    pub state: TaskState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<Message>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}

impl TaskStatus {
    pub fn new(state: TaskState) -> Self {
        Self {
            state,
            message: None,
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
        }
    }

    pub fn with_message(state: TaskState, message: Message) -> Self {
        Self {
            state,
            message: Some(message),
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    pub artifact_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub parts: Vec<Part>,
}

impl Artifact {
    pub fn text(name: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            artifact_id: new_id(),
            name: Some(name.into()),
            parts: vec![Part::Text { text: text.into() }],
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub context_id: String,
    pub status: TaskStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<Message>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<Artifact>,
}

/// Compact task view for `tasks/list`.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TaskSummary {
    pub id: String,
    pub context_id: String,
    pub state: TaskState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}

impl TaskSummary {
    pub fn from_task(task: &Task) -> Self {
        Self {
            id: task.id.clone(),
            context_id: task.context_id.clone(),
            state: task.status.state,
            timestamp: task.status.timestamp.clone(),
        }
    }
}

/// Events delivered on a task stream (`message/stream`, SSE).
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum StreamEvent {
    Task(Task),
    StatusUpdate {
        task_id: String,
        context_id: String,
        status: TaskStatus,
        #[serde(rename = "final")]
        is_final: bool,
    },
    ArtifactUpdate {
        task_id: String,
        context_id: String,
        artifact: Artifact,
        append: bool,
        last_chunk: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_part_roundtrips_with_kind_tag() {
        let msg = Message::user("hello");
        let value = serde_json::to_value(&msg).unwrap();
        assert_eq!(value["role"], "user");
        assert_eq!(value["parts"][0]["kind"], "text");
        assert_eq!(value["parts"][0]["text"], "hello");
        let back: Message = serde_json::from_value(value).unwrap();
        assert_eq!(back.text_content(), "hello");
    }

    #[test]
    fn stream_event_status_uses_final_key() {
        let event = StreamEvent::StatusUpdate {
            task_id: "t".into(),
            context_id: "c".into(),
            status: TaskStatus::new(TaskState::Working),
            is_final: false,
        };
        let value = serde_json::to_value(&event).unwrap();
        assert_eq!(value["kind"], "status-update");
        assert_eq!(value["final"], false);
        assert_eq!(value["taskId"], "t");
        assert_eq!(value["contextId"], "c");
    }

    #[test]
    fn terminal_states() {
        assert!(TaskState::Completed.is_terminal());
        assert!(!TaskState::Working.is_terminal());
    }
}

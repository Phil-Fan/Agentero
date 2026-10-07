//! In-memory task store with per-task broadcast for streaming.
//!
//! A store owns every task's state, its stream broadcaster, and its cancellation
//! token. Executors mutate tasks through the store so `tasks/get` and
//! `message/stream` always observe a consistent view.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::task::{new_id, Artifact, Message, StreamEvent, Task, TaskState, TaskStatus};

const CHANNEL_CAPACITY: usize = 512;

pub struct TaskStore {
    tasks: RwLock<HashMap<String, Task>>,
    senders: RwLock<HashMap<String, broadcast::Sender<StreamEvent>>>,
    cancels: RwLock<HashMap<String, CancellationToken>>,
}

/// Everything the caller needs to drive and observe a freshly created task.
pub struct CreatedTask {
    pub task: Task,
    pub receiver: broadcast::Receiver<StreamEvent>,
    pub cancel: CancellationToken,
}

impl TaskStore {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            tasks: RwLock::new(HashMap::new()),
            senders: RwLock::new(HashMap::new()),
            cancels: RwLock::new(HashMap::new()),
        })
    }

    /// Create a task from an incoming user message.
    pub fn create(&self, context_id: String, user: Message) -> CreatedTask {
        let id = new_id();
        let user = Message {
            task_id: Some(id.clone()),
            context_id: Some(context_id.clone()),
            ..user
        };
        let task = Task {
            id: id.clone(),
            context_id,
            status: TaskStatus::new(TaskState::Submitted),
            history: vec![user],
            artifacts: Vec::new(),
        };
        let (tx, rx) = broadcast::channel(CHANNEL_CAPACITY);
        self.tasks.write().unwrap().insert(id.clone(), task.clone());
        self.senders.write().unwrap().insert(id.clone(), tx);
        let cancel = CancellationToken::new();
        self.cancels
            .write()
            .unwrap()
            .insert(id.clone(), cancel.clone());
        CreatedTask {
            task,
            receiver: rx,
            cancel,
        }
    }

    pub fn get(&self, id: &str) -> Option<Task> {
        self.tasks.read().unwrap().get(id).cloned()
    }

    pub fn subscribe(&self, id: &str) -> Option<broadcast::Receiver<StreamEvent>> {
        self.senders.read().unwrap().get(id).map(|s| s.subscribe())
    }

    pub fn is_cancelled(&self, id: &str) -> bool {
        self.cancels
            .read()
            .unwrap()
            .get(id)
            .map(|t| t.is_cancelled())
            .unwrap_or(false)
    }

    /// Request cancellation. Idempotent; returns the current snapshot.
    pub fn cancel(&self, id: &str) -> Option<Task> {
        {
            let tasks = self.tasks.read().unwrap();
            let task = tasks.get(id)?;
            if task.status.state.is_terminal() {
                return Some(task.clone());
            }
        }
        if let Some(token) = self.cancels.read().unwrap().get(id) {
            token.cancel();
        }
        self.set_status(
            id,
            TaskStatus::with_message(TaskState::Canceled, Message::agent("canceled by client")),
            true,
        )
    }

    /// Set the task status (no-op once terminal). Returns the snapshot.
    pub fn set_status(&self, id: &str, status: TaskStatus, is_final: bool) -> Option<Task> {
        let (snapshot, context_id) = {
            let mut tasks = self.tasks.write().unwrap();
            let task = tasks.get_mut(id)?;
            if task.status.state.is_terminal() {
                return Some(task.clone());
            }
            task.status = status.clone();
            (task.clone(), task.context_id.clone())
        };
        self.broadcast(StreamEvent::StatusUpdate {
            task_id: id.to_string(),
            context_id,
            status,
            is_final,
        });
        Some(snapshot)
    }

    /// Append a completed artifact and broadcast it.
    pub fn add_artifact(&self, id: &str, artifact: Artifact) -> Option<Task> {
        let (snapshot, context_id) = {
            let mut tasks = self.tasks.write().unwrap();
            let task = tasks.get_mut(id)?;
            task.artifacts.push(artifact.clone());
            (task.clone(), task.context_id.clone())
        };
        self.broadcast(StreamEvent::ArtifactUpdate {
            task_id: id.to_string(),
            context_id,
            artifact,
            append: false,
            last_chunk: true,
        });
        Some(snapshot)
    }

    /// Mark completed unless already terminal.
    pub fn finish_ok(&self, id: &str) {
        self.set_status(id, TaskStatus::new(TaskState::Completed), true);
    }

    /// Mark failed unless already terminal.
    pub fn finish_err(&self, id: &str, message: impl Into<String>) {
        self.set_status(
            id,
            TaskStatus::with_message(TaskState::Failed, Message::agent(message)),
            true,
        );
    }

    fn broadcast(&self, event: StreamEvent) {
        let task_id = match &event {
            StreamEvent::Task(task) => task.id.clone(),
            StreamEvent::StatusUpdate { task_id, .. } => task_id.clone(),
            StreamEvent::ArtifactUpdate { task_id, .. } => task_id.clone(),
        };
        if let Some(sender) = self.senders.read().unwrap().get(&task_id) {
            let _ = sender.send(event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_get_and_finish() {
        let store = TaskStore::new();
        let created = store.create(new_id(), Message::user("hi"));
        let id = created.task.id.clone();
        assert_eq!(store.get(&id).unwrap().status.state, TaskState::Submitted);
        store.finish_ok(&id);
        assert_eq!(store.get(&id).unwrap().status.state, TaskState::Completed);
        // Terminal state is sticky.
        store.set_status(&id, TaskStatus::new(TaskState::Working), false);
        assert_eq!(store.get(&id).unwrap().status.state, TaskState::Completed);
    }

    #[test]
    fn cancel_sets_terminal_and_token() {
        let store = TaskStore::new();
        let created = store.create(new_id(), Message::user("hi"));
        let id = created.task.id.clone();
        let canceled = store.cancel(&id).unwrap();
        assert_eq!(canceled.status.state, TaskState::Canceled);
        assert!(store.is_cancelled(&id));
    }
}

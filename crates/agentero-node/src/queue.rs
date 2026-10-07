//! In-process job queue feeding a worker pool.
//!
//! `message/send` / `message/stream` enqueue a [`Job`] and return immediately;
//! a pool of `--max-parallel` workers pulls jobs and executes them. A queued
//! (never-started) task can be cancelled before it runs, and survives a restart
//! (the server re-enqueues `Submitted` tasks on startup).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use crate::executor::Executor;
use crate::task::Message;

/// One queued task awaiting a worker.
pub struct Job {
    pub task_id: String,
    pub context_id: String,
    pub message: Message,
    pub executor: Arc<dyn Executor>,
    pub cancel: CancellationToken,
}

pub struct JobQueue {
    pending: Mutex<VecDeque<Job>>,
    notify: Notify,
}

impl JobQueue {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            pending: Mutex::new(VecDeque::new()),
            notify: Notify::new(),
        })
    }

    /// Append a job and wake one waiting worker.
    pub fn push(&self, job: Job) {
        self.pending.lock().unwrap().push_back(job);
        self.notify.notify_one();
    }

    /// Wait for and take the next job.
    pub async fn pop(&self) -> Job {
        loop {
            // Register interest before checking so a concurrent `push` is not
            // lost between the check and the await.
            let notified = self.notify.notified();
            if let Some(job) = self.pending.lock().unwrap().pop_front() {
                return job;
            }
            notified.await;
        }
    }

    /// Number of jobs waiting to be executed.
    pub fn depth(&self) -> usize {
        self.pending.lock().unwrap().len()
    }
}

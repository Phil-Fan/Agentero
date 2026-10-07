//! Task executors.
//!
//! An [`Executor`] performs the work for one task and reports progress through
//! the [`TaskStore`]. Two are provided: `echo` (protocol smoke test) and `run`
//! (execute a shell command in a workdir, streaming output).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

use crate::store::TaskStore;
use crate::task::{Artifact, Message, TaskState, TaskStatus};
use crate::{NodeError, Result};

/// Upper bound on captured output (bytes) kept for the artifact.
const MAX_OUTPUT_BYTES: usize = 1024 * 1024;
/// Upper bound on per-line status updates emitted to the stream.
const MAX_STREAM_LINES: usize = 200;

pub struct TaskContext {
    pub store: Arc<TaskStore>,
    pub task_id: String,
    pub context_id: String,
    pub message: Message,
    pub cancel: tokio_util::sync::CancellationToken,
}

impl TaskContext {
    pub fn text(&self) -> String {
        self.message.text_content()
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    pub fn working(&self, note: impl Into<String>) {
        self.store.set_status(
            &self.task_id,
            TaskStatus::with_message(TaskState::Working, Message::agent(note)),
            false,
        );
    }
}

#[async_trait]
pub trait Executor: Send + Sync {
    fn id(&self) -> &'static str;
    fn description(&self) -> &'static str {
        ""
    }
    async fn execute(&self, ctx: TaskContext) -> Result<()>;
}

/// Echoes the user's message back as an artifact. Trivial protocol check.
pub struct EchoExecutor;

#[async_trait]
impl Executor for EchoExecutor {
    fn id(&self) -> &'static str {
        "echo"
    }

    fn description(&self) -> &'static str {
        "Echo the request back (protocol smoke test)"
    }

    async fn execute(&self, ctx: TaskContext) -> Result<()> {
        let text = ctx.text();
        ctx.working(format!("echo: {text}"));
        ctx.store
            .add_artifact(&ctx.task_id, Artifact::text("echo.txt", text));
        Ok(())
    }
}

/// Runs a shell command in a working directory, streaming output.
///
/// Command source: a `data` part `{ "command": "...", "cwd": "..." }`, falling
/// back to the message text. Disabled unless the node was started with
/// `--allow-run`.
pub struct RunExecutor {
    pub allowed: bool,
    pub workdir: Option<PathBuf>,
}

#[async_trait]
impl Executor for RunExecutor {
    fn id(&self) -> &'static str {
        "run"
    }

    fn description(&self) -> &'static str {
        "Run a shell command in the node workspace and stream its output"
    }

    async fn execute(&self, ctx: TaskContext) -> Result<()> {
        if !self.allowed {
            return Err(NodeError::msg(
                "run skill is disabled; start the node with --allow-run",
            ));
        }
        let (command, cwd) = resolve_command(&ctx, self.workdir.as_deref());
        if command.trim().is_empty() {
            return Err(NodeError::msg("empty command"));
        }
        ctx.working(format!("$ {command}"));

        let mut child = spawn(&command, cwd.as_deref())?;
        let (_handles, mut rx) = pipe_lines(&mut child);

        let mut output = String::new();
        let mut emitted = 0usize;
        loop {
            tokio::select! {
                maybe = rx.recv() => {
                    let Some(line) = maybe else { break };
                    if output.len() < MAX_OUTPUT_BYTES {
                        output.push_str(&line);
                        output.push('\n');
                    }
                    if emitted < MAX_STREAM_LINES {
                        emitted += 1;
                        ctx.working(line);
                    }
                }
                _ = ctx.cancel.cancelled() => {
                    let _ = child.kill().await;
                    return Ok(());
                }
            }
        }
        let truncated = output.len() >= MAX_OUTPUT_BYTES;

        // Readers are done; wait for the process exit status.
        let status = child.wait().await?;
        if ctx.is_cancelled() {
            return Ok(());
        }
        let code = status.code();
        let summary = if truncated {
            format!("{output}\n… (output truncated)\n[exit {code:?}]")
        } else {
            format!("{output}[exit {code:?}]")
        };
        ctx.store
            .add_artifact(&ctx.task_id, Artifact::text("output.txt", summary));
        if status.success() {
            Ok(())
        } else {
            Err(NodeError::msg(format!(
                "command failed (exit {code:?})",
                code = code
            )))
        }
    }
}

fn spawn(command: &str, cwd: Option<&Path>) -> Result<Child> {
    let mut cmd = if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.arg("/C").arg(command);
        c
    } else {
        let mut c = Command::new("sh");
        c.arg("-c").arg(command);
        c
    };
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| NodeError::msg(format!("spawn failed: {e}")))
}

/// Drain stdout + stderr into one line channel.
fn pipe_lines(child: &mut Child) -> (Vec<tokio::task::JoinHandle<()>>, mpsc::Receiver<String>) {
    let (tx, rx) = mpsc::channel::<String>(256);
    let mut handles = Vec::new();
    if let Some(out) = child.stdout.take() {
        handles.push(spawn_reader(out, tx.clone()));
    }
    if let Some(err) = child.stderr.take() {
        handles.push(spawn_reader(err, tx.clone()));
    }
    drop(tx);
    (handles, rx)
}

fn spawn_reader<R: AsyncRead + Unpin + Send + 'static>(
    reader: R,
    tx: mpsc::Sender<String>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if tx.send(line).await.is_err() {
                break;
            }
        }
    })
}

fn resolve_command(ctx: &TaskContext, default_cwd: Option<&Path>) -> (String, Option<PathBuf>) {
    if let Some(data) = ctx.message.data() {
        let command = data
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let cwd = data
            .get("cwd")
            .and_then(|v| v.as_str())
            .map(PathBuf::from)
            .or_else(|| default_cwd.map(Path::to_path_buf));
        if !command.is_empty() {
            return (command, cwd);
        }
    }
    (ctx.text(), default_cwd.map(Path::to_path_buf))
}

/// Skill → executor routing, plus the default skill used when a request does
/// not name one.
pub struct ExecutorRegistry {
    executors: HashMap<String, Arc<dyn Executor>>,
    default_skill: String,
}

impl ExecutorRegistry {
    pub fn new(executors: Vec<Arc<dyn Executor>>, default_skill: &str) -> Self {
        let map = executors
            .into_iter()
            .map(|e| (e.id().to_string(), e))
            .collect::<HashMap<_, _>>();
        Self {
            executors: map,
            default_skill: default_skill.to_string(),
        }
    }

    pub fn resolve(&self, skill: Option<&str>) -> Result<Arc<dyn Executor>> {
        let key = skill.unwrap_or(self.default_skill.as_str());
        self.executors
            .get(key)
            .cloned()
            .ok_or_else(|| NodeError::msg(format!("unknown skill '{key}'")))
    }

    pub fn ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.executors.keys().cloned().collect();
        ids.sort();
        ids
    }

    pub fn default_skill(&self) -> &str {
        &self.default_skill
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn Executor>> {
        self.executors.get(id).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_resolves_default_and_named() {
        let echo: Arc<dyn Executor> = Arc::new(EchoExecutor);
        let run: Arc<dyn Executor> = Arc::new(RunExecutor {
            allowed: false,
            workdir: None,
        });
        let reg = ExecutorRegistry::new(vec![echo, run], "echo");
        assert_eq!(reg.resolve(None).unwrap().id(), "echo");
        assert_eq!(reg.resolve(Some("run")).unwrap().id(), "run");
        assert!(reg.resolve(Some("nope")).is_err());
        assert_eq!(reg.ids(), vec!["echo".to_string(), "run".to_string()]);
    }
}

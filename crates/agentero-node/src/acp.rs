//! ACP-native executor.
//!
//! Drives a CLI agent over the [Agent Client Protocol]: spawn the agent process,
//! `initialize` → `session/new` → `session/prompt`, and stream `session/update`
//! notifications into the task. Permission requests from the agent are denied by
//! default (`--acp-approve` opts into auto-approving the first AllowOnce option).
//!
//! The final agent message is collected into an `agent.txt` artifact.
//!
//! [Agent Client Protocol]: https://agentclientprotocol.com/

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use agent_client_protocol::schema::v1::{
    ContentBlock, InitializeRequest, McpServer, McpServerStdio, NewSessionRequest,
    PermissionOptionKind, PromptRequest, RequestPermissionOutcome, RequestPermissionRequest,
    RequestPermissionResponse, SelectedPermissionOutcome, SessionNotification, SessionUpdate,
    TextContent,
};
use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::{AcpAgent, Agent, Client, ConnectionTo};
use async_trait::async_trait;

use crate::executor::{prompt_of, Executor, TaskContext};
use crate::task::{Artifact, Message, TaskState, TaskStatus};
use crate::{NodeError, Result};

/// Runs a CLI agent through ACP. `program`/`args` is the agent command, e.g.
/// `codex-acp` or `node /path/to/claude-agent-acp.js`.
pub struct AcpExecutor {
    pub program: String,
    pub args: Vec<String>,
    pub workdir: Option<PathBuf>,
    pub auto_approve: bool,
}

#[async_trait]
impl Executor for AcpExecutor {
    fn id(&self) -> &'static str {
        "agent"
    }

    fn description(&self) -> &'static str {
        "Drive an ACP CLI agent (session/prompt) and stream its reply"
    }

    async fn execute(&self, ctx: TaskContext) -> Result<()> {
        let prompt = prompt_of(&ctx);
        if prompt.trim().is_empty() {
            return Err(NodeError::msg("empty prompt"));
        }
        let cwd = self
            .workdir
            .clone()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

        let stdio = McpServerStdio::new("agentero-node".to_string(), PathBuf::from(&self.program))
            .args(self.args.clone());
        let acp = AcpAgent::new(McpServer::Stdio(stdio));

        let store = ctx.store.clone();
        let task_id = ctx.task_id.clone();
        let auto_approve = self.auto_approve;
        let buffer: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));

        let connect = Client
            .builder()
            .name("agentero-node")
            .on_receive_notification(
                {
                    let store = store.clone();
                    let task_id = task_id.clone();
                    let buffer = buffer.clone();
                    async move |notification: SessionNotification, _cx| {
                        if let Some(text) = message_text(&notification.update) {
                            if let Ok(mut collected) = buffer.lock() {
                                collected.push_str(&text);
                            }
                            store.set_status(
                                &task_id,
                                TaskStatus::with_message(TaskState::Working, Message::agent(text)),
                                false,
                            );
                        }
                        Ok(())
                    }
                },
                agent_client_protocol::on_receive_notification!(),
            )
            .on_receive_request(
                async move |request: RequestPermissionRequest, responder, _cx| {
                    let _ = responder.respond(permission_response(&request, auto_approve));
                    Ok(())
                },
                agent_client_protocol::on_receive_request!(),
            )
            .connect_with(acp, {
                let cwd = cwd.clone();
                let prompt = prompt.clone();
                move |connection: ConnectionTo<Agent>| async move {
                    let _init = connection
                        .send_request(InitializeRequest::new(ProtocolVersion::V1))
                        .block_task()
                        .await?;
                    let session = connection
                        .send_request(NewSessionRequest::new(cwd))
                        .block_task()
                        .await?;
                    let blocks = vec![ContentBlock::Text(TextContent::new(prompt))];
                    let _response = connection
                        .send_request(PromptRequest::new(session.session_id, blocks))
                        .block_task()
                        .await?;
                    Ok(())
                }
            });

        match connect.await {
            Ok(()) => {
                let text = buffer.lock().map(|b| b.clone()).unwrap_or_default();
                store.add_artifact(&task_id, Artifact::text("agent.txt", text));
                Ok(())
            }
            Err(error) => Err(NodeError::msg(format!("acp: {error}"))),
        }
    }
}

/// Agent message text from a session update (ignoring thoughts / tool calls).
fn message_text(update: &SessionUpdate) -> Option<String> {
    match update {
        SessionUpdate::AgentMessageChunk(chunk) => text_of(&chunk.content),
        _ => None,
    }
}

fn text_of(block: &ContentBlock) -> Option<String> {
    match block {
        ContentBlock::Text(text) => Some(text.text.clone()),
        _ => None,
    }
}

/// Deny by default; when `auto_approve`, pick the first AllowOnce option.
fn permission_response(
    request: &RequestPermissionRequest,
    auto_approve: bool,
) -> RequestPermissionResponse {
    let outcome = if auto_approve {
        request
            .options
            .iter()
            .find(|option| option.kind == PermissionOptionKind::AllowOnce)
            .map_or(RequestPermissionOutcome::Cancelled, |opt| {
                RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
                    opt.option_id.clone(),
                ))
            })
    } else {
        RequestPermissionOutcome::Cancelled
    };
    RequestPermissionResponse::new(outcome)
}

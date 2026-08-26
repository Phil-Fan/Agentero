//! MCP tools + ServerHandler.

use super::notes::{self, WriteMode};
use super::paper;
use super::resources::{self, VAULT_NAME, VAULT_URI};
use super::McpController;
use crate::core::error::AppError;
use crate::features::catalog::{self, papers};
use crate::features::import::{self, LookupImportArgs, NoteShellMode};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, ContentBlock, ListResourcesResult, PaginatedRequestParams,
    ReadResourceRequestParams, ReadResourceResponse, ReadResourceResult, Resource,
    ResourceContents, ServerCapabilities, ServerInfo,
};
use rmcp::service::{RequestContext, RoleServer};
use rmcp::{tool, tool_handler, tool_router, ErrorData as McpError, ServerHandler};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use tauri::Manager;

#[derive(Clone)]
pub struct AgenteroMcp {
    pub ctrl: Arc<McpController>,
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

impl AgenteroMcp {
    pub fn new(ctrl: Arc<McpController>) -> Self {
        Self {
            ctrl,
            tool_router: Self::tool_router(),
        }
    }
}

fn tool_ok_json(value: &impl serde::Serialize) -> CallToolResult {
    let text = serde_json::to_string(value).unwrap_or_else(|_| "{}".into());
    CallToolResult::success(vec![ContentBlock::text(text)])
}

fn tool_err(err: AppError) -> CallToolResult {
    let body = json!({
        "code": err.code(),
        "message": err.to_string(),
    });
    CallToolResult::error(vec![ContentBlock::text(body.to_string())])
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
struct PaperListArgs {
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    tag: Vec<String>,
    #[serde(default)]
    unread: bool,
    #[serde(default)]
    limit: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
struct PaperRefArgs {
    /// Paper id or vault-relative folder path (`papers/…`).
    r#ref: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
struct ImportIdArgs {
    /// arXiv id, DOI, or URL.
    text: String,
    /// Vault-relative parent under `papers/` (default: current Library scope).
    #[serde(default)]
    parent: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
struct NotesWriteArgs {
    r#ref: String,
    content: String,
    /// `replace` (default) or `append`.
    #[serde(default)]
    mode: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
struct TagArgs {
    r#ref: String,
    tags: Vec<String>,
}

fn clamp_limit(raw: Option<u32>) -> usize {
    raw.unwrap_or(50).clamp(1, 200) as usize
}

#[tool_router]
impl AgenteroMcp {
    #[tool(
        description = "List papers in the open vault with catalog metadata (id, path, title, authors, year, tags, doi, arxivId, publication, status, isRead). Abstract is omitted; use paper_get for the full record."
    )]
    async fn paper_list(&self, Parameters(args): Parameters<PaperListArgs>) -> CallToolResult {
        let vault = match self.ctrl.local_vault() {
            Ok(v) => v,
            Err(e) => return tool_err(e),
        };
        match paper::list_papers(
            &vault,
            args.query.as_deref(),
            &args.tag,
            args.unread,
            clamp_limit(args.limit),
        ) {
            Ok(items) => tool_ok_json(&items),
            Err(e) => tool_err(e),
        }
    }

    #[tool(description = "Get one paper's full catalog metadata by id or vault-relative path.")]
    async fn paper_get(&self, Parameters(args): Parameters<PaperRefArgs>) -> CallToolResult {
        let vault = match self.ctrl.local_vault() {
            Ok(v) => v,
            Err(e) => return tool_err(e),
        };
        match paper::get_paper(&vault, &args.r#ref) {
            Ok(row) => tool_ok_json(&row),
            Err(e) => tool_err(e),
        }
    }

    #[tool(description = "Import a paper into the vault by arXiv id, DOI, or URL (magic wand).")]
    async fn import_id(&self, Parameters(args): Parameters<ImportIdArgs>) -> CallToolResult {
        let vault = match self.ctrl.local_vault() {
            Ok(v) => v,
            Err(e) => return tool_err(e),
        };
        let parent = args
            .parent
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| self.ctrl.parent_dir());
        let parent = match import::normalize_parent_dir(&parent) {
            Ok(p) => p,
            Err(e) => return tool_err(e),
        };
        let import_args = LookupImportArgs {
            vault_path: vault.to_string_lossy().to_string(),
            parent_dir: parent,
            text: args.text,
            translator_base_url: self.ctrl.translator_url(),
            task_id: None,
        };
        let note_mode = NoteShellMode::parse(&self.ctrl.paper_note_mode());
        let result = if let Some(app) = self.ctrl.app_handle() {
            let cache = app.try_state::<catalog::CapsCache>();
            import::import_by_identifier_with_progress(
                import_args,
                Some(&app),
                cache.as_ref().map(|s| s.inner()),
                note_mode,
            )
            .await
        } else {
            import::import_by_identifier_with_progress(import_args, None, None, note_mode).await
        };
        match result {
            Ok(r) => tool_ok_json(&r),
            Err(e) => tool_err(e),
        }
    }

    #[tool(
        description = "Read NOTES.md for a paper (id or vault-relative path). Empty string if the file does not exist."
    )]
    async fn paper_notes_get(&self, Parameters(args): Parameters<PaperRefArgs>) -> CallToolResult {
        let vault = match self.ctrl.local_vault() {
            Ok(v) => v,
            Err(e) => return tool_err(e),
        };
        let paper = match paper::resolve_paper(&vault, &args.r#ref) {
            Ok(p) => p,
            Err(e) => return tool_err(e),
        };
        match notes::read_notes(&vault, &paper.path) {
            Ok(text) => tool_ok_json(&json!({
                "ref": paper.path,
                "id": paper.id,
                "content": text,
            })),
            Err(e) => tool_err(e),
        }
    }

    #[tool(
        description = "Write NOTES.md for a paper. mode=replace (default) keeps existing YAML frontmatter unless content includes its own; mode=append adds to the body."
    )]
    async fn paper_notes_write(
        &self,
        Parameters(args): Parameters<NotesWriteArgs>,
    ) -> CallToolResult {
        let vault = match self.ctrl.local_vault() {
            Ok(v) => v,
            Err(e) => return tool_err(e),
        };
        let paper = match paper::resolve_paper(&vault, &args.r#ref) {
            Ok(p) => p,
            Err(e) => return tool_err(e),
        };
        let mode = match args.mode.as_deref().map(str::trim).unwrap_or("replace") {
            "" | "replace" => WriteMode::Replace,
            "append" => WriteMode::Append,
            other => {
                return tool_err(AppError::message(format!(
                    "mode must be replace or append, got {other}"
                )));
            }
        };
        match notes::write_notes(&vault, &paper.path, &paper.id, &args.content, mode) {
            Ok(()) => tool_ok_json(&json!({
                "ref": paper.path,
                "id": paper.id,
                "mode": match mode {
                    WriteMode::Replace => "replace",
                    WriteMode::Append => "append",
                },
            })),
            Err(e) => tool_err(e),
        }
    }

    #[tool(description = "Add tags to a paper. Names may use a color suffix like topic:blue.")]
    async fn paper_tag_add(&self, Parameters(args): Parameters<TagArgs>) -> CallToolResult {
        let vault = match self.ctrl.local_vault() {
            Ok(v) => v,
            Err(e) => return tool_err(e),
        };
        let paper = match paper::resolve_paper(&vault, &args.r#ref) {
            Ok(p) => p,
            Err(e) => return tool_err(e),
        };
        let parsed: Result<Vec<_>, _> =
            args.tags.iter().map(|t| paper::parse_tag_spec(t)).collect();
        let parsed = match parsed {
            Ok(t) => t,
            Err(e) => return tool_err(e),
        };
        match papers::add_tags(&vault, &paper.path, &parsed) {
            Ok(row) => tool_ok_json(&row),
            Err(e) => tool_err(e),
        }
    }

    #[tool(description = "Remove tags from a paper (case-insensitive names).")]
    async fn paper_tag_rm(&self, Parameters(args): Parameters<TagArgs>) -> CallToolResult {
        let vault = match self.ctrl.local_vault() {
            Ok(v) => v,
            Err(e) => return tool_err(e),
        };
        let paper = match paper::resolve_paper(&vault, &args.r#ref) {
            Ok(p) => p,
            Err(e) => return tool_err(e),
        };
        let names: Vec<String> = args
            .tags
            .iter()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
        match papers::remove_tags(&vault, &paper.path, &names) {
            Ok(row) => tool_ok_json(&row),
            Err(e) => tool_err(e),
        }
    }
}

#[tool_handler]
impl ServerHandler for AgenteroMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
        .with_instructions(
            "Agentero research vault MCP. Read resource agentero://vault first, then paper_list / paper_get. ref is a paper id or vault-relative path. Notes writes only touch NOTES.md.",
        )
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        Ok(ListResourcesResult::with_all_items(vec![Resource::new(
            VAULT_URI, VAULT_NAME,
        )
        .with_title("Current vault")
        .with_mime_type("text/markdown")]))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, McpError> {
        if request.uri != VAULT_URI {
            return Err(McpError::resource_not_found(
                format!("unknown resource {}", request.uri),
                None,
            ));
        }
        let markdown = resources::vault_markdown(&self.ctrl);
        Ok(ReadResourceResult::new(vec![ResourceContents::text(markdown, VAULT_URI)]).into())
    }
}

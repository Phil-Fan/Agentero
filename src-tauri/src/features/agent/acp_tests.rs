#[cfg(test)]
mod acp_live {
    use crate::features::agent::acp::permission_response;
    use crate::features::agent::discover::resolve_command;
    use crate::features::agent::list_acp_sessions;
    use crate::features::agent::models::{AgentDescriptor, AgentTemplate, CatalogAcpStatus};
    use crate::features::agent::probe_agent;
    use crate::features::agent::templates::catalog_templates;
    use crate::features::agent::terminal_acp::AcpTerminalManager;
    use crate::features::agent::AgentRegistry;
    use agent_client_protocol::schema::v1::{
        ClientCapabilities, ContentBlock, CreateTerminalRequest, ElicitationCapabilities,
        ElicitationFormCapabilities, EnvVariable, InitializeRequest, KillTerminalRequest,
        McpServer, McpServerStdio, NewSessionRequest, PermissionOption, PermissionOptionId,
        PermissionOptionKind, PromptRequest, ReleaseTerminalRequest, RequestPermissionOutcome,
        RequestPermissionRequest, TerminalOutputRequest, TextContent, ToolCallUpdate,
        ToolCallUpdateFields, WaitForTerminalExitRequest,
    };
    use agent_client_protocol::schema::ProtocolVersion;
    use agent_client_protocol::{AcpAgent, Agent, ConnectionTo};
    use std::collections::{HashMap, HashSet};
    use std::path::PathBuf;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    fn desc(
        id: &str,
        name: &str,
        template: AgentTemplate,
        command: &str,
        args: Vec<String>,
    ) -> AgentDescriptor {
        AgentDescriptor {
            id: id.into(),
            name: name.into(),
            template,
            command: command.into(),
            args,
            env: HashMap::new(),
            available: true,
            last_error: None,
            last_probe_ok: None,
            last_probe_agent_name: None,
            last_probe_error: None,
            last_probed_at: None,
        }
    }

    #[tokio::test]
    async fn probe_opencode_acp_if_installed() {
        if resolve_command("opencode").is_none() {
            eprintln!("skip: opencode not on PATH");
            return;
        }
        let mut d = desc(
            "test-opencode",
            "OpenCode",
            AgentTemplate::Opencode,
            "opencode",
            vec!["acp".into()],
        );
        // Inherit shell proxy so local runs match Settings → Agent proxy.
        for key in [
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "ALL_PROXY",
            "http_proxy",
            "https_proxy",
            "all_proxy",
        ] {
            if let Ok(v) = std::env::var(key) {
                d.env.insert(key.to_string(), v);
            }
        }
        let result = probe_agent(&d, None).await;
        eprintln!("probe result: {:?}", result);
        // Live probe is environment-dependent (network / proxy / cold start).
        if !result.available {
            eprintln!(
                "skip assert: opencode probe failed in this environment: {:?}",
                result.error
            );
            return;
        }
    }

    #[tokio::test]
    async fn probe_kimi_acp_if_installed() {
        if resolve_command("kimi").is_none() {
            eprintln!("skip: kimi not on PATH");
            return;
        }
        let mut d = desc(
            "test-kimi",
            "Kimi Code",
            AgentTemplate::KimiCode,
            "kimi",
            vec!["acp".into()],
        );
        for key in [
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "ALL_PROXY",
            "http_proxy",
            "https_proxy",
            "all_proxy",
        ] {
            if let Ok(v) = std::env::var(key) {
                d.env.insert(key.to_string(), v);
            }
        }
        let result = probe_agent(&d, None).await;
        eprintln!("kimi probe result: {:?}", result);
        // Kimi must be authenticated for the probe to succeed.
        if !result.available {
            eprintln!(
                "skip assert: kimi probe failed in this environment: {:?}",
                result.error
            );
            return;
        }
    }

    /// Capture the exact terminal/create payload Kimi Code sends when it needs to
    /// run a shell command (e.g. Glob/Grep) through the ACP terminal capability.
    #[tokio::test]
    #[ignore = "live Kimi ACP probe; requires authenticated `kimi` on PATH"]
    async fn kimi_terminal_create_probe() {
        if resolve_command("kimi").is_none() {
            eprintln!("skip: kimi not on PATH");
            return;
        }

        let mut env: HashMap<String, String> = HashMap::new();
        for key in [
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "ALL_PROXY",
            "http_proxy",
            "https_proxy",
            "all_proxy",
        ] {
            if let Ok(v) = std::env::var(key) {
                env.insert(key.to_string(), v);
            }
        }

        let stdio = McpServerStdio::new("Kimi Code".to_string(), PathBuf::from("kimi"))
            .args(vec!["acp".to_string()])
            .env(
                env.into_iter()
                    .map(|(k, v)| EnvVariable::new(k, v))
                    .collect(),
            );
        let agent = AcpAgent::new(McpServer::Stdio(stdio));

        let terminals = Arc::new(Mutex::new(AcpTerminalManager::new()));
        let timeout = std::time::Duration::from_secs(25);

        let result =
            tokio::time::timeout(
                timeout,
                agent_client_protocol::Client
                    .builder()
                    .name("agentero-terminal-probe")
                    .on_receive_request(
                        {
                            let terminals = terminals.clone();
                            async move |request: CreateTerminalRequest, responder, _cx| {
                                eprintln!(
                            "[probe] terminal/create: command={} args={:?} cwd={:?} env_count={}",
                            request.command, request.args, request.cwd, request.env.len()
                        );
                                match terminals.lock().await.create(request).await {
                                    Ok(resp) => responder.respond(resp),
                                    Err(e) => responder.respond_with_internal_error(e),
                                }
                            }
                        },
                        agent_client_protocol::on_receive_request!(),
                    )
                    .on_receive_request(
                        {
                            let terminals = terminals.clone();
                            async move |request: TerminalOutputRequest, responder, _cx| {
                                match terminals.lock().await.output(&request).await {
                                    Ok(resp) => responder.respond(resp),
                                    Err(e) => responder.respond_with_internal_error(e),
                                }
                            }
                        },
                        agent_client_protocol::on_receive_request!(),
                    )
                    .on_receive_request(
                        {
                            let terminals = terminals.clone();
                            async move |request: WaitForTerminalExitRequest, responder, _cx| {
                                match terminals.lock().await.wait_for_exit(&request).await {
                                    Ok(resp) => responder.respond(resp),
                                    Err(e) => responder.respond_with_internal_error(e),
                                }
                            }
                        },
                        agent_client_protocol::on_receive_request!(),
                    )
                    .on_receive_request(
                        {
                            let terminals = terminals.clone();
                            async move |request: KillTerminalRequest, responder, _cx| {
                                match terminals.lock().await.kill(&request).await {
                                    Ok(resp) => responder.respond(resp),
                                    Err(e) => responder.respond_with_internal_error(e),
                                }
                            }
                        },
                        agent_client_protocol::on_receive_request!(),
                    )
                    .on_receive_request(
                        {
                            let terminals = terminals.clone();
                            async move |request: ReleaseTerminalRequest, responder, _cx| {
                                match terminals.lock().await.release(request).await {
                                    Ok(resp) => responder.respond(resp),
                                    Err(e) => responder.respond_with_internal_error(e),
                                }
                            }
                        },
                        agent_client_protocol::on_receive_request!(),
                    )
                    .on_receive_request(
                        async move |request: RequestPermissionRequest, responder, _cx| {
                            // Auto-approve so we can observe terminal/create traffic.
                            let _ = responder.respond(permission_response(&request, true));
                            Ok(())
                        },
                        agent_client_protocol::on_receive_request!(),
                    )
                    .connect_with(agent, |connection: ConnectionTo<Agent>| async move {
                        let init = connection
                            .send_request(
                                InitializeRequest::new(ProtocolVersion::V1).client_capabilities(
                                    ClientCapabilities::new()
                                        .elicitation(
                                            ElicitationCapabilities::new()
                                                .form(ElicitationFormCapabilities::new()),
                                        )
                                        .terminal(true),
                                ),
                            )
                            .block_task()
                            .await
                            .map_err(|e| {
                                agent_client_protocol::util::internal_error(format!(
                                    "initialize failed: {e}"
                                ))
                            })?;
                        eprintln!(
                            "kimi terminal probe initialized: {:?}",
                            init.protocol_version
                        );

                        let cwd: PathBuf = std::env::var("KIMI_ACP_PROBE_CWD")
                            .map(PathBuf::from)
                            .unwrap_or_else(|_| std::env::current_dir().expect("cwd"));
                        eprintln!("kimi terminal probe cwd: {}", cwd.display());
                        let session = connection
                            .send_request(NewSessionRequest::new(cwd.clone()))
                            .block_task()
                            .await
                            .map_err(|e| {
                                agent_client_protocol::util::internal_error(format!(
                                    "session/new failed: {e}"
                                ))
                            })?;
                        let session_id = session.session_id.to_string();
                        eprintln!("kimi terminal probe session id: {session_id}");

                        connection
                            .send_request(PromptRequest::new(
                                session_id,
                                vec![ContentBlock::Text(TextContent::new(
                                    "List all .tex files under papers/Dflash/2608.02438/source."
                                        .to_string(),
                                ))],
                            ))
                            .block_task()
                            .await
                            .map_err(|e| {
                                agent_client_protocol::util::internal_error(format!(
                                    "session/prompt failed: {e}"
                                ))
                            })?;
                        eprintln!(
                        "kimi terminal probe prompt sent; waiting for terminal/create traffic..."
                    );

                        // Keep the connection alive so terminal requests can arrive and be logged.
                        tokio::time::sleep(std::time::Duration::from_secs(12)).await;
                        Ok(())
                    }),
            )
            .await;

        match result {
            Ok(Ok(())) => eprintln!("kimi terminal probe completed"),
            Ok(Err(e)) => eprintln!("kimi terminal probe failed: {e}"),
            Err(_) => eprintln!("kimi terminal probe timed out"),
        }
    }

    #[test]
    fn catalog_has_common_agents() {
        let cats = catalog_templates();
        let ids: Vec<_> = cats.iter().map(|c| c.id.as_str()).collect();
        assert!(ids.contains(&"opencode"));
        assert!(ids.contains(&"openclaw"));
        assert!(ids.contains(&"claude-acp"));
        assert!(ids.contains(&"codex-acp"));
        assert!(ids.contains(&"hermes"));
        assert!(ids.contains(&"gemini"));
        assert!(ids.contains(&"qodercli"));
        assert!(ids.contains(&"grok-build"));
        assert!(ids.contains(&"pi"));
        assert!(ids.contains(&"dsh"));
        assert!(ids.contains(&"kimi-code"));
        assert!(!ids.contains(&"custom"));
    }

    #[test]
    fn codex_template_uses_the_acp_adapter() {
        let codex = catalog_templates()
            .into_iter()
            .find(|entry| entry.id == "codex-acp")
            .expect("Codex template");

        assert_eq!(codex.command, "codex-acp");
        assert_eq!(codex.args, Vec::<String>::new());
        assert_eq!(codex.detect_command.as_deref(), Some("codex"));
    }

    #[test]
    fn pi_template_uses_the_acp_adapter() {
        let pi = catalog_templates()
            .into_iter()
            .find(|entry| entry.id == "pi")
            .expect("Pi template");

        assert_eq!(pi.command, "pi-acp");
        assert_eq!(pi.args, Vec::<String>::new());
        assert_eq!(pi.detect_command.as_deref(), Some("pi"));
    }

    #[test]
    fn openclaw_and_hermes_templates_use_native_acp() {
        let cats = catalog_templates();
        let openclaw = cats
            .iter()
            .find(|entry| entry.id == "openclaw")
            .expect("OpenClaw template");
        assert_eq!(openclaw.command, "openclaw");
        assert_eq!(openclaw.args, vec!["acp".to_string()]);
        assert_eq!(openclaw.detect_command.as_deref(), Some("openclaw"));

        let hermes = cats
            .iter()
            .find(|entry| entry.id == "hermes")
            .expect("Hermes template");
        assert_eq!(hermes.command, "hermes");
        assert_eq!(hermes.args, vec!["acp".to_string()]);
        assert_eq!(hermes.detect_command.as_deref(), Some("hermes"));
    }

    #[test]
    fn kimi_template_uses_native_acp() {
        let kimi = catalog_templates()
            .into_iter()
            .find(|entry| entry.id == "kimi-code")
            .expect("Kimi Code template");
        assert_eq!(kimi.command, "kimi");
        assert_eq!(kimi.args, vec!["acp".to_string()]);
        assert_eq!(kimi.detect_command.as_deref(), Some("kimi"));
    }

    #[test]
    fn permission_requests_are_cancelled_unless_yolo_is_enabled() {
        let request = RequestPermissionRequest::new(
            "session",
            ToolCallUpdate::new("tool-call", ToolCallUpdateFields::new()),
            vec![
                PermissionOption::new(
                    "reject-once",
                    "Reject once",
                    PermissionOptionKind::RejectOnce,
                ),
                PermissionOption::new(
                    "allow-always",
                    "Allow always",
                    PermissionOptionKind::AllowAlways,
                ),
                PermissionOption::new("allow-once", "Allow once", PermissionOptionKind::AllowOnce),
            ],
        );

        assert!(matches!(
            permission_response(&request, false).outcome,
            RequestPermissionOutcome::Cancelled
        ));
        assert!(matches!(
            permission_response(&request, true).outcome,
            RequestPermissionOutcome::Selected(selected)
                if selected.option_id == PermissionOptionId::new("allow-once")
        ));
    }

    #[test]
    fn scan_catalog_reflects_local_binaries() {
        let reg = AgentRegistry::load();
        let scan = reg.scan_catalog().expect("scan");
        for e in &scan.entries {
            eprintln!(
                "catalog {} binary={} acp_cmd={} status={:?} path={:?}",
                e.template_id,
                e.binary_available,
                e.acp_command_available,
                e.acp_status,
                e.resolved_path
            );
        }
        let by_id = |id: &str| {
            scan.entries
                .iter()
                .find(|e| e.template_id == id)
                .unwrap_or_else(|| panic!("missing catalog entry {id}"))
        };
        if resolve_command("opencode").is_some() {
            assert!(by_id("opencode").binary_available);
            assert_ne!(by_id("opencode").acp_status, CatalogAcpStatus::Missing);
        }
        if resolve_command("openclaw").is_some() {
            assert!(by_id("openclaw").binary_available);
            assert_ne!(by_id("openclaw").acp_status, CatalogAcpStatus::Missing);
        }
        if resolve_command("claude").is_some() {
            assert!(by_id("claude-acp").binary_available);
        }
        if resolve_command("hermes").is_some() {
            assert!(by_id("hermes").binary_available);
            assert_ne!(by_id("hermes").acp_status, CatalogAcpStatus::Missing);
        }
        if resolve_command("qodercli").is_some() {
            assert!(by_id("qodercli").binary_available);
            assert_ne!(by_id("qodercli").acp_status, CatalogAcpStatus::Missing);
        }
        if resolve_command("npx").is_some() {
            assert!(by_id("grok-build").binary_available);
            assert_ne!(by_id("grok-build").acp_status, CatalogAcpStatus::Missing);
        }
        if resolve_command("codex").is_some() {
            assert!(by_id("codex-acp").binary_available);
        }
        if resolve_command("gemini").is_none() {
            assert!(!by_id("gemini").binary_available);
            assert_eq!(by_id("gemini").acp_status, CatalogAcpStatus::Missing);
        }
    }

    /// #338: codex-acp pages `session/list` over a global time window and filters
    /// by cwd inside each page, so the Host must walk the cursor rather than show
    /// only the first page.
    #[tokio::test]
    async fn codex_acp_session_list_walks_cursor_pages() {
        if resolve_command("codex-acp").is_none() {
            eprintln!("skip: codex-acp not on PATH");
            return;
        }
        let d = desc(
            "test-codex-acp",
            "Codex",
            AgentTemplate::CodexAcp,
            "codex-acp",
            vec![],
        );
        let cwd = std::env::current_dir().expect("cwd");
        let result = list_acp_sessions(&d, cwd.clone(), None, None)
            .await
            .expect("session/list must succeed");
        assert!(result.supported, "codex-acp advertises session.list");
        eprintln!(
            "codex-acp sessions for {}: {}",
            cwd.display(),
            result.sessions.len()
        );
        let mut seen = HashSet::new();
        for s in &result.sessions {
            assert!(
                seen.insert(s.session_id.clone()),
                "duplicate session {}",
                s.session_id
            );
            assert_eq!(s.cwd, cwd.to_string_lossy(), "agent must filter by cwd");
        }
    }
}

#[cfg(test)]
mod tool_payload {
    use crate::features::agent::acp::{cap_tool_payload, TOOL_PAYLOAD_MAX_BYTES};
    use serde_json::{json, Value};

    #[test]
    fn small_payloads_pass_through_unchanged() {
        assert_eq!(cap_tool_payload(None), None);
        let small = json!({ "questions": [{ "question": "Proceed?" }] });
        assert_eq!(cap_tool_payload(Some(small.clone())), Some(small));
    }

    #[test]
    fn oversized_string_is_truncated_with_marker() {
        let big = "x".repeat(TOOL_PAYLOAD_MAX_BYTES * 4);
        let capped = cap_tool_payload(Some(Value::String(big.clone()))).unwrap();
        let Value::String(text) = capped else {
            panic!("expected string payload");
        };
        assert!(text.len() < big.len() / 2, "payload must shrink");
        assert!(
            text.starts_with(&"x".repeat(1024)),
            "head must be preserved"
        );
        assert!(text.contains("truncated"), "marker must be present");
    }

    #[test]
    fn oversized_object_falls_back_to_json_head() {
        let big = json!({ "fileText": "y".repeat(TOOL_PAYLOAD_MAX_BYTES * 2) });
        let capped = cap_tool_payload(Some(big)).unwrap();
        let Value::String(text) = capped else {
            panic!("expected string payload");
        };
        assert!(text.starts_with("{\"fileText\":"));
        assert!(text.contains("truncated"));
        assert!(text.len() <= TOOL_PAYLOAD_MAX_BYTES + 128);
    }

    #[test]
    fn truncation_respects_char_boundaries() {
        // Multi-byte chars across the cut point must not panic.
        let big = "汉".repeat(TOOL_PAYLOAD_MAX_BYTES);
        let capped = cap_tool_payload(Some(Value::String(big))).unwrap();
        let Value::String(text) = capped else {
            panic!("expected string payload");
        };
        assert!(text.contains("truncated"));
    }
}

#[cfg(test)]
mod list_sessions_paging {
    use crate::features::agent::acp::{
        list_sessions_page_done, LIST_SESSIONS_BUDGET, LIST_SESSIONS_MAX, LIST_SESSIONS_MAX_PAGES,
    };
    use std::time::Duration;

    #[test]
    fn exhausted_cursor_stops() {
        assert!(list_sessions_page_done(
            None,
            Some("2026-08-10T00:00:00Z"),
            3,
            1,
            Duration::ZERO
        ));
    }

    #[test]
    fn empty_page_with_a_fresh_cursor_keeps_paging() {
        // #338: codex-acp pages globally and filters by cwd, so a page holding
        // zero sessions for this vault still has more results behind it.
        assert!(!list_sessions_page_done(
            Some("2026-08-05T00:00:00Z"),
            Some("2026-08-10T00:00:00Z"),
            0,
            7,
            Duration::ZERO
        ));
    }

    #[test]
    fn stalled_cursor_stops() {
        let same = "2026-08-10T00:00:00Z";
        assert!(list_sessions_page_done(
            Some(same),
            Some(same),
            1,
            2,
            Duration::ZERO
        ));
    }

    #[test]
    fn caps_stop_the_walk() {
        let next = Some("2026-08-05T00:00:00Z");
        let prev = Some("2026-08-10T00:00:00Z");
        assert!(list_sessions_page_done(
            next,
            prev,
            LIST_SESSIONS_MAX,
            1,
            Duration::ZERO
        ));
        assert!(list_sessions_page_done(
            next,
            prev,
            0,
            LIST_SESSIONS_MAX_PAGES,
            Duration::ZERO
        ));
        assert!(list_sessions_page_done(
            next,
            prev,
            0,
            1,
            LIST_SESSIONS_BUDGET
        ));
    }
}

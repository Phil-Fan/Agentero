//! `agentero mark *` — reading marks (region-anchored first).

use crate::commands::layout::{self as layout_cmd, LayoutIndexItem};
use crate::error::{CliError, ExitCode};
use crate::prompt;
use crate::resolve::{paper_dir, resolve_paper, resolve_vault, GlobalOpts};
use clap::Subcommand;
use serde::Serialize;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Subcommand)]
pub enum MarkCmd {
    /// List per-id mark files under `{paper}/marks/`.
    List {
        /// Vault-relative paper path or id.
        r#ref: String,
        /// Filter by kind (highlight|ask|translate|agent-trace).
        #[arg(long = "kind", value_name = "KIND")]
        kind: Option<String>,
    },
    /// Get one mark by id.
    Get {
        r#ref: String,
        /// Mark id. Accepts a leading `-`: ids are nanoids and that alphabet
        /// includes `-`, so ~1 in 64 would otherwise parse as an unknown flag.
        #[arg(allow_hyphen_values = true)]
        id: String,
    },
    /// Add a mark. Prefer `--region` for figure/table/algorithm/formula anchors.
    Add {
        r#ref: String,
        /// Layout index region id from `layout list` (resolved geometry).
        #[arg(long = "region", value_name = "ID")]
        region: Option<String>,
        /// Mark kind (default: highlight with --region; ask when --question set).
        #[arg(long = "kind", value_name = "KIND")]
        kind: Option<String>,
        /// Annotation note / comment.
        #[arg(long = "comment", value_name = "TEXT")]
        comment: Option<String>,
        /// Optional user question for kind=ask.
        #[arg(long = "question", value_name = "TEXT")]
        question: Option<String>,
        /// Highlight color (yellow|green|blue|pink|purple). Default yellow (desktop palette).
        /// Named `--mark-color` to avoid clashing with global `--color` (ANSI TTY paint).
        #[arg(long = "mark-color", value_name = "NAME", default_value = "yellow")]
        mark_color: String,
        /// Optional quote override (default: region title).
        #[arg(long = "quote", value_name = "TEXT")]
        quote: Option<String>,
    },
    /// Delete a mark file.
    Delete {
        r#ref: String,
        /// Mark id. Accepts a leading `-` (see `Get`).
        #[arg(allow_hyphen_values = true)]
        id: String,
    },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MarkListItem {
    id: String,
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    page: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    geometry: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    quote: Option<String>,
    path: String,
}

pub async fn run(cmd: MarkCmd, globals: &GlobalOpts) -> Result<Value, CliError> {
    match cmd {
        MarkCmd::List { r#ref, kind } => list(globals, &r#ref, kind.as_deref()),
        MarkCmd::Get { r#ref, id } => get(globals, &r#ref, &id),
        MarkCmd::Add {
            r#ref,
            region,
            kind,
            comment,
            question,
            mark_color,
            quote,
        } => add(
            globals,
            &r#ref,
            region.as_deref(),
            kind.as_deref(),
            comment.as_deref(),
            question.as_deref(),
            &mark_color,
            quote.as_deref(),
        ),
        MarkCmd::Delete { r#ref, id } => delete(globals, &r#ref, &id),
    }
}

fn list(globals: &GlobalOpts, ref_: &str, kind: Option<&str>) -> Result<Value, CliError> {
    let vault = resolve_vault(globals)?;
    let paper = resolve_paper(&vault, ref_, globals)?;
    let marks_dir = paper_dir(&vault, &paper.path).join("marks");
    let mut items: Vec<MarkListItem> = Vec::new();

    if marks_dir.is_dir() {
        let rd = fs::read_dir(&marks_dir)
            .map_err(|e| CliError::message(format!("failed to read marks/: {e}")))?;
        for entry in rd.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.ends_with(".json") || name == "annotations.json" {
                continue;
            }
            let id = name.trim_end_matches(".json").to_string();
            let path = entry.path();
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            let Ok(raw) = serde_json::from_str::<Value>(&text) else {
                continue;
            };
            let k = raw
                .get("kind")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if let Some(filter) = kind {
                if !k.eq_ignore_ascii_case(filter) {
                    continue;
                }
            }
            let page = raw.get("page").and_then(|v| v.as_u64()).or_else(|| {
                raw.get("anchor")
                    .and_then(|a| a.get("page"))
                    .and_then(|v| v.as_u64())
            });
            let geometry = raw
                .get("geometry")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let quote = raw
                .get("quote")
                .and_then(|v| v.as_str())
                .or_else(|| {
                    raw.get("anchor")
                        .and_then(|a| a.get("quote"))
                        .and_then(|v| v.as_str())
                })
                .map(|s| s.to_string());
            items.push(MarkListItem {
                id,
                kind: k,
                page,
                geometry,
                quote,
                path: format!("{}/marks/{name}", paper.path),
            });
        }
    }

    items.sort_by(|a, b| a.id.cmp(&b.id));
    let lines: Vec<String> = if items.is_empty() {
        vec!["(no marks)".into()]
    } else {
        items
            .iter()
            .map(|i| {
                format!(
                    "{}  {}  page={}  {}",
                    i.id,
                    i.kind,
                    i.page.map(|p| p.to_string()).unwrap_or_else(|| "-".into()),
                    i.quote.as_deref().unwrap_or("")
                )
            })
            .collect()
    };
    Ok(json!({
        "paperPath": paper.path,
        "count": items.len(),
        "items": items,
        "lines": lines,
    }))
}

fn get(globals: &GlobalOpts, ref_: &str, id: &str) -> Result<Value, CliError> {
    let vault = resolve_vault(globals)?;
    let paper = resolve_paper(&vault, ref_, globals)?;
    let path = mark_file(&vault, &paper.path, id);
    if !path.is_file() {
        return Err(CliError::with_details(
            "mark_not_found",
            format!("mark not found: {id}"),
            json!({ "paperPath": paper.path, "id": id }),
            ExitCode::Business,
        ));
    }
    let text = fs::read_to_string(&path)
        .map_err(|e| CliError::message(format!("failed to read mark: {e}")))?;
    let raw: Value = serde_json::from_str(&text)
        .map_err(|e| CliError::message(format!("invalid mark JSON: {e}")))?;
    Ok(json!({
        "paperPath": paper.path,
        "path": format!("{}/marks/{id}.json", paper.path),
        "mark": raw,
        "lines": [text.trim_end()],
    }))
}

#[allow(clippy::too_many_arguments)]
fn add(
    globals: &GlobalOpts,
    ref_: &str,
    region: Option<&str>,
    kind: Option<&str>,
    comment: Option<&str>,
    question: Option<&str>,
    mark_color: &str,
    quote: Option<&str>,
) -> Result<Value, CliError> {
    let vault = resolve_vault(globals)?;
    let paper = resolve_paper(&vault, ref_, globals)?;

    let Some(region_id) = region.map(str::trim).filter(|s| !s.is_empty()) else {
        return Err(CliError::usage(
            "mark add currently requires --region <id> from `layout list` \
             (sentence highlight/translate pending marks come later)",
        ));
    };

    let region_item = layout_cmd::load_region(&vault, &paper.path, region_id)?;
    let kind = resolve_kind(kind, question)?;
    let color = normalize_mark_color(mark_color)?;
    let id = new_mark_id();
    let now = iso_now();
    let quote_text = quote
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| region_item.title.clone())
        .unwrap_or_else(|| format!("{} {}", region_item.section, region_item.id));

    let mark = match kind.as_str() {
        "highlight" => build_highlight_mark(
            &id,
            &paper.path,
            &now,
            &region_item,
            &quote_text,
            comment,
            &color,
        ),
        "ask" => build_ask_mark(
            &id,
            &paper.path,
            &now,
            &region_item,
            &quote_text,
            question,
            comment,
        ),
        other => {
            return Err(CliError::usage(format!(
                "unsupported --kind '{other}' for --region (use highlight or ask)"
            )));
        }
    };

    let marks_dir = paper_dir(&vault, &paper.path).join("marks");
    fs::create_dir_all(&marks_dir)
        .map_err(|e| CliError::message(format!("failed to create marks/: {e}")))?;
    let file_path = marks_dir.join(format!("{id}.json"));
    let body = format!(
        "{}\n",
        serde_json::to_string_pretty(&mark)
            .map_err(|e| { CliError::message(format!("serialize mark: {e}")) })?
    );
    fs::write(&file_path, body)
        .map_err(|e| CliError::message(format!("failed to write mark: {e}")))?;

    let rel = format!("{}/marks/{id}.json", paper.path);
    Ok(json!({
        "paperPath": paper.path,
        "path": rel,
        "mark": mark,
        "lines": [format!(
            "wrote {rel}  kind={kind}  region={region_id}  geometry=resolved"
        )],
    }))
}

fn delete(globals: &GlobalOpts, ref_: &str, id: &str) -> Result<Value, CliError> {
    let vault = resolve_vault(globals)?;
    let paper = resolve_paper(&vault, ref_, globals)?;
    let path = mark_file(&vault, &paper.path, id);
    if !path.is_file() {
        return Err(CliError::with_details(
            "mark_not_found",
            format!("mark not found: {id}"),
            json!({ "paperPath": paper.path, "id": id }),
            ExitCode::Business,
        ));
    }
    if !globals.yes {
        let ok = prompt::confirm(
            globals,
            &format!("Delete mark {id} under {}?", paper.path),
            false,
        )?;
        if !ok {
            return Err(CliError::needs_confirmation("delete cancelled"));
        }
    }
    fs::remove_file(&path).map_err(|e| CliError::message(format!("failed to delete mark: {e}")))?;
    Ok(json!({
        "paperPath": paper.path,
        "id": id,
        "deleted": true,
        "lines": [format!("deleted {}/marks/{id}.json", paper.path)],
    }))
}

fn resolve_kind(kind: Option<&str>, question: Option<&str>) -> Result<String, CliError> {
    if let Some(k) = kind {
        let t = k.trim().to_ascii_lowercase();
        if t == "highlight" || t == "ask" {
            return Ok(t);
        }
        return Err(CliError::usage(
            "--kind must be highlight or ask when using --region",
        ));
    }
    if question.map(str::trim).filter(|s| !s.is_empty()).is_some() {
        return Ok("ask".into());
    }
    Ok("highlight".into())
}

/// Same palette as desktop `DEFAULT_HIGHLIGHT_COLOR` / `HIGHLIGHT_COLORS`.
fn normalize_mark_color(raw: &str) -> Result<String, CliError> {
    let t = raw.trim().to_ascii_lowercase();
    match t.as_str() {
        "yellow" | "green" | "blue" | "pink" | "purple" => Ok(t),
        other => Err(CliError::usage(format!(
            "unknown --mark-color '{other}' (use yellow|green|blue|pink|purple; default yellow)"
        ))),
    }
}

fn build_highlight_mark(
    id: &str,
    paper_path: &str,
    now: &str,
    region: &LayoutIndexItem,
    quote: &str,
    comment: Option<&str>,
    color: &str,
) -> Value {
    let mut mark = json!({
        "version": 1,
        "kind": "highlight",
        "id": id,
        "paperPath": paper_path,
        "createdAt": now,
        "updatedAt": now,
        "page": region.page,
        "rects": [{
            "x": region.bbox.x,
            "y": region.bbox.y,
            "w": region.bbox.w,
            "h": region.bbox.h,
        }],
        "quote": quote,
        "color": color,
        "geometry": "resolved",
        "layoutRef": {
            "regionId": region.id,
            "stableKey": region.stable_key,
            "kind": region.kind,
            "section": region.section,
            "layoutRegionId": region.layout_region_id,
            "title": region.title,
        },
    });
    if let Some(c) = comment.map(str::trim).filter(|s| !s.is_empty()) {
        mark["comment"] = json!(c);
    }
    mark
}

fn build_ask_mark(
    id: &str,
    paper_path: &str,
    now: &str,
    region: &LayoutIndexItem,
    quote: &str,
    question: Option<&str>,
    comment: Option<&str>,
) -> Value {
    let mut messages = Vec::new();
    let q = question
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| comment.map(str::trim).filter(|s| !s.is_empty()));
    if let Some(content) = q {
        messages.push(json!({
            "id": format!("{id}-q"),
            "role": "user",
            "content": content,
            "createdAt": now,
        }));
    }
    json!({
        "version": 1,
        "kind": "ask",
        "id": id,
        "paperPath": paper_path,
        "createdAt": now,
        "updatedAt": now,
        "status": "open",
        "geometry": "resolved",
        "anchor": {
            "page": region.page,
            "rects": [{
                "x": region.bbox.x,
                "y": region.bbox.y,
                "w": region.bbox.w,
                "h": region.bbox.h,
            }],
            "quote": quote,
            "trigger": "region",
        },
        "messages": messages,
        "layoutRef": {
            "regionId": region.id,
            "stableKey": region.stable_key,
            "kind": region.kind,
            "section": region.section,
            "layoutRegionId": region.layout_region_id,
            "title": region.title,
        },
    })
}

fn mark_file(vault: &Path, paper_path: &str, id: &str) -> std::path::PathBuf {
    let id = id.trim();
    paper_dir(vault, paper_path)
        .join("marks")
        .join(format!("{id}.json"))
}

fn new_mark_id() -> String {
    // url-safe short id (nanoid-like alphabet); good enough for local mark files.
    const ALPHABET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ_abcdefghijklmnopqrstuvwxyz-";
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut n = nanos ^ (std::process::id() as u128).wrapping_shl(40) ^ ((nanos >> 17) | 1);
    let mut out = String::with_capacity(10);
    for i in 0..10u128 {
        out.push(ALPHABET[(n % 64) as usize] as char);
        n = n
            .wrapping_mul(0x9e37_79b9_7f4a_7c15)
            .wrapping_add(i.wrapping_mul(0x85eb_ca6b));
    }
    out
}

fn iso_now() -> String {
    // RFC3339-ish UTC without chrono dependency
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Use simple formatting via humantime-less manual UTC
    // Prefer time crate if available — otherwise approximate with unix.
    // agentero already has no `time` in cli; format via serde_json datetime is fine enough.
    format_unix_rfc3339(secs)
}

fn format_unix_rfc3339(secs: u64) -> String {
    // Civil date from Unix day math (proleptic Gregorian, UTC).
    let z = secs / 86400;
    let tod = secs % 86400;
    let hh = tod / 3600;
    let mm = (tod % 3600) / 60;
    let ss = tod % 60;
    let (y, m, d) = civil_from_days(z as i64);
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}.000Z")
}

/// Algorithm from Howard Hinnant (public domain).
fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as u32, d as u32)
}

// Re-export LayoutIndexItem fields via layout module — mark needs public type.
// layout.rs already has pub struct LayoutIndexItem.

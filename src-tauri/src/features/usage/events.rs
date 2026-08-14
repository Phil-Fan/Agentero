//! Append-only event writes, daily rollup, rename, and queries.

use crate::core::error::AppError;
use crate::features::usage::schema::ensure_usage_at;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::path::Path;

const MAX_BATCH: usize = 200;
const MAX_KIND: usize = 64;
const MAX_PATH: usize = 1024;
const MAX_VAULT: usize = 1024;
const MAX_MODE: usize = 64;
const MAX_EXTRA_BYTES: usize = 8 * 1024;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageRecord {
    #[serde(default)]
    pub ts: Option<String>,
    #[serde(default)]
    pub vault: Option<String>,
    pub kind: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub dur_ms: Option<i64>,
    #[serde(default)]
    pub extra: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageEvent {
    pub id: i64,
    pub ts: String,
    pub vault: Option<String>,
    pub kind: String,
    pub path: Option<String>,
    pub mode: Option<String>,
    pub dur_ms: Option<i64>,
    pub extra: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default)]
pub struct ListFilter {
    pub vault: Option<String>,
    pub kind: Option<String>,
    pub path_prefix: Option<String>,
    pub since: Option<String>,
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageKindCount {
    pub kind: String,
    pub count: i64,
    pub dur_ms: i64,
}

pub fn record_events(db_path: &Path, events: &[UsageRecord]) -> Result<usize, AppError> {
    if events.is_empty() {
        return Ok(0);
    }
    if events.len() > MAX_BATCH {
        return Err(AppError::message(format!(
            "too many usage events in one batch ({}); max {MAX_BATCH}",
            events.len()
        )));
    }
    let conn = ensure_usage_at(db_path)?;
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| AppError::message(format!("usage tx: {e}")))?;
    let mut inserted = 0usize;
    for raw in events {
        let event = normalize(raw)?;
        tx.execute(
            "INSERT INTO usage_events (ts, vault, kind, path, mode, dur_ms, extra)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                event.ts,
                event.vault,
                event.kind,
                event.path,
                event.mode,
                event.dur_ms,
                event.extra,
            ],
        )
        .map_err(|e| AppError::message(format!("insert usage event: {e}")))?;
        upsert_daily(&tx, &event)?;
        inserted += 1;
    }
    tx.commit()
        .map_err(|e| AppError::message(format!("usage commit: {e}")))?;
    Ok(inserted)
}

pub fn rename_path(db_path: &Path, vault: &str, from: &str, to: &str) -> Result<u64, AppError> {
    let from = normalize_rel(from);
    let to = normalize_rel(to);
    if from.is_empty() || to.is_empty() || from == to {
        return Ok(0);
    }
    let vault = vault.trim();
    if vault.is_empty() {
        return Ok(0);
    }
    let conn = ensure_usage_at(db_path)?;
    let like = format!("{from}/%");
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| AppError::message(format!("usage rename tx: {e}")))?;
    let events = tx
        .execute(
            "UPDATE usage_events
             SET path = CASE
               WHEN path = ?1 THEN ?2
               WHEN path LIKE ?3 THEN ?2 || substr(path, length(?1) + 1)
               ELSE path
             END
             WHERE vault = ?4 AND (path = ?1 OR path LIKE ?3)",
            params![from, to, like, vault],
        )
        .map_err(|e| AppError::message(format!("rename usage_events: {e}")))?;
    let daily = tx
        .execute(
            "UPDATE usage_daily
             SET path = CASE
               WHEN path = ?1 THEN ?2
               WHEN path LIKE ?3 THEN ?2 || substr(path, length(?1) + 1)
               ELSE path
             END
             WHERE vault = ?4 AND (path = ?1 OR path LIKE ?3)",
            params![from, to, like, vault],
        )
        .map_err(|e| AppError::message(format!("rename usage_daily: {e}")))?;
    tx.commit()
        .map_err(|e| AppError::message(format!("usage rename commit: {e}")))?;
    Ok((events + daily) as u64)
}

pub fn list_events(db_path: &Path, filter: &ListFilter) -> Result<Vec<UsageEvent>, AppError> {
    let conn = ensure_usage_at(db_path)?;
    let limit = filter.limit.clamp(1, 1000) as i64;
    let vault = filter
        .vault
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("");
    let kind = filter
        .kind
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("");
    let prefix = filter
        .path_prefix
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("");
    let like = if prefix.is_empty() {
        String::new()
    } else {
        format!("{prefix}/%")
    };
    let since = filter
        .since
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("");
    let mut stmt = conn
        .prepare(
            "SELECT id, ts, vault, kind, path, mode, dur_ms, extra
             FROM usage_events
             WHERE (?1 = '' OR vault = ?1)
               AND (?2 = '' OR kind = ?2)
               AND (?3 = '' OR path = ?3 OR path LIKE ?4)
               AND (?5 = '' OR ts >= ?5)
             ORDER BY ts DESC, id DESC
             LIMIT ?6",
        )
        .map_err(|e| AppError::message(format!("list usage prepare: {e}")))?;
    let mut rows = stmt
        .query(params![vault, kind, prefix, like, since, limit])
        .map_err(|e| AppError::message(format!("list usage query: {e}")))?;
    let mut out = Vec::new();
    while let Some(row) = rows
        .next()
        .map_err(|e| AppError::message(format!("list usage row: {e}")))?
    {
        out.push(row_to_event(row)?);
    }
    Ok(out)
}

pub fn summarize(
    db_path: &Path,
    vault: Option<&str>,
    since: Option<&str>,
) -> Result<Vec<UsageKindCount>, AppError> {
    let conn = ensure_usage_at(db_path)?;
    let vault = vault.map(str::trim).filter(|s| !s.is_empty()).unwrap_or("");
    let since = since.map(str::trim).filter(|s| !s.is_empty()).unwrap_or("");
    let mut stmt = conn
        .prepare(
            "SELECT kind, COUNT(*), COALESCE(SUM(dur_ms), 0)
             FROM usage_events
             WHERE (?1 = '' OR vault = ?1)
               AND (?2 = '' OR ts >= ?2)
             GROUP BY kind
             ORDER BY COUNT(*) DESC, kind ASC",
        )
        .map_err(|e| AppError::message(format!("usage summary prepare: {e}")))?;
    let mut rows = stmt
        .query(params![vault, since])
        .map_err(|e| AppError::message(format!("usage summary query: {e}")))?;
    let mut out = Vec::new();
    while let Some(row) = rows
        .next()
        .map_err(|e| AppError::message(format!("usage summary row: {e}")))?
    {
        out.push(UsageKindCount {
            kind: row.get(0)?,
            count: row.get(1)?,
            dur_ms: row.get(2)?,
        });
    }
    Ok(out)
}

pub fn clear_all(db_path: &Path) -> Result<u64, AppError> {
    let conn = ensure_usage_at(db_path)?;
    let n = conn
        .execute("DELETE FROM usage_events", [])
        .map_err(|e| AppError::message(format!("clear usage_events: {e}")))?;
    conn.execute("DELETE FROM usage_daily", [])
        .map_err(|e| AppError::message(format!("clear usage_daily: {e}")))?;
    Ok(n as u64)
}

pub fn clear_vault(db_path: &Path, vault: &str) -> Result<u64, AppError> {
    let vault = vault.trim();
    if vault.is_empty() {
        return Ok(0);
    }
    let conn = ensure_usage_at(db_path)?;
    let n = conn
        .execute("DELETE FROM usage_events WHERE vault = ?1", [vault])
        .map_err(|e| AppError::message(format!("clear vault usage_events: {e}")))?;
    conn.execute("DELETE FROM usage_daily WHERE vault = ?1", [vault])
        .map_err(|e| AppError::message(format!("clear vault usage_daily: {e}")))?;
    Ok(n as u64)
}

struct Normalized {
    ts: String,
    vault: Option<String>,
    kind: String,
    path: Option<String>,
    mode: Option<String>,
    dur_ms: Option<i64>,
    extra: Option<String>,
}

fn normalize(raw: &UsageRecord) -> Result<Normalized, AppError> {
    let kind = raw.kind.trim().to_ascii_lowercase();
    if kind.is_empty() || kind.len() > MAX_KIND || !kind_ok(&kind) {
        return Err(AppError::message(format!(
            "invalid usage kind: {}",
            raw.kind
        )));
    }
    let vault = optional_trimmed(&raw.vault, MAX_VAULT);
    let path = raw
        .path
        .as_deref()
        .map(normalize_rel)
        .filter(|s| !s.is_empty());
    if path.as_ref().is_some_and(|p| p.len() > MAX_PATH) {
        return Err(AppError::message("usage path too long"));
    }
    let mode = optional_trimmed(&raw.mode, MAX_MODE);
    let dur_ms = raw.dur_ms.filter(|n| *n >= 0);
    let extra = match &raw.extra {
        None | Some(serde_json::Value::Null) => None,
        Some(value) => {
            let encoded = serde_json::to_string(value)?;
            if encoded.len() > MAX_EXTRA_BYTES {
                return Err(AppError::message("usage extra payload too large"));
            }
            Some(encoded)
        }
    };
    let ts = raw
        .ts
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(now_rfc3339);
    Ok(Normalized {
        ts,
        vault,
        kind,
        path,
        mode,
        dur_ms,
        extra,
    })
}

fn upsert_daily(tx: &rusqlite::Transaction<'_>, event: &Normalized) -> Result<(), AppError> {
    let day = event.ts.get(..10).unwrap_or("1970-01-01");
    let vault = event.vault.as_deref().unwrap_or("");
    let path = event.path.as_deref().unwrap_or("");
    let dur = event.dur_ms.unwrap_or(0);
    tx.execute(
        "INSERT INTO usage_daily (day, vault, kind, path, count, dur_ms)
         VALUES (?1, ?2, ?3, ?4, 1, ?5)
         ON CONFLICT(day, vault, kind, path) DO UPDATE SET
           count = count + 1,
           dur_ms = dur_ms + excluded.dur_ms",
        params![day, vault, event.kind, path, dur],
    )
    .map_err(|e| AppError::message(format!("upsert usage_daily: {e}")))?;
    Ok(())
}

fn row_to_event(row: &rusqlite::Row<'_>) -> Result<UsageEvent, AppError> {
    let extra_raw: Option<String> = row.get(7)?;
    let extra = match extra_raw.as_deref() {
        None | Some("") => None,
        Some(raw) => Some(serde_json::from_str(raw)?),
    };
    Ok(UsageEvent {
        id: row.get(0)?,
        ts: row.get(1)?,
        vault: row.get(2)?,
        kind: row.get(3)?,
        path: row.get(4)?,
        mode: row.get(5)?,
        dur_ms: row.get(6)?,
        extra,
    })
}

fn kind_ok(kind: &str) -> bool {
    kind.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '_' || c == '-')
}

fn optional_trimmed(value: &Option<String>, max: usize) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.chars().take(max).collect())
}

fn normalize_rel(path: &str) -> String {
    path.trim().replace('\\', "/").trim_matches('/').to_string()
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub fn since_rfc3339_days(days: u32) -> String {
    let days = i64::from(days.max(1));
    (chrono::Utc::now() - chrono::Duration::days(days))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// Used by Host commands that should never fail a user-facing mutation.
#[cfg(feature = "desktop")]
pub fn rename_path_best_effort(vault: &str, from: &str, to: &str) {
    match rename_path(&crate::features::usage::usage_db_path(), vault, from, to) {
        Ok(_) => {}
        Err(e) => {
            log::warn!(target: "agentero::usage", "rename usage paths {from} → {to}: {e}");
        }
    }
}

/// Default-db wrapper used by Tauri commands.
#[cfg(feature = "desktop")]
pub fn record_default(events: &[UsageRecord]) -> Result<usize, AppError> {
    record_events(&crate::features::usage::usage_db_path(), events)
}

#[cfg(feature = "desktop")]
pub fn list_default(filter: &ListFilter) -> Result<Vec<UsageEvent>, AppError> {
    list_events(&crate::features::usage::usage_db_path(), filter)
}

#[cfg(feature = "desktop")]
pub fn summarize_default(
    vault: Option<&str>,
    since: Option<&str>,
) -> Result<Vec<UsageKindCount>, AppError> {
    summarize(&crate::features::usage::usage_db_path(), vault, since)
}

#[cfg(feature = "desktop")]
pub fn clear_default(vault: Option<&str>) -> Result<u64, AppError> {
    let path = crate::features::usage::usage_db_path();
    match vault.map(str::trim).filter(|s| !s.is_empty()) {
        Some(v) => clear_vault(&path, v),
        None => clear_all(&path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_db() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "agentero-usage-events-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir.join("usage.sqlite")
    }

    fn rec(kind: &str, path: &str) -> UsageRecord {
        UsageRecord {
            ts: Some("2026-08-14T10:00:00.000Z".into()),
            vault: Some("/vaults/demo".into()),
            kind: kind.into(),
            path: Some(path.into()),
            mode: Some("pdf".into()),
            dur_ms: Some(1200),
            extra: Some(serde_json::json!({ "source": "arxiv" })),
        }
    }

    #[test]
    fn records_lists_and_summarizes() {
        let db = temp_db();
        let n = record_events(
            &db,
            &[rec("paper.open", "papers/a"), rec("paper.open", "papers/b")],
        )
        .unwrap();
        assert_eq!(n, 2);
        let rows = list_events(
            &db,
            &ListFilter {
                vault: Some("/vaults/demo".into()),
                limit: 10,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].kind, "paper.open");
        let summary = summarize(&db, Some("/vaults/demo"), None).unwrap();
        assert_eq!(summary[0].kind, "paper.open");
        assert_eq!(summary[0].count, 2);
        let _ = fs::remove_dir_all(db.parent().unwrap());
    }

    #[test]
    fn rename_updates_prefix() {
        let db = temp_db();
        record_events(
            &db,
            &[
                rec("paper.open", "papers/old"),
                rec("note.open", "papers/old/NOTES.md"),
            ],
        )
        .unwrap();
        let n = rename_path(&db, "/vaults/demo", "papers/old", "papers/new").unwrap();
        assert!(n >= 2);
        let rows = list_events(
            &db,
            &ListFilter {
                path_prefix: Some("papers/new".into()),
                limit: 10,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(rows.len(), 2);
        let _ = fs::remove_dir_all(db.parent().unwrap());
    }

    #[test]
    fn rejects_bad_kind() {
        let db = temp_db();
        let err = record_events(
            &db,
            &[UsageRecord {
                kind: "DROP TABLE".into(),
                ..rec("paper.open", "papers/a")
            }],
        )
        .unwrap_err();
        assert!(err.to_string().contains("invalid usage kind"));
        let _ = fs::remove_dir_all(db.parent().unwrap());
    }
}

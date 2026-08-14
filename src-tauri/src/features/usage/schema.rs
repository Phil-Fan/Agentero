//! usage.sqlite schema and open/migrate helpers.

use crate::core::error::AppError;
use crate::core::paths;
use rusqlite::Connection;
use std::fs;
use std::path::Path;

/// Current usage schema version written to `schema_meta`.
pub const SCHEMA_VERSION: i32 = 1;

/// Raw events older than this are pruned on open.
pub const EVENT_RETENTION_DAYS: i32 = 180;

/// Daily aggregates older than this are pruned on open.
pub const DAILY_RETENTION_DAYS: i32 = 730;

const DDL_V1: &str = r#"
CREATE TABLE IF NOT EXISTS schema_meta (
    key   TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS usage_events (
    id     INTEGER PRIMARY KEY AUTOINCREMENT,
    ts     TEXT    NOT NULL,
    vault  TEXT,
    kind   TEXT    NOT NULL,
    path   TEXT,
    mode   TEXT,
    dur_ms INTEGER,
    extra  TEXT
);
CREATE INDEX IF NOT EXISTS idx_usage_events_ts    ON usage_events(ts);
CREATE INDEX IF NOT EXISTS idx_usage_events_vault ON usage_events(vault, ts);
CREATE INDEX IF NOT EXISTS idx_usage_events_path  ON usage_events(path, ts);
CREATE INDEX IF NOT EXISTS idx_usage_events_kind  ON usage_events(kind, ts);

CREATE TABLE IF NOT EXISTS usage_daily (
    day    TEXT    NOT NULL,
    vault  TEXT    NOT NULL DEFAULT '',
    kind   TEXT    NOT NULL,
    path   TEXT    NOT NULL DEFAULT '',
    count  INTEGER NOT NULL DEFAULT 0,
    dur_ms INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (day, vault, kind, path)
);
"#;

/// Default on-disk path: `$XDG_DATA_HOME/agentero/usage.sqlite`.
pub fn usage_db_path() -> std::path::PathBuf {
    paths::usage_db_path()
}

/// Open the process default usage database, creating it if needed.
pub fn ensure_usage() -> Result<Connection, AppError> {
    ensure_usage_at(&usage_db_path())
}

/// Open (or create) a usage database at `db_path`.
pub fn ensure_usage_at(db_path: &Path) -> Result<Connection, AppError> {
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(db_path)
        .map_err(|e| AppError::message(format!("open usage {}: {e}", db_path.display())))?;
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;\n\
         PRAGMA synchronous = NORMAL;\n\
         PRAGMA busy_timeout = 5000;\n\
         PRAGMA foreign_keys = ON;",
    )
    .map_err(|e| AppError::message(format!("usage pragma: {e}")))?;
    migrate(&conn)?;
    prune(&conn)?;
    Ok(conn)
}

pub fn schema_version(conn: &Connection) -> Result<i32, AppError> {
    conn.query_row(
        "SELECT value FROM schema_meta WHERE key = 'schema_version'",
        [],
        |row| {
            let raw: String = row.get(0)?;
            Ok(raw.parse::<i32>().unwrap_or(0))
        },
    )
    .map_err(|e| AppError::message(format!("usage schema_version: {e}")))
}

fn migrate(conn: &Connection) -> Result<(), AppError> {
    let version = schema_version(conn).unwrap_or(0);
    if version > SCHEMA_VERSION {
        return Err(AppError::message(format!(
            "usage schema version {version} is newer than this app supports ({SCHEMA_VERSION}); upgrade Agentero"
        )));
    }
    if version < 1 {
        conn.execute_batch(DDL_V1)
            .map_err(|e| AppError::message(format!("usage migrate v1: {e}")))?;
        set_schema_version(conn, 1)?;
    }
    Ok(())
}

fn set_schema_version(conn: &Connection, version: i32) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO schema_meta(key, value) VALUES('schema_version', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [version.to_string()],
    )
    .map_err(|e| AppError::message(format!("write usage schema_version: {e}")))?;
    Ok(())
}

fn prune(conn: &Connection) -> Result<(), AppError> {
    conn.execute(
        "DELETE FROM usage_events WHERE substr(ts, 1, 10) < date('now', ?1)",
        [format!("-{EVENT_RETENTION_DAYS} days")],
    )
    .map_err(|e| AppError::message(format!("prune usage_events: {e}")))?;
    conn.execute(
        "DELETE FROM usage_daily WHERE day < date('now', ?1)",
        [format!("-{DAILY_RETENTION_DAYS} days")],
    )
    .map_err(|e| AppError::message(format!("prune usage_daily: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_usage_creates_schema() {
        let dir = std::env::temp_dir().join(format!(
            "agentero-usage-schema-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let db = dir.join("usage.sqlite");
        let conn = ensure_usage_at(&db).expect("ensure");
        assert_eq!(schema_version(&conn).unwrap(), SCHEMA_VERSION);
        let tables: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('usage_events','usage_daily')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(tables, 2);
        drop(conn);
        let conn2 = ensure_usage_at(&db).expect("reopen");
        assert_eq!(schema_version(&conn2).unwrap(), SCHEMA_VERSION);
        let _ = fs::remove_dir_all(&dir);
    }
}

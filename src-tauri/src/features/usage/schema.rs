//! usage.sqlite schema and open/migrate helpers.
//!
//! v1: events + daily keyed by raw path.
//! v2: vault identity, paper_path / facet / qty / status, daily rollup by
//!     (paper, facet), reserved memories table.

use crate::core::error::AppError;
use crate::core::paths;
use rusqlite::Connection;
use std::fs;
use std::path::Path;

/// Current usage schema version written to `schema_meta`.
pub const SCHEMA_VERSION: i32 = 2;

/// Raw events older than this are pruned on open.
pub const EVENT_RETENTION_DAYS: i32 = 180;

/// Daily aggregates older than this are pruned on open.
pub const DAILY_RETENTION_DAYS: i32 = 730;

const DDL_V2: &str = r#"
CREATE TABLE IF NOT EXISTS schema_meta (
    key   TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS usage_vaults (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    path       TEXT    NOT NULL UNIQUE,
    created_at TEXT    NOT NULL,
    last_seen  TEXT    NOT NULL
);

CREATE TABLE IF NOT EXISTS usage_events (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    ts         TEXT    NOT NULL,
    vault      TEXT,
    kind       TEXT    NOT NULL,
    path       TEXT,
    paper_path TEXT,
    mode       TEXT,
    facet      TEXT,
    status     TEXT,
    dur_ms     INTEGER,
    qty        INTEGER,
    extra      TEXT
);
CREATE INDEX IF NOT EXISTS idx_usage_events_ts     ON usage_events(ts);
CREATE INDEX IF NOT EXISTS idx_usage_events_vault  ON usage_events(vault, ts);
CREATE INDEX IF NOT EXISTS idx_usage_events_kind   ON usage_events(kind, ts);
CREATE INDEX IF NOT EXISTS idx_usage_events_path   ON usage_events(path, ts);
CREATE INDEX IF NOT EXISTS idx_usage_events_paper  ON usage_events(paper_path, ts);
CREATE INDEX IF NOT EXISTS idx_usage_events_facet  ON usage_events(kind, facet, ts);

CREATE TABLE IF NOT EXISTS usage_daily (
    day        TEXT    NOT NULL,
    vault      TEXT    NOT NULL DEFAULT '',
    kind       TEXT    NOT NULL,
    paper_path TEXT    NOT NULL DEFAULT '',
    facet      TEXT    NOT NULL DEFAULT '',
    count      INTEGER NOT NULL DEFAULT 0,
    dur_ms     INTEGER NOT NULL DEFAULT 0,
    qty        INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (day, vault, kind, paper_path, facet)
);

CREATE TABLE IF NOT EXISTS usage_memories (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    vault      TEXT,
    text       TEXT    NOT NULL,
    source     TEXT    NOT NULL DEFAULT 'user',
    enabled    INTEGER NOT NULL DEFAULT 1,
    created_at TEXT    NOT NULL,
    updated_at TEXT    NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_usage_memories_vault ON usage_memories(vault, enabled);
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
        conn.execute_batch(DDL_V2)
            .map_err(|e| AppError::message(format!("usage migrate v2: {e}")))?;
        set_schema_version(conn, SCHEMA_VERSION)?;
        return Ok(());
    }
    if version < 2 {
        migrate_v1_to_v2(conn)?;
        set_schema_version(conn, 2)?;
    }
    Ok(())
}

fn migrate_v1_to_v2(conn: &Connection) -> Result<(), AppError> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS usage_vaults (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            path       TEXT    NOT NULL UNIQUE,
            created_at TEXT    NOT NULL,
            last_seen  TEXT    NOT NULL
        );
        CREATE TABLE IF NOT EXISTS usage_memories (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            vault      TEXT,
            text       TEXT    NOT NULL,
            source     TEXT    NOT NULL DEFAULT 'user',
            enabled    INTEGER NOT NULL DEFAULT 1,
            created_at TEXT    NOT NULL,
            updated_at TEXT    NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_usage_memories_vault ON usage_memories(vault, enabled);
        "#,
    )
    .map_err(|e| AppError::message(format!("usage v2 tables: {e}")))?;

    for col in [
        "ALTER TABLE usage_events ADD COLUMN paper_path TEXT",
        "ALTER TABLE usage_events ADD COLUMN facet TEXT",
        "ALTER TABLE usage_events ADD COLUMN status TEXT",
        "ALTER TABLE usage_events ADD COLUMN qty INTEGER",
    ] {
        match conn.execute_batch(&format!("{col};")) {
            Ok(()) => {}
            Err(e) if e.to_string().contains("duplicate column name") => {}
            Err(e) => return Err(AppError::message(format!("usage v2 columns: {e}"))),
        }
    }

    conn.execute_batch(
        r#"
        CREATE INDEX IF NOT EXISTS idx_usage_events_paper ON usage_events(paper_path, ts);
        CREATE INDEX IF NOT EXISTS idx_usage_events_facet ON usage_events(kind, facet, ts);

        UPDATE usage_events
        SET paper_path = CASE
            WHEN path IS NULL OR path NOT LIKE 'papers/%' THEN NULL
            WHEN instr(substr(path, 8), '/') = 0 THEN path
            ELSE substr(path, 1, 6 + instr(substr(path, 8), '/'))
        END
        WHERE paper_path IS NULL AND path IS NOT NULL;

        INSERT OR IGNORE INTO usage_vaults (path, created_at, last_seen)
        SELECT vault, MIN(ts), MAX(ts)
        FROM usage_events
        WHERE vault IS NOT NULL AND trim(vault) != ''
        GROUP BY vault;
        "#,
    )
    .map_err(|e| AppError::message(format!("usage v2 backfill: {e}")))?;

    conn.execute_batch(
        r#"
        CREATE TABLE usage_daily_v2 (
            day        TEXT    NOT NULL,
            vault      TEXT    NOT NULL DEFAULT '',
            kind       TEXT    NOT NULL,
            paper_path TEXT    NOT NULL DEFAULT '',
            facet      TEXT    NOT NULL DEFAULT '',
            count      INTEGER NOT NULL DEFAULT 0,
            dur_ms     INTEGER NOT NULL DEFAULT 0,
            qty        INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (day, vault, kind, paper_path, facet)
        );
        INSERT INTO usage_daily_v2 (day, vault, kind, paper_path, facet, count, dur_ms, qty)
        SELECT
            day,
            vault,
            kind,
            CASE
                WHEN path NOT LIKE 'papers/%' THEN COALESCE(path, '')
                WHEN instr(substr(path, 8), '/') = 0 THEN path
                ELSE substr(path, 1, 6 + instr(substr(path, 8), '/'))
            END,
            '',
            SUM(count),
            SUM(dur_ms),
            0
        FROM usage_daily
        GROUP BY 1, 2, 3, 4, 5;
        DROP TABLE usage_daily;
        ALTER TABLE usage_daily_v2 RENAME TO usage_daily;
        "#,
    )
    .map_err(|e| AppError::message(format!("usage v2 daily: {e}")))?;
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

/// `papers/<id>/…` → `papers/<id>`; anything else stays `None`.
// TODO(usage v2): drop this once the paper-level rollup recorder calls it.
#[allow(dead_code)]
pub fn paper_path_of(path: &str) -> Option<String> {
    let path = path.trim().replace('\\', "/");
    let path = path.trim_matches('/');
    let rest = path.strip_prefix("papers/")?;
    let id = rest.split('/').next().filter(|s| !s.is_empty())?;
    Some(format!("papers/{id}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "agentero-usage-schema-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn paper_path_of_extracts_paper_folder() {
        assert_eq!(
            paper_path_of("papers/1706.03762/NOTES.md").as_deref(),
            Some("papers/1706.03762")
        );
        assert_eq!(
            paper_path_of("papers/1706.03762").as_deref(),
            Some("papers/1706.03762")
        );
        assert_eq!(paper_path_of("notes/weekly.md"), None);
        assert_eq!(paper_path_of(""), None);
    }

    #[test]
    fn ensure_usage_creates_v2_schema() {
        let dir = temp_dir();
        let db = dir.join("usage.sqlite");
        let conn = ensure_usage_at(&db).expect("ensure");
        assert_eq!(schema_version(&conn).unwrap(), SCHEMA_VERSION);
        let tables: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table'
                 AND name IN ('usage_events','usage_daily','usage_vaults','usage_memories')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(tables, 4);
        let has_facet: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('usage_events') WHERE name = 'facet'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(has_facet, 1);
        drop(conn);
        let conn2 = ensure_usage_at(&db).expect("reopen");
        assert_eq!(schema_version(&conn2).unwrap(), SCHEMA_VERSION);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn migrates_v1_rows() {
        let dir = temp_dir();
        let db = dir.join("usage.sqlite");
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE schema_meta (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL);
            INSERT INTO schema_meta(key, value) VALUES('schema_version', '1');
            CREATE TABLE usage_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                ts TEXT NOT NULL,
                vault TEXT,
                kind TEXT NOT NULL,
                path TEXT,
                mode TEXT,
                dur_ms INTEGER,
                extra TEXT
            );
            CREATE TABLE usage_daily (
                day TEXT NOT NULL,
                vault TEXT NOT NULL DEFAULT '',
                kind TEXT NOT NULL,
                path TEXT NOT NULL DEFAULT '',
                count INTEGER NOT NULL DEFAULT 0,
                dur_ms INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (day, vault, kind, path)
            );
            INSERT INTO usage_events (ts, vault, kind, path, mode, dur_ms, extra)
            VALUES (
                '2026-08-01T10:00:00.000Z',
                '/vaults/demo',
                'note.open',
                'papers/abc/NOTES.md',
                'markdown',
                100,
                '{"source":"arxiv"}'
            );
            INSERT INTO usage_daily (day, vault, kind, path, count, dur_ms)
            VALUES ('2026-08-01', '/vaults/demo', 'note.open', 'papers/abc/NOTES.md', 1, 100);
            "#,
        )
        .unwrap();
        drop(conn);

        let conn = ensure_usage_at(&db).expect("migrate");
        assert_eq!(schema_version(&conn).unwrap(), 2);
        let paper: String = conn
            .query_row(
                "SELECT paper_path FROM usage_events WHERE path = 'papers/abc/NOTES.md'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(paper, "papers/abc");
        let daily_paper: String = conn
            .query_row(
                "SELECT paper_path FROM usage_daily WHERE kind = 'note.open'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(daily_paper, "papers/abc");
        let vaults: i64 = conn
            .query_row("SELECT COUNT(*) FROM usage_vaults", [], |r| r.get(0))
            .unwrap();
        assert_eq!(vaults, 1);
        let _ = fs::remove_dir_all(&dir);
    }
}

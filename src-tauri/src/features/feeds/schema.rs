//! XDG `feeds.sqlite`: subscriptions + cached items.

use crate::core::error::AppError;
use crate::core::paths;
use rusqlite::Connection;
use std::fs;
use std::path::Path;

pub const SCHEMA_VERSION: i32 = 2;
pub const ITEMS_PER_FEED: i64 = 200;

const DDL_V1: &str = r#"
CREATE TABLE IF NOT EXISTS schema_meta (
  key   TEXT PRIMARY KEY NOT NULL,
  value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS subscriptions (
  id              TEXT PRIMARY KEY,
  url             TEXT NOT NULL UNIQUE,
  title           TEXT NOT NULL,
  added_at        TEXT NOT NULL,
  last_fetched_at TEXT,
  last_error      TEXT,
  etag            TEXT,
  last_modified   TEXT
);

CREATE TABLE IF NOT EXISTS items (
  id               TEXT PRIMARY KEY,
  subscription_id  TEXT NOT NULL REFERENCES subscriptions(id) ON DELETE CASCADE,
  guid             TEXT NOT NULL,
  title            TEXT NOT NULL,
  url              TEXT,
  published_at     TEXT,
  summary_text     TEXT,
  content_html     TEXT,
  paper_url        TEXT,
  imported_at      TEXT,
  first_seen_at    TEXT NOT NULL,
  UNIQUE (subscription_id, guid)
);

CREATE INDEX IF NOT EXISTS items_timeline
  ON items (published_at DESC, first_seen_at DESC);
"#;

const MIGRATE_V1_TO_V2: &str = r#"
ALTER TABLE subscriptions ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0;
ALTER TABLE subscriptions ADD COLUMN pinned_at TEXT;
ALTER TABLE items ADD COLUMN body_markdown TEXT;
"#;

pub fn feeds_db_path() -> std::path::PathBuf {
    paths::feeds_db_path()
}

pub fn ensure_feeds() -> Result<Connection, AppError> {
    ensure_feeds_at(&feeds_db_path())
}

pub fn ensure_feeds_at(db_path: &Path) -> Result<Connection, AppError> {
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(db_path)
        .map_err(|e| AppError::message(format!("open feeds {}: {e}", db_path.display())))?;
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;\n\
         PRAGMA synchronous = NORMAL;\n\
         PRAGMA busy_timeout = 5000;\n\
         PRAGMA foreign_keys = ON;",
    )
    .map_err(|e| AppError::message(format!("feeds pragma: {e}")))?;
    migrate(&conn)?;
    Ok(conn)
}

fn migrate(conn: &Connection) -> Result<(), AppError> {
    let version = schema_version(conn).unwrap_or(0);
    if version > SCHEMA_VERSION {
        return Err(AppError::message(format!(
            "feeds schema version {version} is newer than this app supports ({SCHEMA_VERSION})"
        )));
    }
    if version < 1 {
        conn.execute_batch(DDL_V1)
            .map_err(|e| AppError::message(format!("feeds migrate v1: {e}")))?;
        set_schema_version(conn, 1)?;
    }
    let version = schema_version(conn).unwrap_or(0);
    if version < 2 {
        for stmt in MIGRATE_V1_TO_V2.split(';') {
            let s = stmt.trim();
            if s.is_empty() {
                continue;
            }
            match conn.execute_batch(&format!("{s};")) {
                Ok(()) => {}
                Err(e) => {
                    let msg = e.to_string();
                    if msg.contains("duplicate column name") {
                        continue;
                    }
                    return Err(AppError::message(format!("feeds migrate v2: {e}")));
                }
            }
        }
        set_schema_version(conn, 2)?;
    }
    Ok(())
}

fn schema_version(conn: &Connection) -> Result<i32, AppError> {
    conn.query_row(
        "SELECT value FROM schema_meta WHERE key = 'schema_version'",
        [],
        |row| {
            let raw: String = row.get(0)?;
            Ok(raw.parse::<i32>().unwrap_or(0))
        },
    )
    .map_err(|e| AppError::message(format!("feeds schema_version: {e}")))
}

fn set_schema_version(conn: &Connection, version: i32) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO schema_meta(key, value) VALUES('schema_version', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [version.to_string()],
    )
    .map_err(|e| AppError::message(format!("write feeds schema_version: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_schema() {
        let dir = std::env::temp_dir().join(format!(
            "agentero-feeds-schema-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let db = dir.join("feeds.sqlite");
        let conn = ensure_feeds_at(&db).expect("ensure");
        assert_eq!(schema_version(&conn).unwrap(), SCHEMA_VERSION);
        drop(conn);
        let conn2 = ensure_feeds_at(&db).expect("reopen");
        assert_eq!(schema_version(&conn2).unwrap(), SCHEMA_VERSION);
        conn2
            .execute_batch(
                "SELECT pinned, pinned_at FROM subscriptions; SELECT body_markdown FROM items;",
            )
            .expect("v2 columns");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn migrates_v1_to_v2() {
        let dir = std::env::temp_dir().join(format!(
            "agentero-feeds-schema-v1-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let db = dir.join("feeds.sqlite");
        {
            let conn = Connection::open(&db).unwrap();
            conn.execute_batch(DDL_V1).unwrap();
            conn.execute(
                "INSERT INTO schema_meta(key, value) VALUES('schema_version', '1')",
                [],
            )
            .unwrap();
        }
        let conn = ensure_feeds_at(&db).expect("migrate");
        assert_eq!(schema_version(&conn).unwrap(), 2);
        conn.execute(
            "INSERT INTO subscriptions (id, url, title, added_at, pinned) VALUES ('a', 'https://ex.com/f', 't', 'now', 1)",
            [],
        )
        .unwrap();
        let pinned: i64 = conn
            .query_row("SELECT pinned FROM subscriptions WHERE id = 'a'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(pinned, 1);
        let _ = fs::remove_dir_all(&dir);
    }
}

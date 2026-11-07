//! SQLite state: tracked shows, seen releases, and audit events.

use crate::config;
use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

const SCHEMA: &str = r"
CREATE TABLE IF NOT EXISTS shows (
    id            INTEGER PRIMARY KEY,
    title         TEXT NOT NULL UNIQUE,
    match_title   TEXT NOT NULL,
    season        INTEGER NOT NULL DEFAULT 1,
    max_episode   REAL NOT NULL DEFAULT 0,
    enabled       INTEGER NOT NULL DEFAULT 1,
    added_at      TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE IF NOT EXISTS releases (
    infohash      TEXT PRIMARY KEY,
    show_id       INTEGER NOT NULL REFERENCES shows(id),
    season        INTEGER NOT NULL,
    episode       REAL NOT NULL,
    title         TEXT NOT NULL,
    bytes         INTEGER NOT NULL,
    grabbed_at    TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(show_id, season, episode)
);
CREATE TABLE IF NOT EXISTS state (
    key           TEXT PRIMARY KEY,
    value         TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS events (
    id            INTEGER PRIMARY KEY,
    at            TEXT NOT NULL DEFAULT (datetime('now')),
    level         TEXT NOT NULL,
    message       TEXT NOT NULL
);
";

/// A show the user follows.
#[derive(Debug, Clone)]
pub struct Show {
    /// Database identifier.
    pub id: i64,
    /// Title shown to the user.
    pub title: String,
    /// Normalized title used for feed matching.
    pub match_title: String,
    /// Season watermark.
    pub season: u32,
    /// Highest episode already accounted for in `season`.
    pub max_episode: f64,
    /// Whether polling is active.
    pub enabled: bool,
}

/// An audit row from the events table.
#[derive(Debug, Clone)]
pub struct Event {
    /// Timestamp.
    pub at: String,
    /// Severity label.
    pub level: String,
    /// Message text.
    pub message: String,
}

/// A release record that was grabbed and saved to disk.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GrabbedRelease {
    pub infohash: String,
    pub show_id: i64,
    pub show_title: String,
    pub season: u32,
    pub episode: f64,
    pub title: String,
    pub bytes: u64,
    pub grabbed_at: String,
}

/// Open handle to the state database.
pub struct Db {
    conn: Connection,
}

impl Db {
    /// Opens (and creates) the database under the state directory.
    ///
    /// # Errors
    /// Returns an error if the directory or schema cannot be prepared.
    pub fn open() -> Result<Self> {
        let dir = config::state_dir();
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create {}", dir.display()))?;
        Self::open_at(&dir.join("nagare.db"))
    }

    /// Opens a database at an explicit path, used by tests.
    ///
    /// # Errors
    /// Returns an error if the connection or schema setup fails.
    pub fn open_at(path: &Path) -> Result<Self> {
        let conn = Connection::open(path).context("failed to open sqlite database")?;
        conn.execute_batch(SCHEMA)
            .context("failed to apply schema")?;
        Ok(Self { conn })
    }

    /// Inserts a show, or keeps the existing watermark when the title is known.
    ///
    /// # Errors
    /// Returns an error if the insert or lookup fails.
    pub fn add_show(
        &self,
        title: &str,
        match_title: &str,
        season: u32,
        max_episode: f64,
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO shows (title, match_title, season, max_episode) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(title) DO UPDATE SET match_title = excluded.match_title",
            params![title, match_title, season, max_episode],
        )?;
        let id = self.conn.query_row(
            "SELECT id FROM shows WHERE title = ?1",
            params![title],
            |r| r.get(0),
        )?;
        Ok(id)
    }

    /// Lists tracked shows.
    ///
    /// # Errors
    /// Returns an error if the query fails.
    pub fn shows(&self) -> Result<Vec<Show>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, match_title, season, max_episode, enabled
             FROM shows ORDER BY title",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Show {
                id: r.get(0)?,
                title: r.get(1)?,
                match_title: r.get(2)?,
                season: r.get(3)?,
                max_episode: r.get(4)?,
                enabled: r.get::<_, i64>(5)? == 1,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Removes a show by ID. Returns true when a row was deleted.
    ///
    /// # Errors
    /// Returns an error if the delete fails.
    pub fn remove_show(&self, id: i64) -> Result<bool> {
        let changed = self
            .conn
            .execute("DELETE FROM shows WHERE id = ?1", params![id])?;
        Ok(changed > 0)
    }

    /// Checks whether an infohash was already handled.
    ///
    /// # Errors
    /// Returns an error if the query fails.
    pub fn has_release(&self, infohash: &str) -> Result<bool> {
        let found: Option<i64> = self
            .conn
            .query_row(
                "SELECT 1 FROM releases WHERE infohash = ?1",
                params![infohash],
                |r| r.get(0),
            )
            .optional()?;
        Ok(found.is_some())
    }

    /// Records a grabbed release and advances the show watermark.
    ///
    /// # Errors
    /// Returns an error if the transaction fails.
    pub fn record_release(
        &mut self,
        infohash: &str,
        show_id: i64,
        season: u32,
        episode: f64,
        title: &str,
        bytes: u64,
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT OR IGNORE INTO releases (infohash, show_id, season, episode, title, bytes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![infohash, show_id, season, episode, title, bytes as i64],
        )?;
        tx.execute(
            "UPDATE shows
             SET season = ?2, max_episode = ?3
             WHERE id = ?1
               AND (?2 > season OR (?2 = season AND ?3 > max_episode))",
            params![show_id, season, episode],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Lists grabbed releases joined with show metadata, newest first.
    ///
    /// # Errors
    /// Returns an error if the query fails.
    pub fn releases(&self, limit: usize) -> Result<Vec<GrabbedRelease>> {
        let mut stmt = self.conn.prepare(
            "SELECT r.infohash, r.show_id, s.title, r.season, r.episode, r.title, r.bytes, r.grabbed_at
             FROM releases r
             JOIN shows s ON s.id = r.show_id
             ORDER BY r.grabbed_at DESC, r.rowid DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |r| {
            Ok(GrabbedRelease {
                infohash: r.get(0)?,
                show_id: r.get(1)?,
                show_title: r.get(2)?,
                season: r.get(3)?,
                episode: r.get(4)?,
                title: r.get(5)?,
                bytes: r.get::<_, i64>(6)? as u64,
                grabbed_at: r.get(7)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Reads a free-form state value.
    ///
    /// # Errors
    /// Returns an error if the query fails.
    pub fn get_state(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT value FROM state WHERE key = ?1",
                params![key],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// Writes a free-form state value.
    ///
    /// # Errors
    /// Returns an error if the write fails.
    pub fn set_state(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO state (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    /// Appends an audit event.
    ///
    /// # Errors
    /// Returns an error if the insert fails.
    pub fn log_event(&self, level: &str, message: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO events (level, message) VALUES (?1, ?2)",
            params![level, message],
        )?;
        Ok(())
    }

    /// Returns the most recent events, newest first.
    ///
    /// # Errors
    /// Returns an error if the query fails.
    pub fn recent_events(&self, limit: usize) -> Result<Vec<Event>> {
        let mut stmt = self
            .conn
            .prepare("SELECT at, level, message FROM events ORDER BY id DESC LIMIT ?1")?;
        let rows = stmt.query_map(params![limit as i64], |r| {
            Ok(Event {
                at: r.get(0)?,
                level: r.get(1)?,
                message: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn mem() -> Db {
        Db::open_at(Path::new(":memory:")).expect("memory db")
    }

    #[test]
    fn add_and_list_show() {
        let db = mem();
        db.add_show("Some Show", "some show", 1, 4.0).expect("add");
        db.add_show("Some Show", "some show", 2, 9.0)
            .expect("idempotent");
        let shows = db.shows().expect("list");
        assert_eq!(shows.len(), 1);
        assert_eq!(shows[0].max_episode, 4.0);
        assert_eq!(shows[0].season, 1);
    }

    #[test]
    fn record_release_advances_watermark() {
        let mut db = mem();
        let id = db.add_show("Show", "show", 2, 1.0).expect("add");
        db.record_release("HASH", id, 2, 5.0, "Show - 05", 100)
            .expect("record");
        assert!(db.has_release("HASH").expect("check"));
        let shows = db.shows().expect("list");
        assert_eq!(shows[0].season, 2);
        assert_eq!(shows[0].max_episode, 5.0);
    }

    #[test]
    fn remove_missing_show_returns_false() {
        let db = mem();
        assert!(!db.remove_show(999).expect("remove"));
    }

    #[test]
    fn state_roundtrip() {
        let db = mem();
        db.set_state("etag", "abc").expect("set");
        assert_eq!(db.get_state("etag").expect("get").as_deref(), Some("abc"));
    }

    #[test]
    fn list_releases_newest_first() {
        let mut db = mem();
        let id = db.add_show("Show", "show", 1, 0.0).expect("add");
        db.record_release("H1", id, 1, 1.0, "Show - 01", 100).expect("rec1");
        db.record_release("H2", id, 1, 2.0, "Show - 02", 200).expect("rec2");
        let rels = db.releases(10).expect("rels");
        assert_eq!(rels.len(), 2);
        assert_eq!(rels[0].episode, 2.0);
        assert_eq!(rels[0].show_title, "Show");
    }
}

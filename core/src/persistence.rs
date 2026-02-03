use rusqlite::{Connection, params};
use std::path::Path;

use crate::types::Memory;

pub struct Database {
    conn: Connection,
}

impl Database {
    /// Open or create a SQLite database at the given path.
    /// Uses WAL mode for concurrent read performance.
    pub fn open(path: &Path) -> Result<Self, rusqlite::Error> {
        let conn = Connection::open(path)?;
        let db = Self { conn };
        db.init()?;
        db.migrate()?;
        Ok(db)
    }

    /// Create an in-memory database (for testing).
    pub fn in_memory() -> Result<Self, rusqlite::Error> {
        let conn = Connection::open_in_memory()?;
        let db = Self { conn };
        db.init()?;
        Ok(db)
    }

    fn init(&self) -> Result<(), rusqlite::Error> {
        self.conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        self.conn.execute_batch("PRAGMA busy_timeout=5000;")?;

        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS temporal_index (
                commit_hash      TEXT NOT NULL,
                file_path        TEXT NOT NULL,
                commit_timestamp INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (commit_hash, file_path)
            );

            CREATE INDEX IF NOT EXISTS idx_temporal_file
                ON temporal_index(file_path);

            CREATE TABLE IF NOT EXISTS watermark (
                id          INTEGER PRIMARY KEY CHECK (id = 1),
                last_commit TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS memories (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                file_path   TEXT NOT NULL,
                symbol_name TEXT,
                content     TEXT NOT NULL,
                created_at  DATETIME DEFAULT CURRENT_TIMESTAMP
            );

            CREATE INDEX IF NOT EXISTS idx_memories_file
                ON memories(file_path);",
        )?;
        Ok(())
    }

    /// Migrate existing databases that lack the commit_timestamp column.
    fn migrate(&self) -> Result<(), rusqlite::Error> {
        let has_timestamp = {
            let mut stmt = self.conn.prepare("PRAGMA table_info(temporal_index)")?;
            let cols = stmt.query_map([], |row| row.get::<_, String>(1))?;
            let mut found = false;
            for col in cols {
                if col? == "commit_timestamp" {
                    found = true;
                    break;
                }
            }
            found
        };

        if !has_timestamp {
            self.conn.execute_batch(
                "ALTER TABLE temporal_index ADD COLUMN commit_timestamp INTEGER NOT NULL DEFAULT 0;"
            )?;
            // Clear watermark to force re-index so timestamps get populated
            self.conn.execute_batch("DELETE FROM watermark;")?;
        }

        Ok(())
    }

    /// Begin an explicit transaction for batch inserts.
    pub fn begin_transaction(&self) -> Result<(), rusqlite::Error> {
        self.conn.execute_batch("BEGIN")?;
        Ok(())
    }

    /// Commit the current transaction.
    pub fn commit_transaction(&self) -> Result<(), rusqlite::Error> {
        self.conn.execute_batch("COMMIT")?;
        Ok(())
    }

    /// Insert files changed in a single commit.
    pub fn insert_commit(
        &self,
        commit_hash: &str,
        files: &[&str],
        timestamp: i64,
    ) -> Result<(), rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "INSERT OR IGNORE INTO temporal_index (commit_hash, file_path, commit_timestamp)
             VALUES (?1, ?2, ?3)",
        )?;
        for file in files {
            stmt.execute(params![commit_hash, file, timestamp])?;
        }
        Ok(())
    }

    /// Get the co-change count between two files: how many commits contain both.
    pub fn co_change_count(&self, file_a: &str, file_b: &str) -> Result<u32, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT COUNT(DISTINCT a.commit_hash)
             FROM temporal_index a
             JOIN temporal_index b ON a.commit_hash = b.commit_hash
             WHERE a.file_path = ?1 AND b.file_path = ?2",
        )?;
        let count: u32 = stmt.query_row(params![file_a, file_b], |row| row.get(0))?;
        Ok(count)
    }

    /// Get all files that were ever committed alongside the given file,
    /// along with their co-change counts.
    pub fn coupled_files(&self, file_path: &str) -> Result<Vec<(String, u32)>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT b.file_path, COUNT(DISTINCT a.commit_hash) as cnt
             FROM temporal_index a
             JOIN temporal_index b ON a.commit_hash = b.commit_hash
             WHERE a.file_path = ?1 AND b.file_path != ?1
             GROUP BY b.file_path
             ORDER BY cnt DESC",
        )?;

        let rows = stmt.query_map(params![file_path], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Get all files coupled with the given file, along with stats needed for risk scoring:
    /// (path, co_change_count, total_commits_for_coupled_file, max_commit_timestamp)
    pub fn coupled_files_with_stats(
        &self,
        file_path: &str,
    ) -> Result<Vec<(String, u32, u32, i64)>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT
                b.file_path,
                COUNT(DISTINCT a.commit_hash) as co_change_count,
                (SELECT COUNT(DISTINCT commit_hash)
                 FROM temporal_index
                 WHERE file_path = b.file_path) as total_commits,
                MAX(b.commit_timestamp) as last_timestamp
             FROM temporal_index a
             JOIN temporal_index b ON a.commit_hash = b.commit_hash
             WHERE a.file_path = ?1 AND b.file_path != ?1
             GROUP BY b.file_path
             ORDER BY co_change_count DESC",
        )?;

        let rows = stmt.query_map(params![file_path], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, u32>(1)?,
                row.get::<_, u32>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Get the oldest and newest commit timestamps in the database.
    /// Returns (oldest_ts, newest_ts). If no data, returns (0, 0).
    pub fn commit_time_range(&self) -> Result<(i64, i64), rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT COALESCE(MIN(commit_timestamp), 0), COALESCE(MAX(commit_timestamp), 0)
             FROM temporal_index",
        )?;
        let (oldest, newest) = stmt.query_row([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        })?;
        Ok((oldest, newest))
    }

    /// Get the number of commits that touch the given file.
    pub fn commit_count(&self, file_path: &str) -> Result<u32, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT COUNT(DISTINCT commit_hash) FROM temporal_index WHERE file_path = ?1",
        )?;
        let count: u32 = stmt.query_row(params![file_path], |row| row.get(0))?;
        Ok(count)
    }

    /// Get the watermark (last indexed commit SHA).
    pub fn get_watermark(&self) -> Result<Option<String>, rusqlite::Error> {
        let mut stmt = self
            .conn
            .prepare("SELECT last_commit FROM watermark WHERE id = 1")?;
        let result = stmt.query_row([], |row| row.get::<_, String>(0));
        match result {
            Ok(hash) => Ok(Some(hash)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Set the watermark (last indexed commit SHA).
    pub fn set_watermark(&self, commit_hash: &str) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "INSERT OR REPLACE INTO watermark (id, last_commit) VALUES (1, ?1)",
            params![commit_hash],
        )?;
        Ok(())
    }

    /// Add a memory (note) for a file, optionally scoped to a symbol.
    pub fn add_memory(
        &self,
        file_path: &str,
        symbol_name: Option<&str>,
        content: &str,
    ) -> Result<i64, rusqlite::Error> {
        self.conn.execute(
            "INSERT INTO memories (file_path, symbol_name, content) VALUES (?1, ?2, ?3)",
            params![file_path, symbol_name, content],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Get all memories for a specific file.
    pub fn memories_for_file(&self, file_path: &str) -> Result<Vec<Memory>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT id, file_path, symbol_name, content, created_at
             FROM memories WHERE file_path = ?1 ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map(params![file_path], |row| {
            Ok(Memory {
                id: row.get(0)?,
                file_path: row.get(1)?,
                symbol_name: row.get(2)?,
                content: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;
        rows.collect()
    }

    /// Search memories by content or file path substring.
    pub fn search_memories(&self, query: &str) -> Result<Vec<Memory>, rusqlite::Error> {
        let pattern = format!("%{query}%");
        let mut stmt = self.conn.prepare(
            "SELECT id, file_path, symbol_name, content, created_at
             FROM memories
             WHERE content LIKE ?1 OR file_path LIKE ?1
             ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map(params![pattern], |row| {
            Ok(Memory {
                id: row.get(0)?,
                file_path: row.get(1)?,
                symbol_name: row.get(2)?,
                content: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;
        rows.collect()
    }

    /// List all memories, optionally filtered by file path.
    pub fn list_memories(&self, file_path: Option<&str>) -> Result<Vec<Memory>, rusqlite::Error> {
        match file_path {
            Some(path) => self.memories_for_file(path),
            None => {
                let mut stmt = self.conn.prepare(
                    "SELECT id, file_path, symbol_name, content, created_at
                     FROM memories ORDER BY created_at DESC",
                )?;
                let rows = stmt.query_map([], |row| {
                    Ok(Memory {
                        id: row.get(0)?,
                        file_path: row.get(1)?,
                        symbol_name: row.get(2)?,
                        content: row.get(3)?,
                        created_at: row.get(4)?,
                    })
                })?;
                rows.collect()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_query_cochange() {
        let db = Database::in_memory().unwrap();

        db.insert_commit("abc123", &["src/A.ts", "src/B.ts"], 1000)
            .unwrap();
        db.insert_commit("def456", &["src/A.ts", "src/B.ts"], 2000)
            .unwrap();
        db.insert_commit("ghi789", &["src/A.ts", "src/C.ts"], 3000)
            .unwrap();

        assert_eq!(db.co_change_count("src/A.ts", "src/B.ts").unwrap(), 2);
        assert_eq!(db.co_change_count("src/A.ts", "src/C.ts").unwrap(), 1);
        assert_eq!(db.co_change_count("src/B.ts", "src/C.ts").unwrap(), 0);
    }

    #[test]
    fn test_coupled_files() {
        let db = Database::in_memory().unwrap();

        for i in 0..10 {
            db.insert_commit(&format!("commit_{i}"), &["src/A.ts", "src/B.ts"], 1000 + i)
                .unwrap();
        }
        db.insert_commit("single", &["src/A.ts", "src/C.ts"], 2000)
            .unwrap();

        let coupled = db.coupled_files("src/A.ts").unwrap();
        assert_eq!(coupled.len(), 2);
        assert_eq!(coupled[0].0, "src/B.ts");
        assert_eq!(coupled[0].1, 10);
        assert_eq!(coupled[1].0, "src/C.ts");
        assert_eq!(coupled[1].1, 1);
    }

    #[test]
    fn test_commit_count() {
        let db = Database::in_memory().unwrap();
        db.insert_commit("a", &["x.ts"], 100).unwrap();
        db.insert_commit("b", &["x.ts"], 200).unwrap();
        db.insert_commit("c", &["y.ts"], 300).unwrap();

        assert_eq!(db.commit_count("x.ts").unwrap(), 2);
        assert_eq!(db.commit_count("y.ts").unwrap(), 1);
    }

    #[test]
    fn test_watermark() {
        let db = Database::in_memory().unwrap();

        assert_eq!(db.get_watermark().unwrap(), None);
        db.set_watermark("abc123").unwrap();
        assert_eq!(db.get_watermark().unwrap(), Some("abc123".to_string()));
        db.set_watermark("def456").unwrap();
        assert_eq!(db.get_watermark().unwrap(), Some("def456".to_string()));
    }

    #[test]
    fn test_duplicate_insert_ignored() {
        let db = Database::in_memory().unwrap();

        db.insert_commit("abc", &["a.ts", "b.ts"], 100).unwrap();
        db.insert_commit("abc", &["a.ts", "b.ts"], 100).unwrap(); // duplicate

        assert_eq!(db.co_change_count("a.ts", "b.ts").unwrap(), 1);
    }

    #[test]
    fn test_coupled_files_with_stats() {
        let db = Database::in_memory().unwrap();

        // File A committed with B 3 times, with C once
        db.insert_commit("c1", &["A.ts", "B.ts"], 1000).unwrap();
        db.insert_commit("c2", &["A.ts", "B.ts"], 2000).unwrap();
        db.insert_commit("c3", &["A.ts", "B.ts", "C.ts"], 3000).unwrap();
        // B also committed alone once
        db.insert_commit("c4", &["B.ts"], 4000).unwrap();

        let stats = db.coupled_files_with_stats("A.ts").unwrap();
        assert_eq!(stats.len(), 2);

        // B: co_change=3, total_commits=4, last_timestamp=3000 (from co-commits with A)
        let (path, co_change, total, last_ts) = &stats[0];
        assert_eq!(path, "B.ts");
        assert_eq!(*co_change, 3);
        assert_eq!(*total, 4);
        assert_eq!(*last_ts, 3000);

        // C: co_change=1, total_commits=1, last_timestamp=3000
        let (path, co_change, total, last_ts) = &stats[1];
        assert_eq!(path, "C.ts");
        assert_eq!(*co_change, 1);
        assert_eq!(*total, 1);
        assert_eq!(*last_ts, 3000);
    }

    #[test]
    fn test_commit_time_range() {
        let db = Database::in_memory().unwrap();

        // Empty database
        let (oldest, newest) = db.commit_time_range().unwrap();
        assert_eq!(oldest, 0);
        assert_eq!(newest, 0);

        db.insert_commit("c1", &["a.ts"], 1000).unwrap();
        db.insert_commit("c2", &["b.ts"], 5000).unwrap();
        db.insert_commit("c3", &["c.ts"], 3000).unwrap();

        let (oldest, newest) = db.commit_time_range().unwrap();
        assert_eq!(oldest, 1000);
        assert_eq!(newest, 5000);
    }

    #[test]
    fn test_add_and_retrieve_memory() {
        let db = Database::in_memory().unwrap();
        let id = db.add_memory("src/Auth.ts", None, "Auth handles JWT tokens").unwrap();
        assert!(id > 0);

        let memories = db.memories_for_file("src/Auth.ts").unwrap();
        assert_eq!(memories.len(), 1);
        assert_eq!(memories[0].content, "Auth handles JWT tokens");
        assert_eq!(memories[0].file_path, "src/Auth.ts");
        assert!(memories[0].symbol_name.is_none());
    }

    #[test]
    fn test_memory_with_symbol_name() {
        let db = Database::in_memory().unwrap();
        db.add_memory("src/Auth.ts", Some("validateToken"), "Must check expiry").unwrap();

        let memories = db.memories_for_file("src/Auth.ts").unwrap();
        assert_eq!(memories.len(), 1);
        assert_eq!(memories[0].symbol_name, Some("validateToken".to_string()));
    }

    #[test]
    fn test_search_memories_by_content() {
        let db = Database::in_memory().unwrap();
        db.add_memory("src/Auth.ts", None, "Uses JWT for authentication").unwrap();
        db.add_memory("src/Session.ts", None, "Session persistence layer").unwrap();

        let results = db.search_memories("JWT").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].file_path, "src/Auth.ts");
    }

    #[test]
    fn test_search_memories_by_path() {
        let db = Database::in_memory().unwrap();
        db.add_memory("src/Auth.ts", None, "Handles login").unwrap();
        db.add_memory("src/Session.ts", None, "Handles sessions").unwrap();

        let results = db.search_memories("Auth").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].file_path, "src/Auth.ts");
    }

    #[test]
    fn test_list_all_memories() {
        let db = Database::in_memory().unwrap();
        db.add_memory("src/A.ts", None, "Note A").unwrap();
        db.add_memory("src/B.ts", None, "Note B").unwrap();

        let all = db.list_memories(None).unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_list_memories_filtered() {
        let db = Database::in_memory().unwrap();
        db.add_memory("src/A.ts", None, "Note A").unwrap();
        db.add_memory("src/B.ts", None, "Note B").unwrap();

        let filtered = db.list_memories(Some("src/A.ts")).unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].content, "Note A");
    }

    #[test]
    fn test_batch_transaction_inserts() {
        let db = Database::in_memory().unwrap();

        db.begin_transaction().unwrap();
        for i in 0..100 {
            db.insert_commit(&format!("c{i}"), &["batch.ts"], i as i64 * 100).unwrap();
        }
        db.commit_transaction().unwrap();

        let count = db.commit_count("batch.ts").unwrap();
        assert_eq!(count, 100, "all 100 commits should be present after commit");
    }

    #[test]
    fn test_empty_memory_result() {
        let db = Database::in_memory().unwrap();
        let memories = db.memories_for_file("src/NoExist.ts").unwrap();
        assert!(memories.is_empty());

        let search = db.search_memories("nothing").unwrap();
        assert!(search.is_empty());
    }
}

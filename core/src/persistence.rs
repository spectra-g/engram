use rusqlite::{Connection, params};
use std::path::Path;

use crate::types::Memory;

/// Persisted state for the adaptive indexing engine.
/// Single-row table (id=1) tracking progress across process restarts.
#[derive(Debug, Clone)]
pub struct IndexingState {
    pub head_commit: String,
    pub resume_oid: Option<String>,
    pub commits_indexed: u32,
    pub strategy: String,
    pub is_complete: bool,
    pub last_updated: i64,
    /// The file being analyzed (for PathFiltered strategy).
    /// Used to detect when a subsequent call targets a different file,
    /// requiring a fresh walk instead of resuming the old one.
    pub target_path: Option<String>,
}

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

            CREATE TABLE IF NOT EXISTS indexing_state (
                id               INTEGER PRIMARY KEY CHECK (id = 1),
                head_commit      TEXT NOT NULL,
                resume_oid       TEXT,
                commits_indexed  INTEGER NOT NULL DEFAULT 0,
                strategy         TEXT NOT NULL DEFAULT 'global',
                is_complete      INTEGER NOT NULL DEFAULT 0,
                last_updated     INTEGER NOT NULL DEFAULT 0,
                target_path      TEXT
            );

            CREATE TABLE IF NOT EXISTS memories (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                file_path   TEXT NOT NULL,
                symbol_name TEXT,
                content     TEXT NOT NULL,
                created_at  DATETIME DEFAULT CURRENT_TIMESTAMP
            );

            CREATE INDEX IF NOT EXISTS idx_memories_file
                ON memories(file_path);

            CREATE TABLE IF NOT EXISTS metrics_events (
                id                  INTEGER PRIMARY KEY AUTOINCREMENT,
                event_type          TEXT NOT NULL,
                timestamp           DATETIME DEFAULT CURRENT_TIMESTAMP,

                file_path           TEXT,
                coupled_files_count INTEGER DEFAULT 0,
                critical_count      INTEGER DEFAULT 0,
                high_count          INTEGER DEFAULT 0,
                medium_count        INTEGER DEFAULT 0,
                low_count           INTEGER DEFAULT 0,
                test_files_found    INTEGER DEFAULT 0,
                test_intents_total  INTEGER DEFAULT 0,
                commit_count        INTEGER DEFAULT 0,
                analysis_time_ms    INTEGER DEFAULT 0,

                note_id             INTEGER,

                repo_root           TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_metrics_event_type ON metrics_events(event_type);
            CREATE INDEX IF NOT EXISTS idx_metrics_timestamp ON metrics_events(timestamp);
            CREATE INDEX IF NOT EXISTS idx_metrics_repo ON metrics_events(repo_root);",
        )?;
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

    /// Get the current indexing state, if any.
    pub fn get_indexing_state(&self) -> Result<Option<IndexingState>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT head_commit, resume_oid, commits_indexed, strategy, is_complete, last_updated, target_path
             FROM indexing_state WHERE id = 1",
        )?;
        let result = stmt.query_row([], |row| {
            Ok(IndexingState {
                head_commit: row.get(0)?,
                resume_oid: row.get(1)?,
                commits_indexed: row.get(2)?,
                strategy: row.get(3)?,
                is_complete: row.get::<_, i32>(4)? != 0,
                last_updated: row.get(5)?,
                target_path: row.get(6)?,
            })
        });
        match result {
            Ok(state) => Ok(Some(state)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Insert or replace the indexing state.
    pub fn set_indexing_state(&self, state: &IndexingState) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "INSERT OR REPLACE INTO indexing_state
             (id, head_commit, resume_oid, commits_indexed, strategy, is_complete, last_updated, target_path)
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                state.head_commit,
                state.resume_oid,
                state.commits_indexed,
                state.strategy,
                state.is_complete as i32,
                state.last_updated,
                state.target_path,
            ],
        )?;
        Ok(())
    }

    /// Returns true if no indexing has been done yet (no indexing_state row).
    pub fn is_first_index_call(&self) -> Result<bool, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT COUNT(*) FROM indexing_state WHERE id = 1",
        )?;
        let count: i32 = stmt.query_row([], |row| row.get(0))?;
        Ok(count == 0)
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

    /// Insert a metrics event.
    #[allow(clippy::too_many_arguments)]
    pub fn insert_metrics_event(
        &self,
        event_type: &str,
        file_path: Option<&str>,
        coupled_files_count: u32,
        critical_count: u32,
        high_count: u32,
        medium_count: u32,
        low_count: u32,
        test_files_found: u32,
        test_intents_total: u32,
        commit_count: u32,
        analysis_time_ms: u64,
        note_id: Option<i64>,
        repo_root: &str,
    ) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "INSERT INTO metrics_events (
                event_type, file_path, coupled_files_count,
                critical_count, high_count, medium_count, low_count,
                test_files_found, test_intents_total, commit_count,
                analysis_time_ms, note_id, repo_root
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                event_type,
                file_path,
                coupled_files_count,
                critical_count,
                high_count,
                medium_count,
                low_count,
                test_files_found,
                test_intents_total,
                commit_count,
                analysis_time_ms as i64,
                note_id,
                repo_root,
            ],
        )?;
        Ok(())
    }

    /// Get aggregated metrics summary for a repository.
    pub fn get_metrics_summary(
        &self,
        repo_root: &str,
    ) -> Result<crate::types::MetricsSummary, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT
                COUNT(*) FILTER (WHERE event_type = 'analysis') as total_analyses,
                COUNT(*) FILTER (WHERE event_type = 'add_note') as notes_created,
                COUNT(*) FILTER (WHERE event_type = 'search_notes') as searches_performed,
                COUNT(*) FILTER (WHERE event_type = 'list_notes') as lists_performed,
                COALESCE(SUM(coupled_files_count), 0) as total_coupled_files,
                COALESCE(SUM(critical_count), 0) as critical_risk_count,
                COALESCE(SUM(high_count), 0) as high_risk_count,
                COALESCE(SUM(medium_count), 0) as medium_risk_count,
                COALESCE(SUM(low_count), 0) as low_risk_count,
                COALESCE(SUM(test_files_found), 0) as test_files_found,
                COALESCE(SUM(test_intents_total), 0) as test_intents_extracted,
                COALESCE(AVG(analysis_time_ms) FILTER (WHERE event_type = 'analysis'), 0) as avg_analysis_time_ms
            FROM metrics_events
            WHERE repo_root = ?1",
        )?;

        let summary = stmt.query_row(params![repo_root], |row| {
            Ok(crate::types::MetricsSummary {
                total_analyses: row.get::<_, i64>(0)? as u32,
                notes_created: row.get::<_, i64>(1)? as u32,
                searches_performed: row.get::<_, i64>(2)? as u32,
                lists_performed: row.get::<_, i64>(3)? as u32,
                total_coupled_files: row.get::<_, i64>(4)? as u32,
                critical_risk_count: row.get::<_, i64>(5)? as u32,
                high_risk_count: row.get::<_, i64>(6)? as u32,
                medium_risk_count: row.get::<_, i64>(7)? as u32,
                low_risk_count: row.get::<_, i64>(8)? as u32,
                test_files_found: row.get::<_, i64>(9)? as u32,
                test_intents_extracted: row.get::<_, i64>(10)? as u32,
                avg_analysis_time_ms: row.get::<_, f64>(11)? as u64,
            })
        })?;

        Ok(summary)
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
    fn test_indexing_state_roundtrip() {
        let db = Database::in_memory().unwrap();

        assert!(db.get_indexing_state().unwrap().is_none());
        assert!(db.is_first_index_call().unwrap());

        let state = IndexingState {
            head_commit: "abc123".to_string(),
            resume_oid: Some("def456".to_string()),
            commits_indexed: 500,
            strategy: "path_filtered".to_string(),
            is_complete: false,
            last_updated: 1700000000,
            target_path: Some("kernel/sched/core.c".to_string()),
        };
        db.set_indexing_state(&state).unwrap();

        let loaded = db.get_indexing_state().unwrap().unwrap();
        assert_eq!(loaded.head_commit, "abc123");
        assert_eq!(loaded.resume_oid, Some("def456".to_string()));
        assert_eq!(loaded.commits_indexed, 500);
        assert_eq!(loaded.strategy, "path_filtered");
        assert!(!loaded.is_complete);
        assert_eq!(loaded.last_updated, 1700000000);
        assert_eq!(loaded.target_path, Some("kernel/sched/core.c".to_string()));
        assert!(!db.is_first_index_call().unwrap());
    }

    #[test]
    fn test_indexing_state_overwrite() {
        let db = Database::in_memory().unwrap();

        let state1 = IndexingState {
            head_commit: "aaa".to_string(),
            resume_oid: None,
            commits_indexed: 100,
            strategy: "global".to_string(),
            is_complete: false,
            last_updated: 1000,
            target_path: None,
        };
        db.set_indexing_state(&state1).unwrap();

        let state2 = IndexingState {
            head_commit: "bbb".to_string(),
            resume_oid: None,
            commits_indexed: 1000,
            strategy: "global".to_string(),
            is_complete: true,
            last_updated: 2000,
            target_path: None,
        };
        db.set_indexing_state(&state2).unwrap();

        let loaded = db.get_indexing_state().unwrap().unwrap();
        assert_eq!(loaded.head_commit, "bbb");
        assert!(loaded.is_complete);
        assert_eq!(loaded.commits_indexed, 1000);
    }

    #[test]
    fn test_stale_lock_detection() {
        let db = Database::in_memory().unwrap();

        let state = IndexingState {
            head_commit: "abc".to_string(),
            resume_oid: Some("def".to_string()),
            commits_indexed: 50,
            strategy: "global".to_string(),
            is_complete: false,
            last_updated: 1000, // Very old timestamp
            target_path: None,
        };
        db.set_indexing_state(&state).unwrap();

        let loaded = db.get_indexing_state().unwrap().unwrap();
        let now = 1020; // 20 seconds later
        let is_stale = !loaded.is_complete && (now - loaded.last_updated) > 10;
        assert!(is_stale, "Should detect stale incomplete indexing state");
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

    #[test]
    fn test_insert_and_query_metrics() {
        let db = Database::in_memory().unwrap();

        // Insert an analysis event
        db.insert_metrics_event(
            "analysis",
            Some("src/A.ts"),
            5,  // coupled_files_count
            1,  // critical_count
            2,  // high_count
            1,  // medium_count
            1,  // low_count
            2,  // test_files_found
            5,  // test_intents_total
            10, // commit_count
            150, // analysis_time_ms
            None,
            "/repo/root",
        )
        .unwrap();

        // Insert another analysis event
        db.insert_metrics_event(
            "analysis",
            Some("src/B.ts"),
            3,
            0,
            1,
            1,
            1,
            1,
            3,
            5,
            100,
            None,
            "/repo/root",
        )
        .unwrap();

        // Insert a note event
        db.insert_metrics_event(
            "add_note",
            Some("src/C.ts"),
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            Some(1),
            "/repo/root",
        )
        .unwrap();

        // Query summary
        let summary = db.get_metrics_summary("/repo/root").unwrap();
        assert_eq!(summary.total_analyses, 2);
        assert_eq!(summary.notes_created, 1);
        assert_eq!(summary.total_coupled_files, 8);
        assert_eq!(summary.critical_risk_count, 1);
        assert_eq!(summary.high_risk_count, 3);
        assert_eq!(summary.medium_risk_count, 2);
        assert_eq!(summary.low_risk_count, 2);
        assert_eq!(summary.test_files_found, 3);
        assert_eq!(summary.test_intents_extracted, 8);
        assert_eq!(summary.avg_analysis_time_ms, 125); // (150 + 100) / 2
    }

    #[test]
    fn test_metrics_aggregation() {
        let db = Database::in_memory().unwrap();

        // Insert events for multiple repos
        db.insert_metrics_event(
            "analysis",
            Some("src/A.ts"),
            2,
            1,
            0,
            0,
            1,
            1,
            2,
            5,
            100,
            None,
            "/repo1",
        )
        .unwrap();

        db.insert_metrics_event(
            "analysis",
            Some("src/B.ts"),
            3,
            0,
            1,
            1,
            1,
            1,
            3,
            8,
            200,
            None,
            "/repo2",
        )
        .unwrap();

        // Each repo should have isolated metrics
        let summary1 = db.get_metrics_summary("/repo1").unwrap();
        assert_eq!(summary1.total_analyses, 1);
        assert_eq!(summary1.total_coupled_files, 2);

        let summary2 = db.get_metrics_summary("/repo2").unwrap();
        assert_eq!(summary2.total_analyses, 1);
        assert_eq!(summary2.total_coupled_files, 3);
    }

    #[test]
    fn test_empty_metrics() {
        let db = Database::in_memory().unwrap();
        let summary = db.get_metrics_summary("/nonexistent").unwrap();
        assert_eq!(summary.total_analyses, 0);
        assert_eq!(summary.notes_created, 0);
        assert_eq!(summary.total_coupled_files, 0);
        assert_eq!(summary.avg_analysis_time_ms, 0);
    }
}

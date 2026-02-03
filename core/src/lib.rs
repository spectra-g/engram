pub mod cli;
pub mod knowledge;
pub mod persistence;
pub mod risk;
pub mod temporal;
pub mod types;

use std::path::Path;

use persistence::Database;
use types::{AddNoteResponse, AnalysisResponse, ListNotesResponse, SearchNotesResponse};

fn open_db(repo_root: &Path) -> Result<Database, Box<dyn std::error::Error>> {
    let engram_dir = repo_root.join(".engram");
    std::fs::create_dir_all(&engram_dir)?;
    let db_path = engram_dir.join("engram.db");
    Ok(Database::open(&db_path)?)
}

/// Main entry point for analysis. Opens/creates the SQLite database
/// in the repo's `.engram/` directory, indexes git history, and
/// returns coupling analysis for the given file.
pub fn analyze(
    repo_root: &Path,
    file_path: &str,
) -> Result<AnalysisResponse, Box<dyn std::error::Error>> {
    let db = open_db(repo_root)?;
    let mut response = temporal::analyze(repo_root, file_path, &db)?;
    knowledge::enrich_with_memories(&db, &mut response.coupled_files);
    Ok(response)
}

pub fn add_note(
    repo_root: &Path,
    file_path: &str,
    symbol_name: Option<&str>,
    content: &str,
) -> Result<AddNoteResponse, Box<dyn std::error::Error>> {
    let db = open_db(repo_root)?;
    knowledge::add_note(&db, file_path, symbol_name, content)
}

pub fn search_notes(
    repo_root: &Path,
    query: &str,
) -> Result<SearchNotesResponse, Box<dyn std::error::Error>> {
    let db = open_db(repo_root)?;
    knowledge::search_notes(&db, query)
}

pub fn list_notes(
    repo_root: &Path,
    file_path: Option<&str>,
) -> Result<ListNotesResponse, Box<dyn std::error::Error>> {
    let db = open_db(repo_root)?;
    knowledge::list_notes(&db, file_path)
}

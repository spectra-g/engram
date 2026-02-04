pub mod cli;
pub mod knowledge;
pub mod metrics;
pub mod persistence;
pub mod risk;
pub mod temporal;
pub mod test_intents;
pub mod types;

use std::path::Path;

use persistence::Database;
use types::{AddNoteResponse, AnalysisResponse, ListNotesResponse, MetricsResponse, SearchNotesResponse};

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
    test_intents::enrich_with_test_intents(repo_root, &mut response.coupled_files);

    // Record metrics (non-blocking - errors are logged but don't fail the analysis)
    if let Err(e) = metrics::record_analysis_event(&db, &response, &repo_root.to_string_lossy()) {
        eprintln!("Warning: Failed to record analysis metrics: {}", e);
    }

    Ok(response)
}

pub fn add_note(
    repo_root: &Path,
    file_path: &str,
    symbol_name: Option<&str>,
    content: &str,
) -> Result<AddNoteResponse, Box<dyn std::error::Error>> {
    let db = open_db(repo_root)?;
    let response = knowledge::add_note(&db, file_path, symbol_name, content)?;

    // Record metrics (non-blocking - errors are logged but don't fail the note creation)
    if let Err(e) = metrics::record_note_event(&db, response.id, &response.file_path, &repo_root.to_string_lossy()) {
        eprintln!("Warning: Failed to record note metrics: {}", e);
    }

    Ok(response)
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

pub fn get_metrics(
    repo_root: &Path,
) -> Result<MetricsResponse, Box<dyn std::error::Error>> {
    let db = open_db(repo_root)?;
    metrics::get_metrics(&db, &repo_root.to_string_lossy())
}

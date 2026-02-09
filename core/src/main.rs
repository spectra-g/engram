use clap::Parser;
use std::path::Path;
use std::process;
use std::time::Duration;

use engram_core::cli::{Cli, Command};

/// Background task info: repo root + optional file path for PathFiltered indexing.
struct BackgroundTask {
    repo_root: std::path::PathBuf,
    file_path: Option<String>,
}

/// Run the requested command, returning (json_string, optional_background_task).
/// The background task continues indexing after stdout is flushed.
fn run() -> Result<(String, Option<BackgroundTask>), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Command::Analyze { file, repo_root } => {
            let result = engram_core::analyze(Path::new(&repo_root), &file)?;
            let json = serde_json::to_string(&result.response)?;
            let bg = if result.needs_background {
                Some(BackgroundTask {
                    repo_root: result.repo_root,
                    file_path: Some(result.file_path),
                })
            } else {
                None
            };
            Ok((json, bg))
        }
        Command::AddNote { file, symbol, content, repo_root } => {
            let response = engram_core::add_note(
                Path::new(&repo_root),
                &file,
                symbol.as_deref(),
                &content,
            )?;
            Ok((serde_json::to_string(&response)?, None))
        }
        Command::SearchNotes { query, repo_root } => {
            let response = engram_core::search_notes(Path::new(&repo_root), &query)?;
            Ok((serde_json::to_string(&response)?, None))
        }
        Command::ListNotes { file, repo_root } => {
            let response = engram_core::list_notes(Path::new(&repo_root), file.as_deref())?;
            Ok((serde_json::to_string(&response)?, None))
        }
        Command::GetMetrics { repo_root } => {
            let response = engram_core::get_metrics(Path::new(&repo_root))?;
            Ok((serde_json::to_string(&response)?, None))
        }
    }
}

fn main() {
    match run() {
        Ok((json, background_task)) => {
            println!("{json}");

            // Flush stdout so the adapter sees the JSON immediately
            use std::io::Write;
            if let Err(e) = std::io::stdout().flush() {
                eprintln!("Warning: stdout flush failed: {e}");
            }

            // Background indexing (runs after adapter has received the response)
            if let Some(task) = background_task {
                if let Err(e) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    if let Err(e) = engram_core::indexing::background_index(
                        &task.repo_root,
                        Duration::from_secs(5),
                        task.file_path.as_deref(),
                    ) {
                        eprintln!("Background indexing error: {e}");
                    }
                })) {
                    eprintln!("Background indexing panicked: {e:?}");
                }
            }
        }
        Err(e) => {
            eprintln!("Error: {e}");
            process::exit(1);
        }
    }
}

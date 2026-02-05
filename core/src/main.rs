use clap::Parser;
use std::path::Path;
use std::process;

use engram_core::cli::{Cli, Command};

fn run() -> Result<String, Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Command::Analyze { file, repo_root } => {
            let response = engram_core::analyze(Path::new(&repo_root), &file)?;
            Ok(serde_json::to_string(&response)?)
        }
        Command::AddNote { file, symbol, content, repo_root } => {
            let response = engram_core::add_note(
                Path::new(&repo_root),
                &file,
                symbol.as_deref(),
                &content,
            )?;
            Ok(serde_json::to_string(&response)?)
        }
        Command::SearchNotes { query, repo_root } => {
            let response = engram_core::search_notes(Path::new(&repo_root), &query)?;
            Ok(serde_json::to_string(&response)?)
        }
        Command::ListNotes { file, repo_root } => {
            let response = engram_core::list_notes(Path::new(&repo_root), file.as_deref())?;
            Ok(serde_json::to_string(&response)?)
        }
        Command::GetMetrics { repo_root } => {
            let response = engram_core::get_metrics(Path::new(&repo_root))?;
            Ok(serde_json::to_string(&response)?)
        }
    }
}

fn main() {
    match run() {
        Ok(json) => println!("{json}"),
        Err(e) => {
            eprintln!("Error: {e}");
            process::exit(1);
        }
    }
}

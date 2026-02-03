use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "engram-core", about = "Blast radius detector for AI agents")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Analyze the blast radius of a file change
    Analyze {
        /// Path to the file to analyze (relative to repo root)
        #[arg(long)]
        file: String,

        /// Path to the git repository root
        #[arg(long)]
        repo_root: String,
    },

    /// Add a note (memory) about a file or symbol
    AddNote {
        /// File path the note relates to
        #[arg(long)]
        file: String,

        /// Optional symbol name the note relates to
        #[arg(long)]
        symbol: Option<String>,

        /// The note content
        #[arg(long)]
        content: String,

        /// Path to the git repository root
        #[arg(long)]
        repo_root: String,
    },

    /// Search notes by content or file path
    SearchNotes {
        /// Search query
        #[arg(long)]
        query: String,

        /// Path to the git repository root
        #[arg(long)]
        repo_root: String,
    },

    /// List notes, optionally filtered by file
    ListNotes {
        /// Optional file path filter
        #[arg(long)]
        file: Option<String>,

        /// Path to the git repository root
        #[arg(long)]
        repo_root: String,
    },
}

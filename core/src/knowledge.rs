use crate::persistence::Database;
use crate::types::{AddNoteResponse, CoupledFile, ListNotesResponse, SearchNotesResponse};

pub fn add_note(
    db: &Database,
    file_path: &str,
    symbol_name: Option<&str>,
    content: &str,
) -> Result<AddNoteResponse, Box<dyn std::error::Error>> {
    let id = db.add_memory(file_path, symbol_name, content)?;
    Ok(AddNoteResponse {
        id,
        file_path: file_path.to_string(),
        content: content.to_string(),
    })
}

pub fn search_notes(
    db: &Database,
    query: &str,
) -> Result<SearchNotesResponse, Box<dyn std::error::Error>> {
    let memories = db.search_memories(query)?;
    Ok(SearchNotesResponse {
        query: query.to_string(),
        memories,
    })
}

pub fn list_notes(
    db: &Database,
    file_path: Option<&str>,
) -> Result<ListNotesResponse, Box<dyn std::error::Error>> {
    let memories = db.list_memories(file_path)?;
    Ok(ListNotesResponse {
        file_path: file_path.map(|s| s.to_string()),
        memories,
    })
}

pub fn enrich_with_memories(
    db: &Database,
    coupled_files: &mut [CoupledFile],
) {
    for file in coupled_files.iter_mut() {
        if let Ok(memories) = db.memories_for_file(&file.path) {
            file.memories = memories;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_note_response() {
        let db = Database::in_memory().unwrap();
        let resp = add_note(&db, "src/Auth.ts", Some("login"), "Handles OAuth flow").unwrap();

        assert!(resp.id > 0);
        assert_eq!(resp.file_path, "src/Auth.ts");
        assert_eq!(resp.content, "Handles OAuth flow");
    }

    #[test]
    fn test_enrich_coupled_files() {
        let db = Database::in_memory().unwrap();
        db.add_memory("src/Session.ts", None, "Session note").unwrap();

        let mut files = vec![
            CoupledFile {
                path: "src/Session.ts".to_string(),
                coupling_score: 0.9,
                co_change_count: 48,
                risk_score: 0.89,
                memories: Vec::new(),
                test_intents: Vec::new(),
            },
            CoupledFile {
                path: "src/Utils.ts".to_string(),
                coupling_score: 0.1,
                co_change_count: 1,
                risk_score: 0.2,
                memories: Vec::new(),
                test_intents: Vec::new(),
            },
        ];

        enrich_with_memories(&db, &mut files);

        assert_eq!(files[0].memories.len(), 1);
        assert_eq!(files[0].memories[0].content, "Session note");
        assert!(files[1].memories.is_empty());
    }
}

use git2::Repository;
use std::path::Path;
use std::time::Instant;

use crate::persistence::Database;
use crate::risk::{self, RawCoupledFileStats, TimeWindow};
use crate::types::AnalysisResponse;

const DEFAULT_COMMIT_LIMIT: usize = 1000;

/// Index recent git history into the database.
/// Returns the number of commits indexed in this call.
pub fn index_history(
    repo: &Repository,
    db: &Database,
    commit_limit: usize,
) -> Result<u32, Box<dyn std::error::Error>> {
    let mut revwalk = repo.revwalk()?;
    revwalk.push_head()?;
    revwalk.set_sorting(git2::Sort::TIME)?;

    let watermark = db.get_watermark()?;
    let mut indexed = 0u32;

    for oid_result in revwalk {
        if indexed as usize >= commit_limit {
            break;
        }

        let oid = oid_result?;
        let hash = oid.to_string();

        // Stop if we've already indexed this commit
        if let Some(ref wm) = watermark {
            if *wm == hash {
                break;
            }
        }

        let commit = repo.find_commit(oid)?;
        let timestamp = commit.time().seconds();
        let tree = commit.tree()?;

        // Get parent tree (empty for first commit)
        let parent_tree = if commit.parent_count() > 0 {
            Some(commit.parent(0)?.tree()?)
        } else {
            None
        };

        let diff = repo.diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), None)?;

        let mut files_in_commit: Vec<String> = Vec::new();
        diff.foreach(
            &mut |delta, _| {
                if let Some(path) = delta.new_file().path() {
                    if let Some(path_str) = path.to_str() {
                        files_in_commit.push(path_str.to_string());
                    }
                }
                true
            },
            None,
            None,
            None,
        )?;

        if !files_in_commit.is_empty() {
            let file_refs: Vec<&str> = files_in_commit.iter().map(|s| s.as_str()).collect();
            db.insert_commit(&hash, &file_refs, timestamp)?;
        }

        // Update watermark to the most recent commit on first iteration
        if indexed == 0 {
            db.set_watermark(&hash)?;
        }

        indexed += 1;
    }

    Ok(indexed)
}

/// Analyze coupling for a given file path.
/// Indexes history if needed, then queries the database.
pub fn analyze(
    repo_root: &Path,
    file_path: &str,
    db: &Database,
) -> Result<AnalysisResponse, Box<dyn std::error::Error>> {
    let start = Instant::now();
    let repo = Repository::open(repo_root)?;

    // Index history (incremental — skips already-indexed commits)
    index_history(&repo, db, DEFAULT_COMMIT_LIMIT)?;

    let coupled_raw = db.coupled_files_with_stats(file_path)?;
    let commit_count = db.commit_count(file_path)?;
    let (oldest_ts, newest_ts) = db.commit_time_range()?;

    let raw_stats: Vec<RawCoupledFileStats> = coupled_raw
        .into_iter()
        .map(|(path, co_change_count, total_commits, last_timestamp)| {
            RawCoupledFileStats {
                path,
                co_change_count,
                total_commits,
                last_timestamp,
            }
        })
        .collect();

    let window = TimeWindow {
        oldest_ts,
        newest_ts,
    };

    let coupled_files = risk::score_coupled_files(raw_stats, commit_count, &window);

    let elapsed = start.elapsed();

    Ok(AnalysisResponse {
        file_path: file_path.to_string(),
        repo_root: repo_root.to_string_lossy().to_string(),
        coupled_files,
        commit_count,
        analysis_time_ms: elapsed.as_millis() as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::Signature;
    use std::fs;
    use tempfile::TempDir;

    /// Helper: create a git repo in a temp dir, commit files together, return the temp dir.
    fn create_test_repo(commits: &[Vec<(String, String)>]) -> TempDir {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();

        let sig = Signature::now("Test", "test@test.com").unwrap();

        for (i, files) in commits.iter().enumerate() {
            // Write files
            for (path, content) in files {
                let full_path = dir.path().join(path);
                if let Some(parent) = full_path.parent() {
                    fs::create_dir_all(parent).unwrap();
                }
                fs::write(&full_path, content).unwrap();
            }

            // Stage all
            let mut index = repo.index().unwrap();
            index
                .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
                .unwrap();
            index.write().unwrap();

            let tree_id = index.write_tree().unwrap();
            let tree = repo.find_tree(tree_id).unwrap();

            if i == 0 {
                repo.commit(Some("HEAD"), &sig, &sig, &format!("commit {i}"), &tree, &[])
                    .unwrap();
            } else {
                let parent = repo.head().unwrap().peel_to_commit().unwrap();
                repo.commit(
                    Some("HEAD"),
                    &sig,
                    &sig,
                    &format!("commit {i}"),
                    &tree,
                    &[&parent],
                )
                .unwrap();
            }
        }

        dir
    }

    fn f(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
    }

    #[test]
    fn test_index_history_and_coupling() {
        let mut commits = Vec::new();

        // Initial commit with all files
        commits.push(f(&[
            ("src/A.ts", "v0"),
            ("src/B.ts", "v0"),
            ("src/C.ts", "v0"),
        ]));

        // 10 coupled commits: A + B
        for i in 1..=10 {
            let va = format!("v{i}");
            let vb = format!("v{i}");
            commits.push(f(&[("src/A.ts", &va), ("src/B.ts", &vb)]));
        }

        let dir = create_test_repo(&commits);
        let db = Database::in_memory().unwrap();

        let response = analyze(dir.path(), "src/A.ts", &db).unwrap();

        assert_eq!(response.file_path, "src/A.ts");
        assert!(response.commit_count >= 10);

        // B should be the most coupled file
        assert!(!response.coupled_files.is_empty());
        let b_file = response
            .coupled_files
            .iter()
            .find(|f| f.path == "src/B.ts")
            .expect("src/B.ts should be coupled");

        // A and B were committed together in all commits (initial + 10 updates = 11)
        // A total commits = 11, co_change with B = 11 → score = 1.0
        assert!(
            b_file.coupling_score > 0.8,
            "coupling score should be > 0.8, got {}",
            b_file.coupling_score
        );

        // C should have low coupling (only initial commit)
        if let Some(c_file) = response.coupled_files.iter().find(|f| f.path == "src/C.ts") {
            assert!(
                c_file.coupling_score < 0.2,
                "C coupling should be < 0.2, got {}",
                c_file.coupling_score
            );
        }
    }

    #[test]
    fn test_incremental_indexing() {
        let commits = vec![
            f(&[("a.txt", "v1"), ("b.txt", "v1")]),
            f(&[("a.txt", "v2"), ("b.txt", "v2")]),
        ];

        let dir = create_test_repo(&commits);
        let db = Database::in_memory().unwrap();
        let repo = Repository::open(dir.path()).unwrap();

        let first_count = index_history(&repo, &db, 1000).unwrap();
        assert_eq!(first_count, 2);

        // Second call should index 0 (watermark is set)
        let second_count = index_history(&repo, &db, 1000).unwrap();
        assert_eq!(second_count, 0);
    }

    #[test]
    fn test_commit_limit_enforcement() {
        // Create 20 commits
        let mut commits = Vec::new();
        for i in 0..20 {
            commits.push(f(&[("a.txt", &format!("v{i}"))]));
        }

        let dir = create_test_repo(&commits);
        let db = Database::in_memory().unwrap();
        let repo = Repository::open(dir.path()).unwrap();

        // Only allow indexing 5 commits
        let indexed = index_history(&repo, &db, 5).unwrap();
        assert_eq!(indexed, 5, "should stop at the commit limit");

        // The DB should only have 5 distinct commits
        let count = db.commit_count("a.txt").unwrap();
        assert_eq!(count, 5, "DB should contain exactly 5 commits for a.txt");
    }
}

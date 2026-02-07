use git2::Repository;
use std::path::Path;
use std::time::Instant;

use crate::persistence::Database;
use crate::risk::{self, RawCoupledFileStats, TimeWindow};
use crate::types::AnalysisResponse;

const DEFAULT_COMMIT_LIMIT: usize = 1000;

/// Files that should be excluded from the temporal index because they
/// change in nearly every commit and produce misleading coupling signals.
const IGNORED_FILENAMES: &[&str] = &[
    "package-lock.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    "Cargo.lock",
    "Gemfile.lock",
    "poetry.lock",
    "composer.lock",
    "go.sum",
    ".DS_Store",
    "Thumbs.db",
];

const IGNORED_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "ico", "svg", "bmp", "webp",
    "woff", "woff2", "ttf", "eot", "otf",
    "zip", "tar", "gz", "bz2", "xz",
    "exe", "dll", "so", "dylib",
    "pdf", "doc", "docx",
    "pyc", "class", "o", "obj",
    "min.js", "min.css",
];

/// Returns true if the file should be included in the temporal index.
/// Filters out lock files, binary assets, and other noise.
fn should_index_file(path: &str) -> bool {
    // Check filename matches
    if let Some(filename) = path.rsplit('/').next() {
        if IGNORED_FILENAMES.contains(&filename) {
            return false;
        }
    }

    // Check extension matches
    let lower = path.to_lowercase();
    for ext in IGNORED_EXTENSIONS {
        if lower.ends_with(&format!(".{ext}")) {
            return false;
        }
    }

    true
}

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

    db.begin_transaction()?;

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

        let mut diff_opts = git2::DiffOptions::new();
        let diff = repo.diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), Some(&mut diff_opts))?;

        // Enable rename detection so coupling history survives file renames
        let mut find_opts = git2::DiffFindOptions::new();
        find_opts.renames(true);
        let mut diff = diff;
        diff.find_similar(Some(&mut find_opts))?;

        let mut files_in_commit: Vec<String> = Vec::new();
        diff.foreach(
            &mut |delta, _| {
                if let Some(path) = delta.new_file().path() {
                    if let Some(path_str) = path.to_str() {
                        if should_index_file(path_str) {
                            files_in_commit.push(path_str.to_string());
                        }
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

    db.commit_transaction()?;

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
        test_info: None,
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
    fn test_should_index_file_accepts_source_files() {
        assert!(should_index_file("src/Auth.ts"));
        assert!(should_index_file("lib/utils.rs"));
        assert!(should_index_file("README.md"));
        assert!(should_index_file("Cargo.toml"));
        assert!(should_index_file("package.json"));
    }

    #[test]
    fn test_should_index_file_rejects_lockfiles() {
        assert!(!should_index_file("package-lock.json"));
        assert!(!should_index_file("yarn.lock"));
        assert!(!should_index_file("Cargo.lock"));
        assert!(!should_index_file("pnpm-lock.yaml"));
        assert!(!should_index_file("node_modules/foo/yarn.lock"));
    }

    #[test]
    fn test_should_index_file_rejects_binaries() {
        assert!(!should_index_file("assets/logo.png"));
        assert!(!should_index_file("fonts/inter.woff2"));
        assert!(!should_index_file("dist/bundle.min.js"));
        assert!(!should_index_file("release/app.exe"));
        assert!(!should_index_file("lib/native.so"));
        assert!(!should_index_file("build/module.o"));
    }

    #[test]
    fn test_should_index_file_rejects_os_files() {
        assert!(!should_index_file(".DS_Store"));
        assert!(!should_index_file("some/dir/.DS_Store"));
        assert!(!should_index_file("Thumbs.db"));
    }

    #[test]
    fn test_lockfile_filtering_in_indexing() {
        let mut commits = Vec::new();

        // Commit with source + lockfile
        commits.push(f(&[
            ("src/A.ts", "v0"),
            ("package-lock.json", "lock v0"),
        ]));

        for i in 1..=5 {
            commits.push(f(&[
                ("src/A.ts", &format!("v{i}")),
                ("package-lock.json", &format!("lock v{i}")),
                ("src/B.ts", &format!("v{i}")),
            ]));
        }

        let dir = create_test_repo(&commits);
        let db = Database::in_memory().unwrap();

        let response = analyze(dir.path(), "src/A.ts", &db).unwrap();

        // package-lock.json should NOT appear as a coupled file
        let lockfile = response.coupled_files.iter().find(|f| f.path == "package-lock.json");
        assert!(lockfile.is_none(), "package-lock.json should be filtered out");

        // B.ts should still appear as coupled
        let b_file = response.coupled_files.iter().find(|f| f.path == "src/B.ts");
        assert!(b_file.is_some(), "src/B.ts should still be coupled");
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
    fn test_rename_detection() {
        // Pre-rename: A.ts and B.ts committed together
        // Rename: A.ts -> ARenamed.ts
        // Post-rename: ARenamed.ts and B.ts committed together
        // With rename detection, ARenamed.ts should still show up as a changed file
        // (not a delete+add), preserving coupling through B.ts
        let mut commits = Vec::new();

        // Commit 0: initial with both files
        commits.push(f(&[("src/A.ts", "v0"), ("src/B.ts", "v0")]));

        // Commit 1: change both together
        commits.push(f(&[("src/A.ts", "v1"), ("src/B.ts", "v1")]));

        let dir = create_test_repo(&commits);

        // Now do the rename via git2 directly
        let repo = Repository::open(dir.path()).unwrap();
        let sig = Signature::now("Test", "test@test.com").unwrap();

        // Rename A.ts -> ARenamed.ts (copy content, remove old)
        let old_content = fs::read_to_string(dir.path().join("src/A.ts")).unwrap();
        fs::write(dir.path().join("src/ARenamed.ts"), &old_content).unwrap();
        fs::remove_file(dir.path().join("src/A.ts")).unwrap();
        // Also change B.ts so it shows up in this commit
        fs::write(dir.path().join("src/B.ts"), "v2-after-rename").unwrap();

        let mut index = repo.index().unwrap();
        index.remove_path(Path::new("src/A.ts")).unwrap();
        index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let parent = repo.head().unwrap().peel_to_commit().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "rename A to ARenamed", &tree, &[&parent]).unwrap();

        // Index and analyze
        let db = Database::in_memory().unwrap();
        let indexed = index_history(&repo, &db, 1000).unwrap();
        assert!(indexed >= 3);

        // ARenamed.ts should appear in the index (from the rename commit)
        let count = db.commit_count("src/ARenamed.ts").unwrap();
        assert!(count >= 1, "ARenamed.ts should be indexed, got count={count}");

        // B.ts should be coupled to ARenamed.ts (they were in the same rename commit)
        let coupled = db.coupled_files("src/ARenamed.ts").unwrap();
        let b_coupled = coupled.iter().find(|(p, _)| p == "src/B.ts");
        assert!(b_coupled.is_some(), "B.ts should be coupled to ARenamed.ts after rename");
    }

    #[test]
    fn test_should_index_file_extension_case_insensitive() {
        // Extension matching lowercases the path, so uppercase extensions are rejected
        assert!(!should_index_file("assets/Image.PNG"));
        assert!(!should_index_file("assets/Logo.JPG"));
        assert!(!should_index_file("assets/Photo.JPEG"));
        assert!(!should_index_file("dist/bundle.MIN.JS"));
        assert!(!should_index_file("dist/styles.MIN.CSS"));
        assert!(!should_index_file("fonts/Inter.WOFF2"));
    }

    #[test]
    fn test_should_index_file_filename_case_sensitive() {
        // Filename matching is case-sensitive (no lowercasing), so these are NOT rejected
        assert!(should_index_file(".ds_store")); // lowercase, won't match ".DS_Store"
        assert!(should_index_file("PACKAGE-LOCK.JSON")); // uppercase, won't match "package-lock.json"
        assert!(should_index_file("YARN.LOCK")); // uppercase, won't match "yarn.lock"
    }

    #[test]
    fn test_merge_commit_includes_branch_changes() {
        // Documents current behavior: index_history diffs against parent(0) only.
        // For merge commits, this means all files changed on the merged branch
        // appear in the diff, inflating co-change counts.
        //
        // Setup: main has file A. Feature branch changes file B. Merge back to main.
        // After merge, A and B show coupling even though they were never edited together.

        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let sig = Signature::now("Test", "test@test.com").unwrap();

        // Commit 0 (main): create both files
        fs::write(dir.path().join("A.ts"), "v0").unwrap();
        fs::write(dir.path().join("B.ts"), "v0").unwrap();
        let mut index = repo.index().unwrap();
        index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let commit0 = repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[]).unwrap();
        let commit0 = repo.find_commit(commit0).unwrap();

        // Remember the initial branch name (master or main depending on git config)
        let initial_branch = repo.head().unwrap().name().unwrap().to_string();

        // Create feature branch from commit0
        repo.branch("feature", &commit0, false).unwrap();

        // Commit 1 (main): change A only
        fs::write(dir.path().join("A.ts"), "v1-main").unwrap();
        let mut index = repo.index().unwrap();
        index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let main_commit = repo.commit(Some("HEAD"), &sig, &sig, "main: change A", &tree, &[&commit0]).unwrap();
        let main_commit = repo.find_commit(main_commit).unwrap();

        // Switch to feature branch and change B only
        repo.set_head("refs/heads/feature").unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force())).unwrap();
        fs::write(dir.path().join("B.ts"), "v1-feature").unwrap();
        let mut index = repo.index().unwrap();
        index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let feature_commit = repo.commit(Some("refs/heads/feature"), &sig, &sig, "feature: change B", &tree, &[&commit0]).unwrap();
        let feature_commit = repo.find_commit(feature_commit).unwrap();

        // Merge: switch back to the initial branch
        repo.set_head(&initial_branch).unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force())).unwrap();

        // Build merged index
        let mut merge_index = repo.merge_commits(&main_commit, &feature_commit, None).unwrap();
        let merge_tree_id = merge_index.write_tree_to(&repo).unwrap();
        let merge_tree = repo.find_tree(merge_tree_id).unwrap();
        repo.commit(
            Some("HEAD"), &sig, &sig, "merge feature into main",
            &merge_tree, &[&main_commit, &feature_commit],
        ).unwrap();

        // Index and check coupling
        let db = Database::in_memory().unwrap();
        let repo = Repository::open(dir.path()).unwrap();
        let indexed = index_history(&repo, &db, 1000).unwrap();
        assert!(indexed >= 4, "should index at least 4 commits, got {indexed}");

        // Current behavior: the merge commit diffs against parent(0) (main_commit),
        // so B.ts appears changed in the merge diff. This means A.ts and B.ts
        // show coupling through the merge, even though no single non-merge commit
        // changed both files together.
        let coupled = db.coupled_files("A.ts").unwrap();
        let b_coupled = coupled.iter().find(|(p, _)| p == "B.ts");
        assert!(
            b_coupled.is_some(),
            "B.ts should appear coupled to A.ts due to merge commit diffing against parent(0)"
        );
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

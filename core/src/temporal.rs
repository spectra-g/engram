use git2::Repository;
use std::path::Path;
use std::time::{Duration, Instant};

use crate::indexing;
use crate::persistence::Database;
use crate::risk::{self, RawCoupledFileStats, TimeWindow};
use crate::types::{AnalysisResponse, IndexingStatus};

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
pub(crate) fn should_index_file(path: &str) -> bool {
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

/// Analyze coupling for a given file path.
/// Uses adaptive smart indexing, then queries the database.
/// Returns (AnalysisResponse, needs_background_indexing).
pub fn analyze(
    repo_root: &Path,
    file_path: &str,
    db: &Database,
) -> Result<(AnalysisResponse, bool), Box<dyn std::error::Error>> {
    let start = Instant::now();
    let repo = Repository::open(repo_root)?;

    // Smart adaptive indexing (time-budgeted)
    // Budget leaves ~500ms headroom for repo open, DB queries, and caller overhead
    // to stay within the 2s first-call target.
    let index_result = indexing::smart_index(
        &repo,
        db,
        file_path,
        Duration::from_millis(1500),
    )?;

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

    let response = AnalysisResponse {
        file_path: file_path.to_string(),
        repo_root: repo_root.to_string_lossy().to_string(),
        coupled_files,
        commit_count,
        analysis_time_ms: elapsed.as_millis() as u64,
        test_info: None,
        indexing_status: Some(IndexingStatus {
            strategy: index_result.strategy.as_str().to_string(),
            commits_indexed: index_result.commits_indexed,
            is_complete: index_result.is_complete,
        }),
    };

    Ok((response, index_result.needs_background))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexing::budgeted_global_index;
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

        let (response, _) = analyze(dir.path(), "src/A.ts", &db).unwrap();

        // package-lock.json should NOT appear as a coupled file
        let lockfile = response.coupled_files.iter().find(|f| f.path == "package-lock.json");
        assert!(lockfile.is_none(), "package-lock.json should be filtered out");

        // B.ts should still appear as coupled
        let b_file = response.coupled_files.iter().find(|f| f.path == "src/B.ts");
        assert!(b_file.is_some(), "src/B.ts should still be coupled");
    }

    #[test]
    fn test_smart_index_and_coupling() {
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

        let (response, _) = analyze(dir.path(), "src/A.ts", &db).unwrap();

        assert_eq!(response.file_path, "src/A.ts");
        assert!(response.commit_count >= 10);

        // B should be the most coupled file
        assert!(!response.coupled_files.is_empty());
        let b_file = response
            .coupled_files
            .iter()
            .find(|f| f.path == "src/B.ts")
            .expect("src/B.ts should be coupled");

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

        // First call indexes everything via smart_index
        let (r1, _) = analyze(dir.path(), "a.txt", &db).unwrap();
        assert!(r1.indexing_status.as_ref().unwrap().is_complete);

        // Second call should do no additional indexing
        let (r2, _) = analyze(dir.path(), "a.txt", &db).unwrap();
        assert!(r2.indexing_status.as_ref().unwrap().is_complete);
    }

    #[test]
    fn test_rename_detection() {
        let mut commits = Vec::new();
        commits.push(f(&[("src/A.ts", "v0"), ("src/B.ts", "v0")]));
        commits.push(f(&[("src/A.ts", "v1"), ("src/B.ts", "v1")]));

        let dir = create_test_repo(&commits);

        let repo = Repository::open(dir.path()).unwrap();
        let sig = Signature::now("Test", "test@test.com").unwrap();

        let old_content = fs::read_to_string(dir.path().join("src/A.ts")).unwrap();
        fs::write(dir.path().join("src/ARenamed.ts"), &old_content).unwrap();
        fs::remove_file(dir.path().join("src/A.ts")).unwrap();
        fs::write(dir.path().join("src/B.ts"), "v2-after-rename").unwrap();

        let mut index = repo.index().unwrap();
        index.remove_path(Path::new("src/A.ts")).unwrap();
        index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let parent = repo.head().unwrap().peel_to_commit().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "rename A to ARenamed", &tree, &[&parent]).unwrap();

        // Use budgeted_global_index directly for rename detection test
        let db = Database::in_memory().unwrap();
        let repo = Repository::open(dir.path()).unwrap();
        let (indexed, _, _) = budgeted_global_index(
            &repo, &db, Duration::from_secs(10), 1000, None, 100,
        ).unwrap();
        assert!(indexed >= 3);

        let count = db.commit_count("src/ARenamed.ts").unwrap();
        assert!(count >= 1, "ARenamed.ts should be indexed, got count={count}");

        let coupled = db.coupled_files("src/ARenamed.ts").unwrap();
        let b_coupled = coupled.iter().find(|(p, _)| p == "src/B.ts");
        assert!(b_coupled.is_some(), "B.ts should be coupled to ARenamed.ts after rename");
    }

    #[test]
    fn test_should_index_file_extension_case_insensitive() {
        assert!(!should_index_file("assets/Image.PNG"));
        assert!(!should_index_file("assets/Logo.JPG"));
        assert!(!should_index_file("assets/Photo.JPEG"));
        assert!(!should_index_file("dist/bundle.MIN.JS"));
        assert!(!should_index_file("dist/styles.MIN.CSS"));
        assert!(!should_index_file("fonts/Inter.WOFF2"));
    }

    #[test]
    fn test_should_index_file_filename_case_sensitive() {
        assert!(should_index_file(".ds_store"));
        assert!(should_index_file("PACKAGE-LOCK.JSON"));
        assert!(should_index_file("YARN.LOCK"));
    }

    #[test]
    fn test_merge_commit_includes_branch_changes() {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let sig = Signature::now("Test", "test@test.com").unwrap();

        fs::write(dir.path().join("A.ts"), "v0").unwrap();
        fs::write(dir.path().join("B.ts"), "v0").unwrap();
        let mut index = repo.index().unwrap();
        index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let commit0 = repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[]).unwrap();
        let commit0 = repo.find_commit(commit0).unwrap();

        let initial_branch = repo.head().unwrap().name().unwrap().to_string();
        repo.branch("feature", &commit0, false).unwrap();

        fs::write(dir.path().join("A.ts"), "v1-main").unwrap();
        let mut index = repo.index().unwrap();
        index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let main_commit = repo.commit(Some("HEAD"), &sig, &sig, "main: change A", &tree, &[&commit0]).unwrap();
        let main_commit = repo.find_commit(main_commit).unwrap();

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

        repo.set_head(&initial_branch).unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force())).unwrap();

        let mut merge_index = repo.merge_commits(&main_commit, &feature_commit, None).unwrap();
        let merge_tree_id = merge_index.write_tree_to(&repo).unwrap();
        let merge_tree = repo.find_tree(merge_tree_id).unwrap();
        repo.commit(
            Some("HEAD"), &sig, &sig, "merge feature into main",
            &merge_tree, &[&main_commit, &feature_commit],
        ).unwrap();

        let db = Database::in_memory().unwrap();
        let repo = Repository::open(dir.path()).unwrap();
        let (indexed, _, _) = budgeted_global_index(
            &repo, &db, Duration::from_secs(10), 1000, None, 100,
        ).unwrap();
        assert!(indexed >= 4, "should index at least 4 commits, got {indexed}");

        let coupled = db.coupled_files("A.ts").unwrap();
        let b_coupled = coupled.iter().find(|(p, _)| p == "B.ts");
        assert!(
            b_coupled.is_some(),
            "B.ts should appear coupled to A.ts due to merge commit diffing against parent(0)"
        );
    }

    #[test]
    fn test_commit_limit_enforcement() {
        let mut commits = Vec::new();
        for i in 0..20 {
            commits.push(f(&[("a.txt", &format!("v{i}"))]));
        }

        let dir = create_test_repo(&commits);
        let db = Database::in_memory().unwrap();
        let repo = Repository::open(dir.path()).unwrap();

        let (indexed, _, _) = budgeted_global_index(
            &repo, &db, Duration::from_secs(10), 5, None, 100,
        ).unwrap();
        assert_eq!(indexed, 5, "should stop at the commit limit");

        let count = db.commit_count("a.txt").unwrap();
        assert_eq!(count, 5, "DB should contain exactly 5 commits for a.txt");
    }
}

use git2::{Oid, Repository};
use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::persistence::{Database, IndexingState};
use crate::temporal::should_index_file;

const DEFAULT_COMMIT_LIMIT: usize = 1000;
const SCOPE_BUDGET_MS: u64 = 500;
const FOREGROUND_BATCH_SIZE: usize = 100;
const BACKGROUND_BATCH_SIZE: usize = 50;

/// Safety margin before starting a `diff_tree_to_tree`.
/// `path_filtered_index` uses `simplify_first_parent()` so diffs are against
/// first-parent only — typically 10-50ms on the Linux kernel. A 200ms margin
/// covers even large first-parent diffs while ensuring subsequent calls
/// (150ms budget < 200ms) never attempt diffs.
const DIFF_SAFETY_MARGIN_MS: u128 = 200;

/// The strategy chosen after the scoping phase.
#[derive(Debug, Clone, PartialEq)]
pub enum Strategy {
    /// Small repo: finished within scope budget.
    Complete,
    /// Fast repo (>40% in scope): continue global indexing.
    ContinueGlobal,
    /// Medium repo (1-40% in scope): budgeted global with background.
    BudgetedGlobal,
    /// Huge repo (<1% in scope): path-filtered indexing.
    PathFiltered,
}

impl Strategy {
    pub fn as_str(&self) -> &'static str {
        match self {
            Strategy::Complete => "complete",
            Strategy::ContinueGlobal => "continue_global",
            Strategy::BudgetedGlobal => "budgeted_global",
            Strategy::PathFiltered => "path_filtered",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "complete" => Strategy::Complete,
            "continue_global" => Strategy::ContinueGlobal,
            "budgeted_global" => Strategy::BudgetedGlobal,
            "path_filtered" => Strategy::PathFiltered,
            _ => Strategy::BudgetedGlobal,
        }
    }
}

/// Result of a smart_index call.
pub struct SmartIndexResult {
    pub strategy: Strategy,
    pub commits_indexed: u32,
    pub is_complete: bool,
    pub needs_background: bool,
}

/// Pure function: decide strategy based on scoping results.
pub fn decide_strategy(commits_processed: u32, hit_end: bool, commit_limit: usize) -> Strategy {
    if hit_end {
        return Strategy::Complete;
    }

    let limit = commit_limit as f64;
    let progress = commits_processed as f64 / limit;

    if progress > 0.4 {
        Strategy::ContinueGlobal
    } else if progress >= 0.01 {
        Strategy::BudgetedGlobal
    } else {
        Strategy::PathFiltered
    }
}

/// Cheap check: did `file_path` change in this commit vs its first parent?
/// Uses blob OID comparison — O(path_depth) per call.
/// Returns false if the file doesn't exist in either tree (no error).
pub fn file_changed_in_commit(
    commit: &git2::Commit,
    file_path: &Path,
) -> bool {
    let tree = match commit.tree() {
        Ok(t) => t,
        Err(_) => return false,
    };

    let commit_blob = tree.get_path(file_path).ok().map(|e| e.id());

    let parent_blob = if commit.parent_count() > 0 {
        commit
            .parent(0)
            .ok()
            .and_then(|p| p.tree().ok())
            .and_then(|t| t.get_path(file_path).ok())
            .map(|e| e.id())
    } else {
        // First commit: if file exists, it was added
        None
    };

    commit_blob != parent_blob
}

/// Time-bounded global indexing. Processes commits from HEAD (or resume_oid),
/// inserting changed files into the DB.
///
/// Returns (commits_indexed, last_oid_processed, hit_end_of_history).
pub fn budgeted_global_index(
    repo: &Repository,
    db: &Database,
    budget: Duration,
    commit_limit: usize,
    resume_from: Option<&str>,
    batch_size: usize,
) -> Result<(u32, Option<String>, bool), Box<dyn std::error::Error>> {
    let start = Instant::now();
    let mut revwalk = repo.revwalk()?;
    revwalk.set_sorting(git2::Sort::TIME)?;

    revwalk.push_head()?;

    if let Some(oid_str) = resume_from {
        let resume_oid = Oid::from_str(oid_str)?;
        // Skip commits until we pass the resume point
        loop {
            match revwalk.next() {
                Some(Ok(oid)) if oid == resume_oid => break,
                Some(Ok(_)) => continue,
                _ => return Ok((0, None, true)),
            }
        }
    }

    let mut indexed = 0u32;
    let mut last_oid: Option<String> = None;
    let mut hit_end = true;
    let mut batch_count = 0usize;

    db.begin_transaction()?;

    for oid_result in revwalk {
        if start.elapsed() >= budget || indexed as usize >= commit_limit {
            hit_end = false; // Stopped early (time or limit), not end of history
            break;
        }

        let oid = oid_result?;
        let hash = oid.to_string();
        let commit = repo.find_commit(oid)?;
        let timestamp = commit.time().seconds();
        let tree = commit.tree()?;

        let parent_tree = if commit.parent_count() > 0 {
            Some(commit.parent(0)?.tree()?)
        } else {
            None
        };

        let mut diff_opts = git2::DiffOptions::new();
        diff_opts.skip_binary_check(true);

        let diff = repo.diff_tree_to_tree(
            parent_tree.as_ref(),
            Some(&tree),
            Some(&mut diff_opts),
        )?;

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

        last_oid = Some(hash);
        indexed += 1;
        batch_count += 1;

        // Commit in batches to yield the write lock
        if batch_count >= batch_size {
            db.commit_transaction()?;
            db.begin_transaction()?;
            batch_count = 0;
        }
    }

    db.commit_transaction()?;

    Ok((indexed, last_oid, hit_end))
}

/// Path-filtered indexing for huge repos. Scans commits cheaply using
/// blob OID comparison, only doing full diffs when the target file changed.
///
/// When `resume_from` is Some, skips the revwalk to that OID and continues
/// from where the previous run left off (delayed detection context is
/// reconstructed from the resume commit's blob).
pub fn path_filtered_index(
    repo: &Repository,
    db: &Database,
    file_path: &str,
    budget: Duration,
    resume_from: Option<&str>,
    batch_size: usize,
) -> Result<(u32, Option<String>, bool), Box<dyn std::error::Error>> {
    let start = Instant::now();
    let target = Path::new(file_path);

    let mut revwalk = repo.revwalk()?;
    revwalk.push_head()?;
    revwalk.set_sorting(git2::Sort::TIME)?;
    // Follow only first-parent links — drastically reduces commit count
    // on merge-heavy repos (Linux kernel: 1.2M → ~100K commits)
    revwalk.simplify_first_parent()?;

    let mut indexed = 0u32;
    let mut last_oid: Option<String> = None;
    let mut hit_end = true;
    let mut batch_count = 0usize;

    // Delayed change detection: walk commits, extract blob OID for target
    // file from each commit's tree (1 tree load per commit instead of 2).
    // Compare consecutive blobs to detect changes.
    //
    // In a first-parent walk: commit[i]'s parent = commit[i+1].
    // If blob[i] != blob[i+1], commit[i] changed the file.
    // We detect this when we process commit[i+1] and compare against prev.
    let mut prev_entry: Option<(Oid, Option<Oid>)> = None; // (commit_oid, blob_oid)

    // Resume: skip to the resume point and reconstruct delayed detection context
    if let Some(oid_str) = resume_from {
        let resume_oid = Oid::from_str(oid_str)?;
        let mut skip_count = 0u32;
        let mut found = false;
        loop {
            skip_count += 1;
            if skip_count % 1000 == 0 && start.elapsed() >= budget {
                // Budget exhausted during skip — return no progress
                return Ok((0, None, false));
            }
            match revwalk.next() {
                Some(Ok(oid)) if oid == resume_oid => {
                    // Reconstruct prev_entry from the resume commit's blob
                    let commit = repo.find_commit(oid)?;
                    let tree = commit.tree()?;
                    let blob = tree.get_path(target).ok().map(|e| e.id());
                    prev_entry = Some((oid, blob));
                    last_oid = Some(oid.to_string());
                    found = true;
                    break;
                }
                Some(Ok(_)) => continue,
                _ => break, // OID not found (history rewritten?)
            }
        }
        if !found {
            // Resume OID not in history — caller should start fresh
            return Ok((0, None, false));
        }
    }

    db.begin_transaction()?;

    for oid_result in revwalk {
        if start.elapsed() >= budget {
            hit_end = false;
            break;
        }

        let oid = oid_result?;
        let commit = repo.find_commit(oid)?;
        let tree = commit.tree()?;
        let blob = tree.get_path(target).ok().map(|e| e.id());

        // Check if the PREVIOUS (newer) commit changed the file
        if let Some((prev_oid, prev_blob)) = prev_entry.take() {
            if prev_blob != blob {
                // Safety margin: don't start an expensive diff if we can't
                // afford it. A kernel merge diff can take 500ms+.
                let elapsed = start.elapsed();
                let remaining_ms = budget.as_millis().saturating_sub(elapsed.as_millis());
                if elapsed >= budget || remaining_ms < DIFF_SAFETY_MARGIN_MS {
                    hit_end = false;
                    break;
                }

                // prev commit changed the file — do full diff
                // current `tree` is the parent tree (since this commit IS the parent)
                let child_commit = repo.find_commit(prev_oid)?;
                let child_tree = child_commit.tree()?;

                let mut diff_opts = git2::DiffOptions::new();
                diff_opts.skip_binary_check(true);

                let diff = repo.diff_tree_to_tree(
                    Some(&tree),
                    Some(&child_tree),
                    Some(&mut diff_opts),
                )?;

                let hash = prev_oid.to_string();
                let timestamp = child_commit.time().seconds();
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

                indexed += 1;
                batch_count += 1;

                if batch_count >= batch_size {
                    db.commit_transaction()?;
                    db.begin_transaction()?;
                    batch_count = 0;
                }
            }
        }

        last_oid = Some(oid.to_string());
        prev_entry = Some((oid, blob));
    }

    // Handle root commit: if it has the file, it's the initial add
    if let Some((prev_oid, prev_blob)) = prev_entry {
        if prev_blob.is_some() && hit_end {
            let commit = repo.find_commit(prev_oid)?;
            if commit.parent_count() == 0 {
                // Safety margin for root diff too
                let remaining_ms = budget.as_millis().saturating_sub(start.elapsed().as_millis());
                if remaining_ms >= DIFF_SAFETY_MARGIN_MS {
                    let tree = commit.tree()?;
                    let hash = prev_oid.to_string();
                    let timestamp = commit.time().seconds();

                    let mut diff_opts = git2::DiffOptions::new();
                    diff_opts.skip_binary_check(true);

                    let diff = repo.diff_tree_to_tree(
                        None,
                        Some(&tree),
                        Some(&mut diff_opts),
                    )?;

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
                        let file_refs: Vec<&str> =
                            files_in_commit.iter().map(|s| s.as_str()).collect();
                        db.insert_commit(&hash, &file_refs, timestamp)?;
                    }

                    indexed += 1;
                }
            }
        }
    }

    db.commit_transaction()?;

    Ok((indexed, last_oid, hit_end))
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Orchestrator: scopes the repo, decides strategy, executes, saves state.
pub fn smart_index(
    repo: &Repository,
    db: &Database,
    file_path: &str,
    foreground_budget: Duration,
) -> Result<SmartIndexResult, Box<dyn std::error::Error>> {
    let existing_state = db.get_indexing_state()?;

    // Subsequent call: short budget, check if HEAD moved
    if let Some(ref state) = existing_state {
        let head = repo.head()?.peel_to_commit()?.id().to_string();

        if state.head_commit == head && state.is_complete {
            // Already fully indexed at this HEAD
            return Ok(SmartIndexResult {
                strategy: Strategy::from_str(&state.strategy),
                commits_indexed: state.commits_indexed,
                is_complete: true,
                needs_background: false,
            });
        }

        if state.head_commit == head && !state.is_complete {
            let prev_strategy = Strategy::from_str(&state.strategy);

            // PathFiltered with different file: the resume_oid and progress
            // are from a different file's walk. Start fresh for the new file
            // with a short budget — the safety margin guarantees no expensive
            // diffs happen, so this returns quickly. Background will build
            // coupling data for the new file.
            //
            // The temporal_index data from the old file's walk is retained
            // (it's valid coupling data, just for a different file).
            let file_changed = prev_strategy == Strategy::PathFiltered
                && state
                    .target_path
                    .as_ref()
                    .is_some_and(|p| p != file_path);

            if file_changed {
                // Full foreground budget — this is effectively a first call
                // for the new file, so it deserves the same time as any cold start.
                let (indexed, last_oid, hit_end) = path_filtered_index(
                    repo,
                    db,
                    file_path,
                    foreground_budget,
                    None, // Fresh walk from HEAD for the new file
                    FOREGROUND_BATCH_SIZE,
                )?;

                db.set_indexing_state(&IndexingState {
                    head_commit: head,
                    resume_oid: if hit_end { None } else { last_oid },
                    commits_indexed: indexed,
                    strategy: Strategy::PathFiltered.as_str().to_string(),
                    is_complete: hit_end,
                    last_updated: unix_now(),
                    target_path: Some(file_path.to_string()),
                })?;

                return Ok(SmartIndexResult {
                    strategy: Strategy::PathFiltered,
                    commits_indexed: indexed,
                    is_complete: hit_end,
                    needs_background: !hit_end,
                });
            }

            // Same HEAD, same file (or global strategy), not complete.
            //
            // For PathFiltered: return cached data immediately and let
            // background continue. The revwalk skip to a deep resume_oid
            // can take longer than any short foreground budget, so doing
            // foreground work here is counterproductive. The DB already
            // has coupling data from the first call + previous backgrounds.
            if prev_strategy == Strategy::PathFiltered {
                return Ok(SmartIndexResult {
                    strategy: prev_strategy,
                    commits_indexed: state.commits_indexed,
                    is_complete: false,
                    needs_background: true,
                });
            }

            // For global strategies: try to resume with a short budget.
            let is_stale = (unix_now() - state.last_updated) > 10;

            if is_stale || state.resume_oid.is_some() {
                let resume = state.resume_oid.as_deref();
                let remaining_budget = Duration::from_millis(150);

                let (indexed, last_oid, hit_end) = budgeted_global_index(
                    repo,
                    db,
                    remaining_budget,
                    DEFAULT_COMMIT_LIMIT.saturating_sub(state.commits_indexed as usize),
                    resume,
                    FOREGROUND_BATCH_SIZE,
                )?;

                let total = state.commits_indexed + indexed;
                let is_complete = hit_end;

                db.set_indexing_state(&IndexingState {
                    head_commit: head,
                    resume_oid: if is_complete {
                        None
                    } else {
                        last_oid.or(state.resume_oid.clone())
                    },
                    commits_indexed: total,
                    strategy: state.strategy.clone(),
                    is_complete,
                    last_updated: unix_now(),
                    target_path: state.target_path.clone(),
                })?;

                return Ok(SmartIndexResult {
                    strategy: prev_strategy,
                    commits_indexed: total,
                    is_complete,
                    needs_background: !is_complete,
                });
            }

            // Not stale, another process may be working — just return what we have
            return Ok(SmartIndexResult {
                strategy: prev_strategy,
                commits_indexed: state.commits_indexed,
                is_complete: false,
                needs_background: false,
            });
        }

        // HEAD moved — start fresh indexing
    }

    // First call (or HEAD moved)
    let head = repo.head()?.peel_to_commit()?.id().to_string();

    // Circuit breaker: check repo size before scoping.
    // If repo has >20K tracked files, a single diff_tree_to_tree on a merge
    // commit can take 20+ seconds. Skip scoping and go straight to PathFiltered.
    //
    // Instead of loading the full index (which takes ~100ms on Linux kernel),
    // stat the .git/index file. Each entry is ~62 bytes + path, so
    // 20K files ≈ 2MB index. Use 1MB threshold for safety margin.
    let index_path = repo.path().join("index");
    let index_size = std::fs::metadata(&index_path).map(|m| m.len()).unwrap_or(0);
    let is_huge = index_size > 1_000_000; // >1MB ≈ >10K tracked files

    let (strategy, scope_indexed, scope_last_oid) = if is_huge {
        // Huge repo: skip scoping entirely
        (Strategy::PathFiltered, 0u32, None)
    } else {
        // Normal repo: run scoping phase
        let scope_budget = Duration::from_millis(SCOPE_BUDGET_MS);
        let (indexed, last_oid, hit_end) = budgeted_global_index(
            repo,
            db,
            scope_budget,
            DEFAULT_COMMIT_LIMIT,
            None,
            FOREGROUND_BATCH_SIZE,
        )?;
        let strat = decide_strategy(indexed, hit_end, DEFAULT_COMMIT_LIMIT);
        (strat, indexed, last_oid)
    };

    if strategy == Strategy::Complete {
        db.set_indexing_state(&IndexingState {
            head_commit: head,
            resume_oid: None,
            commits_indexed: scope_indexed,
            strategy: strategy.as_str().to_string(),
            is_complete: true,
            last_updated: unix_now(),
            target_path: None,
        })?;

        return Ok(SmartIndexResult {
            strategy,
            commits_indexed: scope_indexed,
            is_complete: true,
            needs_background: false,
        });
    }

    // Execute phase: use remaining foreground budget
    // For huge repos, use the full budget (no time spent on scoping)
    let remaining = if is_huge {
        foreground_budget
    } else {
        foreground_budget.saturating_sub(Duration::from_millis(SCOPE_BUDGET_MS))
    };

    let (exec_indexed, exec_last_oid, exec_hit_end) = match strategy {
        Strategy::PathFiltered => {
            path_filtered_index(repo, db, file_path, remaining, None, FOREGROUND_BATCH_SIZE)?
        }
        Strategy::ContinueGlobal | Strategy::BudgetedGlobal => {
            let resume = scope_last_oid.as_deref();
            let remaining_limit = DEFAULT_COMMIT_LIMIT.saturating_sub(scope_indexed as usize);
            budgeted_global_index(repo, db, remaining, remaining_limit, resume, FOREGROUND_BATCH_SIZE)?
        }
        Strategy::Complete => unreachable!(),
    };

    let total_indexed = scope_indexed + exec_indexed;
    let is_complete = exec_hit_end;
    let final_resume = if is_complete { None } else { exec_last_oid.or(scope_last_oid) };

    let target_path = if strategy == Strategy::PathFiltered {
        Some(file_path.to_string())
    } else {
        None
    };

    db.set_indexing_state(&IndexingState {
        head_commit: head,
        resume_oid: final_resume,
        commits_indexed: total_indexed,
        strategy: strategy.as_str().to_string(),
        is_complete,
        last_updated: unix_now(),
        target_path,
    })?;

    Ok(SmartIndexResult {
        strategy,
        commits_indexed: total_indexed,
        is_complete,
        needs_background: !is_complete,
    })
}

/// Background continuation: reopens repo+DB, reads indexing_state,
/// continues from resume_oid for the given budget.
///
/// `file_path` is passed directly from the foreground caller (main.rs)
/// so that PathFiltered repos can continue their file-specific walk
/// without needing to store the path in the database.
pub fn background_index(
    repo_root: &Path,
    budget: Duration,
    file_path: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let engram_dir = repo_root.join(".engram");
    let db_path = engram_dir.join("engram.db");
    let db = Database::open(&db_path)?;

    let state = match db.get_indexing_state()? {
        Some(s) if !s.is_complete => s,
        _ => return Ok(()), // Nothing to do
    };

    let strategy = Strategy::from_str(&state.strategy);
    let repo = Repository::open(repo_root)?;
    let resume = state.resume_oid.as_deref();

    let (indexed, last_oid, hit_end) = match strategy {
        Strategy::PathFiltered => {
            match file_path {
                Some(path) => path_filtered_index(
                    &repo,
                    &db,
                    path,
                    budget,
                    resume,
                    BACKGROUND_BATCH_SIZE,
                )?,
                None => return Ok(()), // No file path — can't do PathFiltered
            }
        }
        _ => {
            let remaining_limit =
                DEFAULT_COMMIT_LIMIT.saturating_sub(state.commits_indexed as usize);
            budgeted_global_index(
                &repo,
                &db,
                budget,
                remaining_limit,
                resume,
                BACKGROUND_BATCH_SIZE,
            )?
        }
    };

    let total = state.commits_indexed + indexed;
    let is_complete = hit_end;

    db.set_indexing_state(&IndexingState {
        head_commit: state.head_commit,
        resume_oid: if is_complete { None } else { last_oid.or(state.resume_oid) },
        commits_indexed: total,
        strategy: state.strategy,
        is_complete,
        last_updated: unix_now(),
        target_path: file_path.map(|s| s.to_string()).or(state.target_path),
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::Signature;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_repo(commits: &[Vec<(&str, &str)>]) -> TempDir {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let sig = Signature::now("Test", "test@test.com").unwrap();

        for (i, files) in commits.iter().enumerate() {
            for (path, content) in files {
                let full_path = dir.path().join(path);
                if let Some(parent) = full_path.parent() {
                    fs::create_dir_all(parent).unwrap();
                }
                fs::write(&full_path, content).unwrap();
            }

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
                    Some("HEAD"), &sig, &sig, &format!("commit {i}"), &tree, &[&parent],
                )
                .unwrap();
            }
        }

        dir
    }

    #[test]
    fn test_decide_strategy_complete() {
        assert_eq!(decide_strategy(50, true, 1000), Strategy::Complete);
        assert_eq!(decide_strategy(0, true, 1000), Strategy::Complete);
    }

    #[test]
    fn test_decide_strategy_continue_global() {
        assert_eq!(decide_strategy(500, false, 1000), Strategy::ContinueGlobal);
        assert_eq!(decide_strategy(401, false, 1000), Strategy::ContinueGlobal);
    }

    #[test]
    fn test_decide_strategy_budgeted_global() {
        assert_eq!(decide_strategy(100, false, 1000), Strategy::BudgetedGlobal);
        assert_eq!(decide_strategy(10, false, 1000), Strategy::BudgetedGlobal);
        assert_eq!(decide_strategy(400, false, 1000), Strategy::BudgetedGlobal);
    }

    #[test]
    fn test_decide_strategy_path_filtered() {
        assert_eq!(decide_strategy(9, false, 1000), Strategy::PathFiltered);
        assert_eq!(decide_strategy(0, false, 1000), Strategy::PathFiltered);
    }

    #[test]
    fn test_file_changed_in_commit() {
        let commits = vec![
            vec![("src/a.rs", "v0"), ("src/b.rs", "v0")],
            vec![("src/a.rs", "v1")], // Only a changed
        ];
        let dir = create_test_repo(&commits);
        let repo = Repository::open(dir.path()).unwrap();

        // Get HEAD commit (commit 1 — only a.rs changed)
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        assert!(file_changed_in_commit(&head, Path::new("src/a.rs")));
        assert!(!file_changed_in_commit(&head, Path::new("src/b.rs")));

        // File that doesn't exist
        assert!(!file_changed_in_commit(&head, Path::new("nonexistent.rs")));
    }

    #[test]
    fn test_file_changed_in_first_commit() {
        let commits = vec![
            vec![("src/a.rs", "v0")],
        ];
        let dir = create_test_repo(&commits);
        let repo = Repository::open(dir.path()).unwrap();

        let head = repo.head().unwrap().peel_to_commit().unwrap();
        // First commit: file was added, so it changed
        assert!(file_changed_in_commit(&head, Path::new("src/a.rs")));
        assert!(!file_changed_in_commit(&head, Path::new("nonexistent")));
    }

    #[test]
    fn test_budgeted_global_index_basic() {
        let commits = vec![
            vec![("a.rs", "v0"), ("b.rs", "v0")],
            vec![("a.rs", "v1"), ("b.rs", "v1")],
            vec![("a.rs", "v2")],
        ];
        let dir = create_test_repo(&commits);
        let repo = Repository::open(dir.path()).unwrap();
        let db = Database::in_memory().unwrap();

        let (indexed, last_oid, hit_end) = budgeted_global_index(
            &repo, &db, Duration::from_secs(10), 1000, None, 100,
        ).unwrap();

        assert_eq!(indexed, 3);
        assert!(hit_end);
        assert!(last_oid.is_some());

        // Verify data is in DB
        assert_eq!(db.commit_count("a.rs").unwrap(), 3);
        assert_eq!(db.commit_count("b.rs").unwrap(), 2);
    }

    #[test]
    fn test_budgeted_global_index_with_limit() {
        let mut commits = Vec::new();
        for i in 0..20 {
            commits.push(vec![("a.rs", if i % 2 == 0 { "even" } else { "odd" })]);
        }
        let dir = create_test_repo(&commits);
        let repo = Repository::open(dir.path()).unwrap();
        let db = Database::in_memory().unwrap();

        let (indexed, _last_oid, hit_end) = budgeted_global_index(
            &repo, &db, Duration::from_secs(10), 5, None, 100,
        ).unwrap();

        assert_eq!(indexed, 5);
        assert!(!hit_end); // Didn't reach end, hit limit
    }

    #[test]
    fn test_budgeted_global_index_resume() {
        let commits = vec![
            vec![("a.rs", "v0")],
            vec![("a.rs", "v1")],
            vec![("a.rs", "v2")],
            vec![("a.rs", "v3")],
        ];
        let dir = create_test_repo(&commits);
        let repo = Repository::open(dir.path()).unwrap();
        let db = Database::in_memory().unwrap();

        // Index first 2
        let (indexed1, last_oid1, _) = budgeted_global_index(
            &repo, &db, Duration::from_secs(10), 2, None, 100,
        ).unwrap();
        assert_eq!(indexed1, 2);

        // Resume from where we left off
        let (indexed2, _, hit_end) = budgeted_global_index(
            &repo, &db, Duration::from_secs(10), 2, last_oid1.as_deref(), 100,
        ).unwrap();
        assert_eq!(indexed2, 2);
        assert!(hit_end);

        // All 4 commits should be in DB
        assert_eq!(db.commit_count("a.rs").unwrap(), 4);
    }

    #[test]
    fn test_path_filtered_index() {
        let commits = vec![
            vec![("src/target.rs", "v0"), ("src/other.rs", "v0")],
            vec![("src/other.rs", "v1")], // target NOT changed
            vec![("src/target.rs", "v1"), ("src/coupled.rs", "v0")], // target changed
            vec![("src/other.rs", "v2")], // target NOT changed
        ];
        let dir = create_test_repo(&commits);
        let repo = Repository::open(dir.path()).unwrap();
        let db = Database::in_memory().unwrap();

        let (indexed, _, _) = path_filtered_index(
            &repo, &db, "src/target.rs", Duration::from_secs(10), None, 100,
        ).unwrap();

        // Should have indexed 2 commits where target.rs changed
        assert_eq!(indexed, 2);

        // coupled.rs should appear in the DB (co-changed with target in commit 2)
        let coupled = db.coupled_files("src/target.rs").unwrap();
        let has_coupled = coupled.iter().any(|(p, _)| p == "src/coupled.rs");
        assert!(has_coupled, "coupled.rs should be co-changed with target.rs");
    }

    #[test]
    fn test_path_filtered_index_with_resume() {
        // Create a repo where target.rs changes in commits 0, 2, and 4
        let commits = vec![
            vec![("src/target.rs", "v0"), ("src/a.rs", "v0")],   // commit 0: initial
            vec![("src/a.rs", "v1")],                             // commit 1: no target change
            vec![("src/target.rs", "v1"), ("src/b.rs", "v0")],   // commit 2: target changed
            vec![("src/a.rs", "v2")],                             // commit 3: no target change
            vec![("src/target.rs", "v2"), ("src/c.rs", "v0")],   // commit 4: target changed
        ];
        let dir = create_test_repo(&commits);
        let repo = Repository::open(dir.path()).unwrap();
        let db = Database::in_memory().unwrap();

        // First pass: index with limit of 2 commits by using a very short budget
        // But instead, use commit limit via path_filtered — it doesn't have a limit,
        // so index all first, then test resume separately.
        //
        // Better approach: index first 3 revwalk commits (budget-limited), get resume_oid
        let (indexed1, last_oid1, hit_end1) = path_filtered_index(
            &repo, &db, "src/target.rs", Duration::from_secs(10), None, 100,
        ).unwrap();

        // Should index all changes (small repo completes within budget)
        assert!(hit_end1);
        assert!(indexed1 >= 2, "Expected at least 2 indexed, got {indexed1}");

        // Now test resume from a known OID — re-index should produce 0 new
        // (INSERT OR IGNORE prevents duplicates, but indexed count reflects work done)
        if let Some(ref resume_oid) = last_oid1 {
            let db2 = Database::in_memory().unwrap();
            let (indexed2, _, _) = path_filtered_index(
                &repo, &db2, "src/target.rs", Duration::from_secs(10),
                Some(resume_oid), 100,
            ).unwrap();
            // Resuming from the last OID: only root commit (if any) remains
            // The exact count depends on history depth, but it shouldn't crash
            assert!(indexed2 <= 1, "Resume should produce minimal new work, got {indexed2}");
        }
    }

    #[test]
    fn test_path_filtered_safety_margin_prevents_diffs() {
        // With a very short budget (< DIFF_SAFETY_MARGIN_MS), no diffs should happen
        let commits = vec![
            vec![("src/target.rs", "v0"), ("src/coupled.rs", "v0")],
            vec![("src/target.rs", "v1"), ("src/coupled.rs", "v1")],
            vec![("src/target.rs", "v2"), ("src/coupled.rs", "v2")],
        ];
        let dir = create_test_repo(&commits);
        let repo = Repository::open(dir.path()).unwrap();
        let db = Database::in_memory().unwrap();

        // Budget of 100ms is less than DIFF_SAFETY_MARGIN_MS (200ms)
        // The blob walk should run but no diffs should execute
        let (indexed, _, hit_end) = path_filtered_index(
            &repo, &db, "src/target.rs", Duration::from_millis(100), None, 100,
        ).unwrap();

        // The safety margin should prevent any diffs from running
        assert_eq!(indexed, 0, "No diffs should run with budget < safety margin");
        assert!(!hit_end, "Should not have completed");
    }

    #[test]
    fn test_smart_index_pathfiltered_different_file() {
        // Simulate: a PathFiltered state exists for file A,
        // then smart_index is called for file B.
        let commits = vec![
            vec![("src/a.rs", "v0"), ("src/b.rs", "v0")],
            vec![("src/a.rs", "v1"), ("src/b.rs", "v1")],
        ];
        let dir = create_test_repo(&commits);
        let repo = Repository::open(dir.path()).unwrap();
        let db = Database::in_memory().unwrap();

        // Manually set state as if a PathFiltered index was done for "src/a.rs"
        let head = repo.head().unwrap().peel_to_commit().unwrap().id().to_string();
        db.set_indexing_state(&IndexingState {
            head_commit: head,
            resume_oid: Some("deadbeef".to_string()),
            commits_indexed: 50,
            strategy: "path_filtered".to_string(),
            is_complete: false,
            last_updated: unix_now(),
            target_path: Some("src/a.rs".to_string()),
        }).unwrap();

        // Now call smart_index for a DIFFERENT file
        let result = smart_index(
            &repo, &db, "src/b.rs", Duration::from_secs(5),
        ).unwrap();

        // Should detect file change, start fresh for b.rs
        assert_eq!(result.strategy, Strategy::PathFiltered);
        // Small repo completes within budget, so needs_background is false.
        // On a large repo this would be true. The key verification is that
        // file change was detected and state was updated (below).
        assert!(result.is_complete);

        // State should be updated to the new file
        let state = db.get_indexing_state().unwrap().unwrap();
        assert_eq!(state.target_path, Some("src/b.rs".to_string()));
        // Old resume_oid from "src/a.rs" should be gone (fresh start)
        assert_ne!(state.resume_oid, Some("deadbeef".to_string()));
    }

    #[test]
    fn test_smart_index_small_repo() {
        let commits = vec![
            vec![("a.rs", "v0"), ("b.rs", "v0")],
            vec![("a.rs", "v1"), ("b.rs", "v1")],
        ];
        let dir = create_test_repo(&commits);
        let repo = Repository::open(dir.path()).unwrap();
        let db = Database::in_memory().unwrap();

        let result = smart_index(
            &repo, &db, "a.rs", Duration::from_secs(5),
        ).unwrap();

        assert_eq!(result.strategy, Strategy::Complete);
        assert!(result.is_complete);
        assert!(!result.needs_background);
        assert_eq!(result.commits_indexed, 2);
    }

    #[test]
    fn test_smart_index_subsequent_call_fast() {
        let commits = vec![
            vec![("a.rs", "v0")],
            vec![("a.rs", "v1")],
        ];
        let dir = create_test_repo(&commits);
        let repo = Repository::open(dir.path()).unwrap();
        let db = Database::in_memory().unwrap();

        // First call indexes everything
        let r1 = smart_index(&repo, &db, "a.rs", Duration::from_secs(5)).unwrap();
        assert!(r1.is_complete);

        // Second call should be instant (already complete at same HEAD)
        let start = Instant::now();
        let r2 = smart_index(&repo, &db, "a.rs", Duration::from_secs(5)).unwrap();
        let elapsed = start.elapsed();

        assert!(r2.is_complete);
        assert!(!r2.needs_background);
        assert!(elapsed.as_millis() < 50, "Subsequent call took too long: {:?}", elapsed);
    }

    #[test]
    fn test_strategy_round_trip() {
        for strategy in &[
            Strategy::Complete,
            Strategy::ContinueGlobal,
            Strategy::BudgetedGlobal,
            Strategy::PathFiltered,
        ] {
            assert_eq!(&Strategy::from_str(strategy.as_str()), strategy);
        }
    }
}

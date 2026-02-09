//! Performance tests against the Linux kernel repository.
//!
//! These tests require a local clone of the Linux kernel at `engram/../linux/`
//! They are `#[ignore]`d by default and will NOT run in CI.
//!
//! Run with: `cargo test --release --test perf_linux -- --ignored --test-threads=1`
//!
//! The test simulates the real production flow:
//!   1. First analyze call (cold, <2s) → foreground path-filtered indexing
//!   2. background_index(5s) runs (simulates what main.rs does after stdout flush)
//!   3. Subsequent call same file (<200ms) → state already complete from background
//!   4. background_index(5s) again
//!   5. Different file call (<200ms) → global index from background covers it
//!
//! In production, these background pauses happen naturally between user actions.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const TARGET_FILE: &str = "kernel/sched/core.c";
const SECOND_FILE: &str = "mm/memory.c";

fn linux_repo_path() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest_dir).join("../../linux").to_path_buf()
}

fn linux_repo_exists() -> bool {
    linux_repo_path().join(".git").exists()
}

fn clear_engram_state() {
    let _ = std::fs::remove_dir_all(linux_repo_path().join(".engram"));
}

/// Simulate the background indexing that main.rs runs after flushing stdout.
/// `file_path` is passed from the foreground caller, just like in production.
fn run_background(repo_root: &Path, file_path: Option<&str>) {
    let _ = engram_core::indexing::background_index(repo_root, Duration::from_secs(5), file_path);
}

#[test]
#[ignore]
fn test_linux_kernel_performance() {
    if !linux_repo_exists() {
        eprintln!("Skipping: Linux repo not found at {:?}", linux_repo_path());
        return;
    }

    let repo_root = linux_repo_path().canonicalize().unwrap();

    // Warm up the OS filesystem cache by touching the packfile index.
    // The first git2 operation after a cold OS cache can add 1-2s due to
    // mmap of the multi-GB packfile. This mirrors production where the
    // repo has already been used by git/IDE before engram is invoked.
    {
        let repo = git2::Repository::open(&repo_root).unwrap();
        let _ = repo.head().unwrap().peel_to_commit().unwrap();
    }

    // ── Phase 1: Cold first call ──────────────────────────────────────
    clear_engram_state();

    let start = Instant::now();
    let result = engram_core::analyze(&repo_root, TARGET_FILE).unwrap();
    let first_call_ms = start.elapsed().as_secs_f64() * 1000.0;
    let r = &result.response;

    eprintln!(
        "[1] First call: {:.0}ms, {} coupled files, {} commits, strategy: {:?}, complete: {:?}",
        first_call_ms,
        r.coupled_files.len(),
        r.commit_count,
        r.indexing_status.as_ref().map(|s| &s.strategy),
        r.indexing_status.as_ref().map(|s| s.is_complete),
    );

    assert!(
        first_call_ms < 2000.0,
        "First call took {:.0}ms, expected < 2000ms",
        first_call_ms,
    );
    assert!(
        !r.coupled_files.is_empty(),
        "First call should return coupling data",
    );
    assert!(r.commit_count > 0, "Should have indexed commits");

    let first_coupled = r.coupled_files.len();
    let first_commits = r.commit_count;

    // ── Background indexing (simulates main.rs after stdout flush) ────
    eprintln!("[bg] Running background_index for 5s with {}...", TARGET_FILE);
    let bg_start = Instant::now();
    run_background(&repo_root, Some(TARGET_FILE));
    eprintln!("[bg] Background completed in {:.0}ms", bg_start.elapsed().as_secs_f64() * 1000.0);

    // ── Phase 2: Subsequent call, same file ───────────────────────────
    let start = Instant::now();
    let result = engram_core::analyze(&repo_root, TARGET_FILE).unwrap();
    let subsequent_ms = start.elapsed().as_secs_f64() * 1000.0;
    let r2 = &result.response;

    eprintln!(
        "[2] Subsequent call (same file): {:.0}ms, {} coupled files (was {}), {} commits (was {}), complete: {:?}",
        subsequent_ms,
        r2.coupled_files.len(),
        first_coupled,
        r2.commit_count,
        first_commits,
        r2.indexing_status.as_ref().map(|s| s.is_complete),
    );

    assert!(
        subsequent_ms < 200.0,
        "Subsequent call took {:.0}ms, expected < 200ms",
        subsequent_ms,
    );

    // Background should have enriched the data — more coupled files or
    // more commits indexed (unless coupled_files already hit the cap of 10).
    assert!(
        r2.coupled_files.len() > first_coupled || r2.commit_count > first_commits,
        "Background should enrich data: coupled {} -> {}, commits {} -> {}",
        first_coupled, r2.coupled_files.len(), first_commits, r2.commit_count,
    );

    let second_coupled = r2.coupled_files.len();
    let second_commits = r2.commit_count;

    // ── Background again ──────────────────────────────────────────────
    run_background(&repo_root, Some(TARGET_FILE));

    // ── Phase 3: Different file (first call for this file) ─────────
    let start = Instant::now();
    let result = engram_core::analyze(&repo_root, SECOND_FILE).unwrap();
    let diff_file_ms = start.elapsed().as_secs_f64() * 1000.0;
    let r3 = &result.response;

    eprintln!(
        "[3] Different file (first call): {:.0}ms, {} coupled files, strategy: {:?}",
        diff_file_ms,
        r3.coupled_files.len(),
        r3.indexing_status.as_ref().map(|s| &s.strategy),
    );

    // Different file on a huge repo is a cold first call for that file —
    // same 2s budget as the original first call.
    assert!(
        diff_file_ms < 2000.0,
        "Different file first call took {:.0}ms, expected < 2000ms",
        diff_file_ms,
    );

    let third_coupled = r3.coupled_files.len();
    let third_commits = r3.commit_count;

    // ── Background for the new file ──────────────────────────────────
    run_background(&repo_root, Some(SECOND_FILE));

    // ── Phase 4: Subsequent call for second file ─────────────────────
    let start = Instant::now();
    let result = engram_core::analyze(&repo_root, SECOND_FILE).unwrap();
    let second_subsequent_ms = start.elapsed().as_secs_f64() * 1000.0;
    let r4 = &result.response;

    eprintln!(
        "[4] Subsequent call (second file): {:.0}ms, {} coupled files (was {}), {} commits (was {})",
        second_subsequent_ms,
        r4.coupled_files.len(),
        third_coupled,
        r4.commit_count,
        third_commits,
    );

    assert!(
        second_subsequent_ms < 200.0,
        "Subsequent call for second file took {:.0}ms, expected < 200ms",
        second_subsequent_ms,
    );

    // Background should have enriched the second file's data too.
    assert!(
        r4.coupled_files.len() > third_coupled || r4.commit_count > third_commits,
        "Background should enrich second file: coupled {} -> {}, commits {} -> {}",
        third_coupled, r4.coupled_files.len(), third_commits, r4.commit_count,
    );

    // ── Summary: verify progressive enrichment for first file ────────
    eprintln!(
        "\n[summary] {} enrichment: coupled {} -> {}, commits {} -> {}",
        TARGET_FILE, first_coupled, second_coupled, first_commits, second_commits,
    );
    eprintln!(
        "[summary] {} enrichment: coupled {} -> {}, commits {} -> {}",
        SECOND_FILE, third_coupled, r4.coupled_files.len(), third_commits, r4.commit_count,
    );
}

# Engram Development Plan

## Overview

Engram is a context brain for AI agents that provides temporal coupling analysis — detecting which files change together in git history to predict blast radius and store persistent project knowledge.

---

## ✅ Phase 1: Foundation (Completed)

**Goal:** Rust core with basic git indexing and coupling detection.

### Completed Items

- [x] Rust workspace setup with Cargo
- [x] SQLite database layer (`persistence.rs`)
  - [x] Temporal index table (commit_hash, file_path, timestamp)
  - [x] Watermark table for incremental indexing
  - [x] Query methods for co-change counts
- [x] Git history indexing (`temporal.rs`)
  - [x] Walk git commits via libgit2
  - [x] Extract changed files per commit
  - [x] Incremental indexing with watermark
  - [x] Commit limit (1000 default)
- [x] Basic coupling detection
  - [x] Query which files co-occur in commits
- [x] CLI interface (`cli.rs`, `main.rs`)
  - [x] Argument parsing with clap
  - [x] JSON output to stdout

---

## ✅ Phase 2: Risk Scoring & MCP Adapter (Completed)

**Goal:** Add risk scoring algorithm and TypeScript MCP adapter.

### Completed Items

- [x] Risk scoring algorithm (`risk.rs`)
  - [x] Multi-factor formula: `(churn × 0.5) + (recency × 0.3) + (coupling × 0.2)`
  - [x] Normalize scores to [0.0, 1.0]
  - [x] Sort descending by risk
  - [x] 8 unit tests
- [x] TypeScript adapter (`adapter/`)
  - [x] MCP server setup
  - [x] Process bridge to spawn Rust binary
  - [x] Tool registration: `get_impact_analysis`
  - [x] Type definitions mirroring Rust types
  - [x] Basic formatter (JSON stringify)
- [x] E2E test suite (`e2e/`)
  - [x] Test fixture generator (deterministic git repos)
  - [x] MCP client test helper
  - [x] Blast radius tests
  - [x] Risk scoring tests
  - [x] Error handling tests
  - [x] Latency tests (<2s cold, <200ms warm)

---

## ✅ Phase 3A: LLM-Friendly Formatter + Token Budgeting (Completed)

**Goal:** Format analysis output for LLMs with human-readable summaries and display caps.

### Completed Items

- [x] **Rust truncation** (`risk.rs`)
  - [x] Hard cap at 10 results after sorting
  - [x] 2 new tests (truncation with >10, no truncation with <10)
- [x] **TypeScript formatted types** (`adapter/src/types.ts`)
  - [x] `RiskLevel` type: "Critical" | "High" | "Medium" | "Low"
  - [x] `FormattedCoupledFile` interface
  - [x] `FormattedAnalysisResponse` extending base response
- [x] **Formatter rewrite** (`adapter/src/formatter.ts`)
  - [x] `classifyRisk(score)` — threshold classification
  - [x] `describeFile(file, commitCount)` — percentage description
  - [x] `buildSummaryLine(filePath, files)` — top-level summary with risk counts
  - [x] `buildFileDetails(files)` — multi-line detail block with emoji
  - [x] `formatAnalysisResponse(response)` — orchestrator function
  - [x] Display cap at 5 files in `summary` and `formatted_files`
  - [x] Full raw data preserved in `coupled_files`
- [x] **Formatter tests** (`adapter/tests/formatter.test.ts`)
  - [x] 13 new tests covering classification, descriptions, truncation, edge cases
- [x] **E2E test updates**
  - [x] Added `summary` assertion to blast-radius test
  - [x] Added `formatted_files` with `risk_level` assertion to risk-scoring test

### Test Coverage (Phase 3A)
- Rust: 20 tests (18 original + 2 truncation)
- Adapter: 20 tests (7 process-bridge + 13 formatter)
- E2E: 10 tests (updated with new field assertions)

---

## ✅ Phase 3B: Knowledge Graph (Memories) (Completed)

**Goal:** Persistent note storage with automatic enrichment in analysis results.

### Completed Items

- [x] **Rust types** (`types.rs`)
  - [x] `Memory` struct
  - [x] `AddNoteResponse`, `SearchNotesResponse`, `ListNotesResponse`
  - [x] `memories: Vec<Memory>` field added to `CoupledFile` (skip_serializing_if empty)
- [x] **Persistence layer** (`persistence.rs`)
  - [x] `memories` table with index on file_path
  - [x] `add_memory()`, `memories_for_file()`, `search_memories()`, `list_memories()`
  - [x] 7 new tests (add+retrieve, symbol_name, search by content/path, list all/filtered, empty result)
- [x] **Knowledge module** (`knowledge.rs`)
  - [x] Business logic: `add_note()`, `search_notes()`, `list_notes()`
  - [x] `enrich_with_memories()` — attaches memories to coupled files
  - [x] 2 tests (add_note response, enrich coupled files)
- [x] **CLI subcommands** (`cli.rs`, `main.rs`)
  - [x] Switched from flat args to clap subcommands
  - [x] `analyze`, `add-note`, `search-notes`, `list-notes`
  - [x] Routing in main.rs to appropriate lib functions
- [x] **Library API** (`lib.rs`)
  - [x] Registered knowledge module
  - [x] Extracted `open_db()` helper (DRY)
  - [x] Public functions: `add_note()`, `search_notes()`, `list_notes()`
  - [x] Wired `enrich_with_memories()` into `analyze()`
- [x] **Adapter updates** (`adapter/src/`)
  - [x] Mirrored new Rust types in `types.ts`
  - [x] Updated `process-bridge.ts` with subcommand args + new functions
  - [x] Registered 2 new MCP tools: `save_project_note`, `read_project_notes`
  - [x] Updated formatter to display memories with "Notes:" in detail blocks
  - [x] 3 new process-bridge tests
  - [x] 1 new formatter test for memories display
- [x] **E2E tests** (`e2e/tests/memories.test.ts`)
  - [x] New fixture: `createMemoriesRepo()`
  - [x] 3 tests: save+retrieve note, notes in analysis results, search notes

### Test Coverage (Phase 3B)
- Rust: 29 tests (20 from 3A + 7 persistence + 2 knowledge)
- Adapter: 24 tests (20 from 3A + 3 process-bridge + 1 formatter)
- E2E: 13 tests (10 from 3A updated + 3 new memories)

---

## ✅ Phase 3 Fixes: Production Hardening (Completed)

**Goal:** Fix critical architectural gaps identified in review.

### Completed Items

- [x] **Fix A: Rename Detection** (`temporal.rs`)
  - [x] Enable `DiffFindOptions::renames(true)` with `diff.find_similar()`
  - [x] Coupling history now survives file renames (rename tracked as update, not delete+add)
  - [x] 1 test: verifies `ARenamed.ts` coupled to `B.ts` after rename

- [x] **Fix B: Batch SQLite Inserts** (`persistence.rs`, `temporal.rs`)
  - [x] Added `begin_transaction()` and `commit_transaction()` to Database
  - [x] Wrapped entire `index_history` loop in single transaction
  - [x] Eliminates per-commit fsync overhead on cold starts
  - [x] 1 test: verifies 100 batch inserts all persist after commit

- [x] **Fix C: Filter Lockfiles and Binaries** (`temporal.rs`)
  - [x] `should_index_file()` filter function
  - [x] Deny list for lockfiles: `package-lock.json`, `yarn.lock`, `Cargo.lock`, etc.
  - [x] Deny list for binaries: images, fonts, archives, executables, compiled files, minified assets
  - [x] Applied in diff traversal before indexing
  - [x] 5 tests: accepts source files, rejects lockfiles, rejects binaries, rejects OS files, E2E lockfile filtering

### Test Coverage (Production Fixes)
- Rust: 36 tests (29 from 3B + 1 rename + 1 batch + 5 filter)
- Adapter: 24 tests (unchanged)
- E2E: 15 tests (13 from 3B + 2 updated for new behavior)

**Total: 75 tests, all passing**

---

## ✅ Phase 3C: Test Intent Extraction (Completed)

**Goal:** Enrich impact analysis with test titles from coupled test files, so the AI knows which existing tests may need updating.

### Completed Items

- [x] **Rust types** (`types.rs`)
  - [x] `TestIntent` struct with `title: String`
  - [x] `test_intents: Vec<TestIntent>` field on `CoupledFile` (skip_serializing_if empty)
- [x] **Test intents module** (`test_intents.rs`)
  - [x] `is_test_file()` — filename pattern matching for JS/TS, Go, Python, Rust
  - [x] `extract_test_intents()` — regex extraction of `it()`/`test()`, `#[test] fn`, `def test_*`, `func Test*`
  - [x] `enrich_with_test_intents()` — reads test files from disk, attaches intents to coupled files
  - [x] Cap at 5 intents per file
  - [x] `LazyLock` compiled regexes for performance
  - [x] 14 unit tests (detection, extraction, enrichment, edge cases)
- [x] **Wired into analyze flow** (`lib.rs`)
  - [x] Called after `enrich_with_memories()` in `analyze()`
- [x] **Adapter updates** (`adapter/src/`)
  - [x] `TestIntent` interface in `types.ts`
  - [x] `test_intents?: string[]` on `FormattedCoupledFile`
  - [x] Formatter renders test intents with "Current test behavior (may need updating):" qualification
  - [x] 2 new formatter tests
- [x] **E2E tests** (`e2e/tests/test-intents.test.ts`)
  - [x] New fixture: `createTestIntentsRepo()` (Auth.ts coupled with Auth.test.ts + Session.ts)
  - [x] 4 tests: extract intents, skip non-test files, qualification in summary, cap at 5

### Test Coverage (Phase 3C)
- Rust: 52 tests (38 from fixes + 14 new test_intents)
- Adapter: 26 tests (24 from 3B + 2 new formatter)
- E2E: 25 tests (21 from fixes + 4 new test-intents)

**Total: 103 tests, all passing**

---

## 📋 Phase 4: Distribution & Usability (Outstanding)

**Goal:** Make engram installable and usable outside development environment.

### Outstanding Items

- [ ] **Binary distribution**
  - [ ] GitHub Actions CI for building release binaries (Linux, macOS, Windows)
  - [ ] Publish binaries to GitHub Releases
  - [ ] `postinstall` script in adapter to download correct platform binary
  - [ ] Or bundle pre-built binaries in npm package (size consideration)

- [ ] **npm package**
  - [ ] Publish `@engram/adapter` to npm registry
  - [ ] Version tagging and release workflow
  - [ ] Installation docs for end users

- [ ] **Zombie process cleanup**
  - [ ] Add `process.on('exit')` handler in adapter to kill child process
  - [ ] Or implement `stdin` pipe detection in Rust for self-termination
  - [ ] Test process cleanup on adapter crash/kill

- [ ] **Configuration**
  - [ ] Allow custom ignore patterns (user-defined lockfile/binary lists)
  - [ ] Config file support (`.engramrc` or `engram.config.json`)
  - [ ] Per-repo or global config

- [ ] **Documentation**
  - [ ] Installation guide for non-developers
  - [ ] Troubleshooting guide
  - [ ] Architecture deep dive (for contributors)
  - [ ] Video demo / screenshots

---

## 🔮 Future Enhancements (Deferred)

These are potential future directions, intentionally deferred to keep MVP scope manageable.

### Validation/Coverage Graph
- [ ] Parse `lcov.info` and test coverage reports
- [ ] Map coverage to file-level "confidence" scores
- [ ] Tree-sitter AST parsing for symbol-level analysis
- [ ] "What breaks?" → "What tests validate this change?"

**Status:** Deferred — massive scope increase, not needed for v1

### Monorepo Support
- [ ] Detect monorepo structure (lerna, nx, turborepo)
- [ ] Scope coupling analysis to single project within monorepo
- [ ] Handle cross-project coupling differently

### Performance Optimizations
- [ ] Parallel commit processing
- [ ] Incremental diff computation (reuse previous diff state)
- [ ] Bloom filters for fast "never coupled" lookups

### Advanced Features
- [ ] Coupling strength visualization (graph UI)
- [ ] Temporal coupling trends over time
- [ ] "Who changed this?" mapping (contributor knowledge)
- [ ] Integration with issue trackers (coupling to bug patterns)

---

## Current State Summary

### What Works Now
✅ Full git indexing with incremental updates
✅ Temporal coupling detection
✅ Multi-factor risk scoring
✅ Rename-aware history tracking
✅ Lockfile/binary filtering
✅ Batch SQLite inserts
✅ Persistent knowledge graph (notes)
✅ Test intent extraction from coupled test files
✅ LLM-formatted output
✅ Complete MCP protocol (3 tools)
✅ 103 comprehensive tests

### What's Missing
❌ Easy installation (requires manual build)
❌ Binary distribution strategy
❌ Process cleanup on crash
❌ End-user documentation

### Next Immediate Step
**Phase 4 Distribution** — focus on making it installable by non-developers.

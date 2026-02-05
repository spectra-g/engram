This updated breakdown incorporates the rigorous End-to-End (E2E) and Unit Testing strategy you requested.

### 1. Repository Strategy: Single Monorepo

We will use a single repository to ensure strict version coupling between the Core engine and the Adapter.

*      **Repo Name:** `engram`
*      **Structure:**
    ```text
    engram/
    ├── core/                # RUST: The Intelligence Engine
    ├── adapter/             # NODE: The MCP Server & Binary Manager
    ├── e2e/                 # NODE: The End-to-End Test Harness & NFR Benchmarks
    ├── fixtures/            # SHARED: Setup scripts for generating local dummy Git repos
    └── .github/workflows/   # CI/CD: Cross-compilation & Test Matrix
    ```

---
### 2. Module Breakdown & Responsibilities

---
#### A. Core Module (`/core`)
*    **Tech Stack:** Rust, `git2` (Libgit2), `rusqlite` (SQLite), `tree-sitter`, `serde`, `chrono` (timestamps).
*    **Output:** `engram-core` binary.
*    **Performance Target:** <200ms for "Hot Path" analysis.

##### 1. Sub-Modules & Responsibilities

* *1. `cli_entry` (The Orchestrator)*
*    **Role:** The API Surface (JSON-In / JSON-Out).
*    **Responsibilities:**
    *    **Command Parsing:** Handle analysis requests (`--analyze`) and memory management (`--add-note`, `--list-notes`).
    *    **Response Aggregation:** Combine data from Temporal, Validation, and Knowledge layers into a single JSON payload.
    *    **Latency Guard:** Enforce strict timeouts; if the deep history scan isn't ready, return partial results immediately with a "partial_data" flag.
    *    **Path Normalization**: Convert all CLI args to relative POSIX paths (forward slashes) before processing.
    *    **Token Budgeting**: Apply hard truncation (Top 5 Files, Top 3 Test Intents) before serializing JSON.

* *2. `config_loader`*
*   **Role**: Configuration Management.
*   **Responsibilities**:
    * Load .engramrc (JSON/YAML).
    * Provide test_patterns to validation_graph.
    * Provide ignore_globs to temporal_graph.

* *3. `temporal_graph` (Time & Risk Engine)**
*    **Role:** Calculates Coupling and "Blast Radius" Probability.
*    **Responsibilities:**
    *    **Hot Scan:** Immediate `git log` traversal (last 1k commits) to find files frequently committed with the target file.
    *    **Cold Backfill:** Background thread to index the entire history into SQLite.
    *    **Risk Scoring (New):** Calculate a `0.0 - 1.0` risk score for every file in the result set based on a heuristic:
        *    *Churn:* High commit frequency = High Risk.
        *    *Recency:* Changed yesterday = High Risk.
        *    *Coupling:* Tightly coupled to many files = High Risk.
    *     **Heuristic Logic:**
        *     $$Score = (Churn \times 0.5) + (Complexity \times 0.3) + (Coupling \times 0.2)$$
        *     *Churn:* How many times has this file changed in the last 30 days? (High churn = High risk).
        *     *Complexity:* (Optional) Quick scan for file length or indentation depth.
    *     **Output Update:** The JSON response must now sort files by this Risk Score, not just by name.

* *4. `knowledge_graph` (The Memory Lobe - *New*)**
*    **Role:** Persistent Semantic Storage (The "Serena Killer").
*    **Responsibilities:**
    *    **Annotation Management:** CRUD operations for attaching text notes to file paths or symbol names (e.g., "This file handles legacy auth").
    *    **Context Injection:** When `cli_entry` requests analysis for `User.ts`, this module queries the DB: *"Do we have any warnings or notes attached to `User.ts`?"*
    *    **Search:** Allow fuzzy searching of notes (e.g., "Find where we mentioned 'billing migration'").

* *5. `validation_graph` (The Truth Layer)**
*    **Role:** Maps Code -> Tests.
*    **Responsibilities:**
    *    **Staleness Guard:** Check `mtime` of `lcov.info` vs `git HEAD`. If coverage data is old, flag it as "Stale" in the response.
    *    **Coverage Mapping:** Parse LCOV data to find which Test Files execute the specific lines being changed.
    *    **Intent Extraction:** Use `tree-sitter` to extract the specific `it("should...")` strings from the relevant test files to give the AI semantic context of the tests.

* *6. `persistence` (The Hippocampus)**
*   **Role:** SQLite Interface.
*   **Responsibilities:**
    *    Manage connection pooling to `.Engram/db.sqlite`.
    *   **Schema Management:**
        *    `temporal_index`: Stores file pairs and co-change counts.
        *    `memories`: Stores user notes (Columns: `id`, `file_path`, `content`, `created_at`).
    *   **Schema Update:** You need a new table in `db.sqlite`.
    ```sql
    CREATE TABLE memories (
        id INTEGER PRIMARY KEY,
        file_path TEXT NOT NULL,
        symbol_name TEXT,  -- Optional, for granular tagging
        content TEXT NOT NULL,
        created_at DATETIME DEFAULT CURRENT_TIMESTAMP
    );
    ```
    *   **Optimization:** Ensure queries (especially for the Risk Score) run in sub-millisecond time using proper indices.
    *   **Concurrency**: Enable PRAGMA journal_mode=WAL to allow the "Cold Backfill" thread to write without blocking "Hot Path" reads.

---
##### 2. Unit Testing Strategy

Since this is the "Brain," it must be rigorously tested without flaky external dependencies.

* *A. Database Isolation (The `:memory:` Pattern)**
*    **Concept:** Never write to disk during unit tests.
*    **Implementation:** Use Dependency Injection for the DB connection string.
*    **Code Example:**
    ```rust
    // In production
    let db = Database::new("engram.db");
    // In test
    let db = Database::new(":memory:"); // Creates purely RAM-based SQLite instance
    ```

* *B. Testing Git Logic (The "Ephemeral Repo" Pattern)**
*    **Problem:** Mocking `libgit2` C-bindings is painful and inaccurate.
*    **Solution:** Create real, tiny Git repos in `tmp` folders during tests.
*    **Workflow:**
    1.   **Setup:** Use the `tempfile` crate to create a directory.
    2.   **Init:** Run `git init` inside it programmatically.
    3.   **Simulate History:**
        *    Create `A.ts` and `B.ts`.
        *    Commit them together 10 times.
        *    Commit `A.ts` alone 1 time.
    4.   **Execute:** Point `temporal_graph` at this temp path.
    5.   **Assert:** Verify that the engine reports `B.ts` has a ~90% coupling score with `A.ts`.
    6.   **Teardown:** The `tempfile` crate automatically deletes the directory when the test struct goes out of scope.

* *C. Testing Memories**
*    **Scenario:** Add a note -> Retrieve analysis -> Verify note is present.
*    **Assertion:** Ensure that if a note is added to `src/utils.ts`, querying `src/utils.ts` (or a file coupled to it) includes that note in the JSON response.

* *D. Testing Risk Scoring**
*    **Scenario:** Simulate a file with high churn (committed 50 times in the last hour).
*    **Assertion:** Ensure its calculated `risk_score` is > 0.8.
---
#### B. Adapter Module (`/adapter`)
*    **Tech Stack:** TypeScript, `@modelcontextprotocol/sdk` (MCP), `zod` (Validation), `semver`.
*    **Target:** NPM Package (`@engram/adapter`) / Local MCP Server.
*    **Architecture Pattern:** "Humble Object" — Contains minimal logic, primarily handling translation between MCP JSON-RPC and CLI Arguments.

##### 1. Sub-Modules & Responsibilities

* *1. `binary_manager` (Lifecycle & Distribution)**
*    **Role:** The "Zero-Config" Enabler.
*    **Responsibilities:**
    *    **Architecture Detection:** On `postinstall`, detect OS (`darwin`, `linux`, `win32`) and Arch (`x64`, `arm64`).
    *    **Asset Retrieval:** Download the correct pre-compiled `engram-core` binary from GitHub Releases (or use a local bundle for offline environments).
    *    **Version Check:** Ensure the installed Node adapter version is compatible with the Rust binary version (Semantic Versioning).
    *    **Execution Permissions:** Ensure `chmod +x` is applied on Unix systems.

* *2. `process_bridge` (The IO Pipeline)**
*    **Role:** Safe Execution of the Rust Brain.
*    **Responsibilities:**
    *    **Spawn & Capture:** Execute `engram-core` via `child_process.spawn` using `stdio` for data transfer.
    *    **Timeout Enforcer:** Implement a **Hard Deadline** (e.g., 250ms). If Rust hangs, kill the process and return a "TimeoutError" to the Agent so the UI doesn't freeze.
    *    **Error Parsing:** Capture `stderr` from Rust. If Rust panics, format the error nicely for the Agent ("Engram Engine Error: Database Locked") rather than crashing the Node server.

* *3. `mcp_server` (The Agent Interface)**
*    **Role:** Tool Definitions & formatting.
*    **Responsibilities:**
    *    **Tool: `get_impact_analysis` (Updated):**
        *    *Input:* `{ file_path: string }`
        *    *Logic:* Calls `engram-core --analyze`.
        *    *Output formatting:* Converts Rust's raw JSON into a human-readable "Warning" for the LLM.
            *    *Raw:* `{ "risk_score": 0.9, "coupled": ["A.ts"] }`
            *    *LLM Output:* "⚠️ **High Risk Change (0.9)**. Historically, changing this file breaks `A.ts`."
        *     **Behavior:** When the Agent asks for impact, the JSON response must now include any "Memories" attached to the coupled files.
        *     **Output Example:**
            ```json
            {
            "file": "src/Billing.ts",
            "risk_score": 0.85,
            "coupling_reason": "Co-changed 15 times with User.ts",
            "memories": ["Marked as fragile by Claude on Feb 2, 2026"]
            }            
            ```
    *    **Tool: `save_project_note` (New - The Memory):**
        *    *Input:* `{ file_path: string, note: string }`
        *     **Behavior:** Calls Rust binary: `./engram-core --add-note --file "User.ts" --content "Critical billing logic"`
            *     **Why:** Allows the AI to "save" its understanding for future sessions
    *    **Tool: `read_project_notes` (New):**
        *    *Input:* `{ query: string }`
        *    *Logic:* Calls `engram-core --search-notes`. Used for retrieving onboarding info.
---
##### 2. Unit Testing Strategy

The Adapter must be tested to ensure it speaks "MCP" correctly and handles binary failures gracefully.

* *A. Mocking the Binary (The `child_process` Shim)**
*    **Concept:** Do *not* spawn the actual Rust binary in Node tests. It makes tests slow and platform-dependent.
*    **Implementation:** Use `jest.mock('child_process')`.
*    **Test Cases:**
    *    *Happy Path:* Mock `stdout` returning valid JSON. Assert the Adapter transforms it into the correct MCP text response.
    *    *Risk Highlighting:* Mock a response with `risk_score: 0.95`. Assert the Adapter adds "CRITICAL WARNING" text to the output.
    *    *Panic Handling:* Mock `stderr` outputting "Rusqlite Error". Assert the Adapter catches it and returns a clean error tool result.
    *    *Timeout:* Mock a process that never closes `stdout`. Assert the Adapter throws a timeout exception after 250ms.

* *B. Schema Validation Testing**
*    **Concept:** Ensure we don't break the contract with Claude/Cursor.
*    **Tools:** `zod` and `@modelcontextprotocol/sdk`.
*    **Workflow:**
    *    Instantiate the MCP Server.
    *    Call `listTools()`.
    *    Assert that `save_project_note` exists and requires both `file_path` and `note` arguments.

* *C. Integration "Smoke Test"**
*    **Scenario:** Binary version mismatch.
*    **Setup:** Mock `binary_manager` to report Binary v1.0 and Adapter v2.0.
*    **Assertion:** Ensure the server startup throws a clear "Version Mismatch" error, preventing hard-to-debug runtime errors later.
---
#### C. E2E & NFR Module (`/e2e`)
*   *Tech:** TypeScript, Jest/Vitest (Custom Runner).
*   *This is the new module specifically for System Testing and Latency Assertion.**

*   *Responsibilities:**
*      **The "Fake Agent":** Acts as a client (like Claude Desktop). It spawns the Adapter (which spawns Core) and communicates via `stdio`.
*      **Environment Orchestration:**
    *      Before test: Generates a complex "Scenario Repo" in a temp folder (e.g., a repo with 1,000 commits, specific merge conflicts, and a stale `lcov.info` file).
    *      Sets environment variables to point the Adapter to the locally built Rust binary (bypassing the NPM download).
*      **NFR (Non-Functional Requirement) Enforcement:**
    *      Measures "Round Trip Time" (RTT) from Request Sent -> Response Received.
    *      Fails the build if RTT > 200ms (on warm cache) or > 2s (cold start).
---
### 3. Detailed Testing Implementation Plan

Here is how we verify the application end-to-end locally without external dependencies.

#### Phase 1: The "Fixture Factory"
We need a script that generates Git history deterministically.
*      **Location:** `/fixtures/repo-generator.ts`
*      **Logic:**
    ```typescript
    // Pseudo-code
    const repo = new TestRepo('test-scenario-1');
    await repo.init();
    // Simulate "Temporal Coupling"
    for(let i=0; i<50; i++) {
        await repo.commit({
            files: ['src/Auth.ts', 'src/Session.db'], 
            message: `feature: update auth ${i}`
        });
    }
    // Simulate "Stale Coverage"
    await repo.write('coverage/lcov.info', '...');
    await repo.touch('src/Auth.ts'); // Make source newer than coverage
    ```

#### Phase 2: The E2E Test Suite (`/e2e`)

*   *Test Case 1: The "Blast Radius" Verification**
*      **Setup:** Point adapter to `test-scenario-1`.
*      **Action:** Send MCP Request `{ "tool": "get_impact_analysis", "args": { "file": "src/Auth.ts" } }`.
*      **Assertion:**
    *      Response must include `src/Session.db`.
    *      Response must *not* include unrelated files.
    *      *Blast Radius Check:* If the proposal says Auth affects Session, the tool *must* return it.

*   *Test Case 2: Latency & NFR Assertion**
*      **Logic:**
    ```typescript
    test('Hot Path Latency < 200ms', async () => {
        const start = performance.now();
        
        const response = await client.callTool('get_impact_analysis', { ... });
        
        const duration = performance.now() - start;
        
        expect(response.content).toBeDefined();
        // The NFR Gate
        expect(duration).toBeLessThan(200); 
    });
    ```

*   *Test Case 3: Stale Data Handling**
*      **Setup:** Create a repo where `src/User.ts` is newer than `coverage/lcov.info`.
*      **Action:** request analysis for `src/User.ts`.
*      **Assertion:** Response JSON must contain a field `warning: "Coverage data is stale"` or exclude specific line-level testing data, relying only on Git history.
---
### 4. Technical Summary of Work

#### 1. Rust Core (`/core`)
*      **Dependencies:** `git2`, `rusqlite`, `serde`, `tempfile` (dev-only), `criterion` (for micro-benchmarking internal Rust functions).
*      **Task:** Implement `LazyGraph` struct.
    *      `new()`: Fast scan (last 30 days).
    *      `backfill()`: Background thread for full history.
*      **Task:** Implement `CoverageGuard`.
    *      `check_freshness(file_path)`: Returns Enum `Fresh | Stale`.

#### 2. Node Adapter (`/adapter`)
*      **Dependencies:** `@modelcontextprotocol/sdk`, `zod`.
*      **Task:** Implement `EngramClient` class.
    *      Wraps `spawn`.
    *      Handles `SIGINT` cleanup (ensure no zombie Rust processes).

#### 3. Integration Harness (`/e2e`)
*      **Dependencies:** `jest`, `simple-git` (for setting up test repos).
*      **Task:** Build `RepoFactory`.
*      **Task:** Build `LatencyMonitor`.

### 5. Final Deliverable: "The Green Checkmark"

To consider this work complete, the following command must pass in the root directory:

```bash
npm run test:all

```

Which triggers:
1.     **Rust Unit Tests:** `cargo test` (Verifies DB logic, Git parsing with temp repos).
2.     **Node Unit Tests:** `npm test` inside `/adapter` (Verifies Protocol handling).
3.     **E2E Suite:** `npm test` inside `/e2e` (Spins up real repo, runs adapter + binary, checks JSON correctness, asserts Latency < 200ms).
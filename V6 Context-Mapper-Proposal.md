---
# Product Strategy Document: Engram (v5.1)

*    **Product Identity:** The "Blast Radius" Detector for AI Agents.
*    **Core Value:** Temporal Context & Regression Prevention.
*    **Architecture:** Local-First "Fat Client" (Node.js Thin Adapter + Rust JSON Engine).
*    **Deployment:** Zero-Config (NPM Package / Local Binary).
*    **Target Launch:** March 2026.
---
## 1. Problem Statement

### The "Amnesiac Genius" Paradox
AI Agents (Claude Desktop, OpenAI Operator, Cursor) are exceptionally good at generating code based on what they can *see*. However, they suffer from a critical "Blind Spot": **They lack Organizational Memory.**

When an Agent refactors `User.ts`, it sees the code definitions, but it does not know:
1.   **The Temporal History:** That `User.ts` has historically changed alongside `Billing.ts` 90% of the time.
2.   **The Validation Truth:** Which specific test files execute the lines being changed (especially if naming conventions are inconsistent).

### The Failure of RAG
Current tools rely on **Semantic Search (Vector RAG)**. This finds code that *looks similar* or shares keywords. It fails to find code that is **functionally coupled** but semantically distinct.
*    *Result:* The AI writes code that passes syntax checks but triggers "spooky action at a distance" regressions in other parts of the system.

### The Solution: Engram
Engram is a **Local Intelligence Engine** that runs alongside the Agent. Instead of just indexing *code*, it indexes *time* (Git History) and *truth* (Coverage Data) to predict the "Blast Radius" of any proposed change before the code is even written.
---
## 2. Business Objectives

### Product positioning
We are pivoting away from complex Enterprise integrations. Engram is a **"Power Tool"** for the individual Senior Engineer. It is designed to be as essential—and as easy to install—as `eslint` or `prettier`.

### Financial Goal
*    **Model:** Freemium "Pro Tool."
*    **Tier 1 (Free - OSS):** Basic Git Mining (Last 30 days/1,000 commits).
*    **Tier 2 (Pro License - $150/yr):** Deep History (All-time), Cross-Repo analysis, and Coverage Report integration.
*    **Target:** 1,000 active Pro users within 6 months.

### User Metrics
*    **Primary Metric:** **Regression Avoidance Rate** (Percentage of times a user opens a file suggested by Engram that was *not* in their initial prompt).
*    **Performance Constraint:** "Thought-Speed Latency." The Rust engine must return impact analysis in <200ms to keep the Agent fluid.
---
## 3. Strategic Analysis: The Three Pillars of Context

Engram provides three specific layers of intelligence that standard RAG misses:

### 1. The Temporal Graph (Time)
*    **Question:** *What usually breaks when this changes?*
*    **The Moat:** Most tools ignore time. We mine the `git log` to build a "Co-Change Matrix."
*    **Scenario:** If `Auth.ts` and `Session.db` have been committed together 15 times in the last year, Engram forces the Agent to check `Session.db` when editing `Auth.ts`.

### 2. The Validation Graph (Truth)
*    **Question:** *What behaviors must be preserved?*
*    **The Tech:** Coverage Parsing + Tree-sitter Spec Extraction.
*    **The Value:** We don't just tell the Agent which file tests the code; we extract the Test Intents.
*    **Scenario:** Engram reads `Billing.test.ts` and tells Claude: "Be careful. This code is required to 'handle negative balances gracefully' and 'retry on timeout'." This acts as a semantic guardrail.

### 3. The Structural Graph (Space)
*    **Question:** *Where are the definitions?*
*    **The Tech:** Lightweight `tree-sitter` parsing.
*    **Strategy:** We do not compete with huge language servers here. We use structure only to identify function names to query the Temporal/Validation graphs.
---
## 4. Technical Architecture: The "Sidecar" Model

To achieve high performance without requiring Docker or a cloud server, we utilize a **Local-First, Fat-Client Architecture**.

### Component A: The Orchestrator (Node.js / Thin Adapter)
*    **Role:** The Interface Layer.
*    **Tech:** TypeScript, Model Context Protocol (MCP) SDK.
*    **Design Philosophy:** **Protocol Agnostic.** This layer is a thin wrapper. Whether we connect via MCP, OpenAI Plugin, or VS Code Extension, the logic remains in the Core.
*    **Function:**
    *    Connects to Claude Desktop via `stdio`.
    *    Handles the conversation logic.
    *    Spawns and manages the Rust binary.

### Component B: The Engine (`Engram-core`)
*    **Role:** The Heavy Lifter.
*    **Tech:** **Rust**, `git2-rs` (Libgit2 bindings), `rusqlite` (SQLite), `lcov-parser`.
*    **Configuration:** Supports an `.engramrc` JSON file for defining custom test patterns (e.g., `["it", "test", "scenario", "parameterizedTest"]`) to handle enterprise wrappers.
*    **Data Storage:** A local, hidden SQLite file (`.Engram/db.sqlite`) inside the user's project root (git-ignored).
*    **JSON-In/JSON-Out:** The binary accepts a JSON payload and returns a JSON payload, ensuring zero dependency on specific AI model APIs.

### The Data Flow (Optimized for Latency)
1.   **Trigger:** User asks Claude: *"Refactor the `calculateTax` function in `Billing.ts`."*
2.   **Orchestrator:** Calls `./Engram-core --analyze src/Billing.ts --symbol calculateTax`.
3.   **Rust Engine:**
    *    **Git Scan (Cold Start Strategy):**
        *    *Fast Path:* Scans the last 1,000 commits immediately to find recent coupling. -> *Result: `TaxTables.json` (High recent correlation).*
        *    *Background Path:* If the DB is new, spawns a detached thread to index the full history (5+ years) without blocking the response.
    *    **Coverage Scan (Staleness Check):**
        *    Checks `mtime` of `coverage/lcov.info` vs `git HEAD`.
        *    *If Fresh:* Parses and maps Code -> Test File.
        *    *If Stale:* Returns a warning flag and falls back to Git history correlation only.
4.   **Ranking & Truncation:** To prevent "Context Flooding," the engine ranks results and truncates the output to the **Top 3 Coupled Files** and **Top 5 Relevant Test Names**.
5.   **Response:** Returns a JSON payload to Node.js.
6.   **Agent Context:** Engram injects a scoped warning: *"Warning: Changing `Billing.ts` historically affects `TaxTables.json`. Please review `tests/finance_integration.test.ts` (Behaviors: 'handle negative balances', 'retry on timeout') before finalizing."*
---
## 5. The 30-Day Roadmap

### Week 1: The Rust Engine (`Engram-core`)
*    **Goal:** A high-performance CLI that outputs file coupling with Lazy Indexing.
*    **Key Task:** Initialize the Rust project with `git2-rs`.
*    **Logic Update:** Implement **Lazy Indexing**:
    *    `scan()` immediately processes the last 30 days/1k commits (Time budget: <200ms).
    *    Trigger a background thread to backfill the SQLite DB with the full history.
*    **Deliverable:** A compiled binary that provides immediate insights on a 50k commit repo without hanging.

### Week 2: The Adapter Layer
*    **Goal:** Connect the Engine to Claude via MCP (but keep it modular).
*    **Key Task:** Build the Node.js wrapper using `child_process.spawn`.
*    **UX:** Define the MCP Tool `get_impact_analysis`.
*    **Architecture:** Ensure the Node code contains *no business logic*, only formatting JSON for the MCP protocol.

### Week 3: The Truth Layer (Coverage & Config)
*    **Goal:** Precise test identification, Stale Data handling, and Custom parsing.
*    **Key Task 1:** Add `lcov` parsing with a **Time-Check Guard**. If `lcov.info` is older than `HEAD`, return `coverage_status: "stale"`.
*    **Key Task 2:** Implement `.engramrc` parsing in Rust.
*    **Key Task 3:** Tree-sitter extraction:
    *    Load custom patterns from config.
    *    Extract test intent strings.
    *    Apply **Token Budgeting** (limit output to Top 5 behaviors).

### Week 4: Packaging & Distribution
*    **Goal:** Zero-friction install.
*    **Key Task:** Cross-platform compilation (GitHub Actions).
    *    Build binaries for `x86_64-unknown-linux-gnu`, `apple-darwin`, `pc-windows-msvc`.
*    **Distribution:** Publish to NPM. The `postinstall` script detects the OS and downloads the correct Rust binary.
*    **Launch:** "Zero to Context in 30 seconds."
---
## 6. Risk Assessment & Mitigation

| Risk | Impact | Mitigation Strategy |
| :--- | :--- | :--- |
| **"Sherlock" Risk (Cursor)** | Critical | Cursor may eventually add "Git History" embeddings. **Mitigation:** Position Engram as the agnostic brain for **Claude/Operator**, ensuring we are not locked into VS Code. |
| **Platform Lock-in** | High | Relying solely on MCP is risky if OpenAI creates a rival standard. **Mitigation:** The Rust Core is pure JSON-in/JSON-out. The MCP layer is just a disposable adapter. |
| **Performance (Monorepos)** | High | Initial scan on huge repos causes timeouts. **Mitigation:** **Lazy Indexing.** Instant results from recent history; deep history indexes in the background. |
| **Context Flooding** | Medium | Dumping too much info confuses the model. **Mitigation:** **Smart Truncation.** Hard limit of 3 files / 5 test behaviors per prompt. |
| **Stale Coverage Data** | Medium | Misleading the agent with old test lines. **Mitigation:** **Timestamp Guard.** Automatically disable coverage insights if the file is older than the last git commit. |
---
## 7. Final Verdict

By shifting to a **Local-First, Rust-backed architecture** and addressing the realities of large-scale engineering (stale data, huge git logs, custom test wrappers), Engram v5.1 represents a robust, production-ready specification. We are not just building a tool; we are building the "missing memory lobe" for AI coding agents.

* *Immediate Next Step:** Initialize the Rust workspace and benchmark `git2-rs` with the "Lazy Indexing" strategy on the React repository.
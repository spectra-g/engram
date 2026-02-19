# Engram

**The "Missing Context" Engine for AI Agents.**

Engram gives your AI agent the context it can’t see in the code alone.

While LLMs are excellent at analyzing the specific files you give them, they lack the broader context of your repository's history and guardrails. Engram bridges this gap by surfacing hidden dependencies (via git history) and required behaviours (via test intents) that the AI would otherwise not have access to, miss or ignore.

## Why Engram?

*    **Temporal History:** Answers *"What usually changes when this file changes?"* to prevent the "fix one thing, break another" cycle.
*    **Test Intent:** Extracts test intent strings (e.g., "should handle negative balance") so the AI understands what *behaviour* to preserve.
*    **Organizational Memory:** A persistent store for you or the LLM to record undocumented architectural constraints, ensuring lessons learned aren't lost when you start a new conversation.

### Built for Privacy. Public for Integrity.

*    **Local-First:** All processing happens on your local hardware.
*    **Zero Telemetry:** We do not track your usage, your code, or your identity.
*    **Audit it yourself:** The source code is available below.

## Real-World Example: The Bug That Tests Can't Catch

A TypeScript service (`TransactionExportService`) writes pipe-delimited lines like `TXN-001|2024-11-15|250.00|COMPLETED`. 

A legacy JavaScript cron job (`legacy-mainframe-sync.js`) parses them using **hardcoded array indices** - `parts[2]` for amount, `parts[3]` for status. 

There are zero imports between them. No shared types. Nothing in the code connects them.

**The task:** *"Add a `currency` field next to the amount."*

### Without Engram

The AI agent updates the TypeScript service and tests. The export format becomes `ID|DATE|AMOUNT|CURRENCY|STATUS`. All tests pass. The PR ships.

**The problem:** The legacy script still reads `parts[3]` expecting a status like `COMPLETED` - but now gets `USD`. `parseFloat("USD")` returns `NaN`. The mainframe receives corrupted data. Nothing failed. Nothing warned. Silent breakage in production.

### With Engram

Before writing any code, the agent calls `get_impact_analysis`. Engram checks git history and returns:

> **Critical Risk (0.99):** `bin/legacy-mainframe-sync.js` — Changed together in 21 of 21 commits (100%)

The agent reads the flagged file, finds the positional parser, and updates **both** files together. Same feature, zero breakage.

After the fix, the agent calls `save_project_note`:

> *"The export line format is consumed by bin/legacy-mainframe-sync.js using hardcoded positional indices. Any change to field order MUST be mirrored there. Current format: ID|DATE|AMOUNT|CURRENCY|STATUS (indices 0-4)."*

Now every future agent gets this warning automatically - before it writes a single line of code.

---

## What It Does

**1. Temporal Graph**
*    **What:** Mines git history to find files that are frequently committed alongside your target file.
*    **Why:** To reveal hidden dependencies. If `A.ts` and `B.ts` changed together 40 times in the last year, your AI needs to know about `B.ts` before editing `A.ts`.

**2. Validation Graph**
*    **What:** Automatically locates relevant tests and extracts their specific intent strings (e.g., `it("should validate JWT expiration")`).
*    **Why:** To provide behavioural guardrails. The AI can check its plan against your existing test requirements without needing to read the full test suite.
*    **Supported Frameworks:**
     *   **JS/TS:** Vitest, Jest, Mocha, Playwright, Cypress (`it`, `test`, `describe`)
     *   **JVM (Java/Kotlin/Scala):** JUnit 4, JUnit 5 (@DisplayName), Kotest, ScalaTest
     *   **Rust:** Native `#[test]`
     *   **Python:** Pytest, Unittest (`def test_...`)
     *   **Go:** Native `func Test...`

**3. Knowledge Graph**
*    **What:** A persistent store where the LLM can save/retrieve "memories" about architectural decisions, edge cases, or project quirks.
*    **Why:** To bridge the gap between sessions. If the AI learns that "Auth requires a restart on config change," it saves that note so the next AI agent knows it too.

## Tool calls

### 1. `get_impact_analysis` - Blast radius calculation for a target file

For a given file, return the impacted files, their test intents and any stored notes.

**Example:**

```json
{
  "file_path": "src/Auth.ts",
  "repo_root": "/path/to/repo"
}

```

**Returns:**

```json
{
  "summary": "Changing src/Auth.ts may affect 2 files. 1 critical risk, 1 medium risk.\n\n⚠️ Critical Risk (0.89): src/Session.ts\n   Changed together in 48 of 50 commits (96%)\n   Notes: Session requires Redis connection\n\n⚠ High Risk (0.72): src/Auth.test.ts\n   Changed together in 31 of 50 commits (62%)\n   Current test behaviour (may need updating):\n     - should login with valid credentials\n     - should reject invalid password\n     - should handle OAuth callback",
  "formatted_files": [
    {
      "path": "src/Session.ts",
      "risk_level": "Critical",
      "risk_score": 0.89,
      "description": "Changed together in 48 of 50 commits (96%)",
      "memories": ["Session requires Redis connection"]
    },
    {
      "path": "src/Auth.test.ts",
      "risk_level": "High",
      "risk_score": 0.72,
      "description": "Changed together in 31 of 50 commits (62%)",
      "test_intents": [
        "should login with valid credentials",
        "should reject invalid password",
        "should handle OAuth callback"
      ]
    }
  ],
  "coupled_files": [...],
  "commit_count": 50
}

```

### 2. `save_project_note` - Remember context about files

Store persistent notes that automatically appear in future impact analyses.

**Example:**

```json
{
  "file_path": "src/Auth.ts",
  "note": "Uses JWT tokens, must validate expiry timestamp",
  "repo_root": "/path/to/repo"
}

```

### 3. `read_project_notes` - Retrieve saved context

Search notes by content or file path, or list all project knowledge.

**Example:**

```json
{
  "query": "Redis",
  "repo_root": "/path/to/repo"
}

```

## Performance

Engram is built to be invisible until you need it. It uses an **Adaptive Indexing Strategy** that respects your CPU and scales from side-projects to massive monorepos.

### Benchmarked against the Linux Kernel
We take performance seriously. Engram is benchmarked against the [Linux Kernel](https://github.com/torvalds/linux) repository (**1.2 million+ commits**).

### Performance Targets

**Standard Repos (Most Projects)**
*    **First Run:** < 2 seconds (Full historical indexing)
*    **Subsequent Runs:** < 200ms

**Massive Repos (e.g., Linux Kernel)**
*    **First Run (per file):** < 2 seconds (Path-filtered indexing)
*    **Subsequent Runs:** < 200ms

## Architecture

```
┌─────────────┐
│ AI Agent    │ ← MCP protocol over stdio
└──────┬──────┘
       │
┌──────▼──────────────┐
│ Node.js Adapter     │ ← TypeScript MCP server
│ (adapter/)          │
└──────┬──────────────┘
       │ spawns & communicates via JSON
┌──────▼──────────────┐
│ Rust Core Binary    │ ← Fast git indexing + SQLite
│ (core/)             │
└──────┬──────────────┘
       │ reads
┌──────▼──────────────┐
│ .engram/engram.db   │ ← Persistent SQLite database
└─────────────────────┘

```

### Under the Hood
*    **Adaptive Strategy:** Engram automatically detects repo size. For small repos, it indexes everything. For massive repos, it switches to a path-filtered strategy to avoid blocking the agent.
*    **Low Footprint:** No heavy background daemons. Indexing happens on-demand within strict time budgets, utilizing `rusqlite` and WAL mode for high-throughput concurrency.
*    **Smart Filtering:** Automatically ignores noise like lockfiles, binary assets, and auto-generated code to keep the signal high.

## Setup

Engram is an [MCP server](https://modelcontextprotocol.io/) and works with any MCP-compatible client.

### Claude Code

```bash
claude mcp add --scope user --transport stdio engram -- npx -y @spectra-g/engram-adapter
```

### Cursor
Settings > General > MCP Servers > Add New MCP Server:
-  **Name:** `engram`
-  **Type:** `command`
-  **Command:** `npx -y @spectra-g/engram-adapter`
---

### System Instruction (Recommended)

To ensure your AI uses Engram effectively, add this to your project rules (`.cursorrules` or `CLAUDE.md`).

```markdown
## Engram Workflow Policy
You have access to a tool called `engram` (specifically `get_impact_analysis` and `save_project_note`).
You MUST follow this strictly sequential workflow for EVERY code modification request:

### Phase 1: Analysis (MANDATORY START)
1.  **Blast Radius Check**: Before reading code or proposing changes, you MUST call `get_impact_analysis` on the target file(s).
2.  **Context Loading**:
    *   **Coupling**: If "High" or "Critical" risk files are returned, evaluate if they are *functionally related*.
        *   *Action:* Read the file (`read_file`) if it poses a logical regression risk.
        *   *Ignore:* Skip files that appear coincidental (e.g., lockfiles, gitignore, bulk formatting updates).
    *   **Memories**: Pay close attention to any "Memories" returned in the analysis summary.
    *   **Tests**: If `test_intents` are present, treat them as strict behavioural constraints. If absent, proceed with standard code analysis.

### Phase 2: Execution
3.  **Fix/Refactor**: Proceed with the code changes. Update tests if the behaviour is intentionally changing.

### Phase 3: Knowledge Capture (MANDATORY END)
4.  **Save Learnings**: Before finishing, ask: *"Would a future developer be **surprised** by something I discovered?"*
    *   **IF YES** (Hidden dependencies, non-obvious bugs, env quirks): You MUST use `save_project_note`.
    *   **IF NO** (Typos, standard refactors, documented behaviour): Do NOT save a note.
```

## Development & Benchmarking

### Build from Source
Requires Rust (1.70+) and Node.js (18+).

```bash
npm run build:all    # Build Rust core + TypeScript adapter
npm run test:all     # Run standard test suite

```

### Performance Benchmarking
To verify performance against the Linux kernel (requires a local clone of `linux` as a sibling directory):

```bash
# 1. Clone linux kernel to ../linux
# 2. Run the ignored performance tests
npm run test:all-local

```

## Contributing

We welcome bug reports and community fixes. Please note that by contributing to this repository, you grant spectra-g a perpetual, irrevocable license to include your changes in both the public source and the commercially licensed versions of the software.

## License

This project is licensed under the **PolyForm Noncommercial License 1.0.0**.

*    **Personal/Non-Profit:** Free to use.
*    **Commercial Use:** Requires a commercial license.

[View License](LICENSE) | [Purchase Commercial License](https://engrampro.net)

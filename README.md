# Engram

**The "Missing Context" Engine for AI Agents.**

Engram gives your AI agent the context it can’t see in the code alone.

While LLMs are excellent at analyzing the specific files you give them, they lack the broader context of your repository's history and guardrails. Engram bridges this gap by surfacing hidden dependencies (via git history) and required behaviours (via test intents) that the AI would otherwise not have access to, miss or ignore.

## Why Engram?

*    **Temporal Coupling:** Answers *"What usually changes when this file changes?"* to prevent the "fix one thing, break another" cycle.
*    **Behavioural Guardrails:** extracts "Test Intents" (e.g., "should handle negative balance") so the AI understands *what* to preserve.
*    **Nuance Capture:** Provides a lightweight store for you or the LLM to record undocumented architectural constraints, ensuring lessons learned aren't lost when you start a new conversation.

### Built for Privacy. Public for Integrity.

*    **Local-First:** All processing happens on your local hardware.
*    **Zero Telemetry:** We do not track your usage, your code, or your identity.
*    **Audit it yourself:** The source code is available below.

## What It Does

**1. Temporal Analysis (Blast Radius)**
*    **What:** Instantly analyzes git history to find files that are frequently committed alongside your target file.
*    **Why:** To reveal hidden dependencies. If `A.ts` and `B.ts` changed together 40 times in the last year, your AI needs to know about `B.ts` before editing `A.ts`.

**2. Test Intent Discovery**
*    **What:** Automatically locates relevant tests and extracts their specific intent strings (e.g., `it("should validate JWT expiration")`).
*    **Why:** To provide immediate behavioural context. The AI can check its plan against your existing test requirements without needing to read the full test suite.

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

## Installation

To use Engram with your AI agent (Claude Desktop, Cursor, etc.), you need to expose the tool capabilities to them.

```bash
npm install -g @spectra-g/engram-adapter

```

This installs the necessary binary for your platform (macOS, Linux, Windows) and the MCP adapter that communicates with your AI.

## Setup

Engram is an [MCP server](https://modelcontextprotocol.io/) and works with any MCP-compatible client.

### Claude Desktop

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

# Engram

**The "Blast Radius" Detector for AI Agents.**

Engram gives your AI agent the one thing it lacks: **Organizational Memory.**

By analyzing your git history and project notes, Engram predicts what will break *before* the AI writes a single line of code. It detects files that are secretly coupled, even if they don't import each other directly, preventing the "fix one thing, break another" cycle.

## Built for Privacy. Public for Integrity.

This tool handles your most sensitive asset: your source code. To prove that your data never leaves your machine, the source code for Engram is publicly auditable.

### Data Integrity Guarantee

*   **Local-First:** All processing happens on your local hardware.
*   **Zero Telemetry:** We do not track your usage, your repository names, or your identity.
*   **No Phone-Home:** The application does not have a "backend" where your code is uploaded.
*   **Audit it yourself:** You can search this repository for any networking or `fetch`/`curl` calls.

## What It Does

**Temporal graph**
* Analyzes git history to find files that are frequently committed alongside your target file, ranked by risk score.
* **Why?** To predict what might break when you change a file.

**Test Intent**
* Automatically finds the specific tests associated with the files you are changing and reads their descriptions (e.g., "should handle negative balance").
* **Why?** So the AI understands what behaviour to preserve before making changes.

**Knowledge graph**

* Provides a persistence store where either you or the LLM can store/retrieve relevant notes concerning decisions, nuances, quirks, architecture etc
* **Why?** Lessons learnt aren't lost when you start a new conversation.

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

## How It Works

### Architecture

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

Engram is designed to be low-latency and zero-config.

1.  **Smart Indexing:** Engram scans your git history incrementally in the background. It automatically filters out "noise" (like `package-lock.json` or binary files) and tracks file renames to ensure the history is accurate.
2.  **The Risk Algorithm:** Engram calculates a **Risk Score (0-1)** based on **Coupling** (frequency) and **Recency** (how lately it happened).
3.  **Context Injection:** Finally, it combines these insights with your stored notes and relevant test names, formatting them into a concise summary that fits perfectly within the AI's context window.

### Performance

-  **Cold start** (first run): < 2 seconds (includes full git indexing)
-  **Warm path** (cached DB): < 200ms
-  All data stored locally in `.engram/engram.db` at the repo root

## Install

```bash
npm install -g @spectra-g/engram-adapter
```

The correct binary for your platform (macOS, Linux, Windows) is installed automatically.

## Setup

Engram is an [MCP server](https://modelcontextprotocol.io/) and works with any MCP-compatible client. Below are two examples - refer to your client's documentation for specifics.

### Claude Code

```bash
claude mcp add --scope user --transport stdio engram -- npx -y @spectra-g/engram-adapter
```

### Cursor

Settings > General > MCP Servers > Add New MCP Server:
- **Name:** `engram`
- **Type:** `command`
- **Command:** `npx -y @spectra-g/engram-adapter`

### Other Clients

Engram is MCP client-agnostic. Any client that supports the stdio transport can connect. If you need help with a specific client, please [open an issue](https://github.com/spectra-g/engram/issues) or contribute a setup guide.

### System Instruction (Recommended)

To make your AI use Engram automatically on every task, add this to your project rules file (`CLAUDE.md`, `.cursorrules`, etc.):

```markdown
## Engram Workflow Policy
You have access to a tool called `engram` (specifically `get_impact_analysis` and `save_project_note`).
You MUST follow this strictly sequential workflow for EVERY code modification request:

### Phase 1: Analysis (MANDATORY START)
1.  **Blast Radius Check**: Before reading code or proposing changes, you MUST call `get_impact_analysis` on the target file(s).
2.  **Context Loading**: If the analysis reveals "High" or "Critical" risk coupled files, you must read those files (`read_file`) to prevent regressions.
3.  **Review Notes**: Pay close attention to any "Memories" returned in the analysis summary.
4.  **Consider Test Coverage**: If the analysis includes `test_info` or `test_intents`, factor the existing test count and descriptions into your testing approach. Update existing tests if behavior is intentionally changing, and consider whether coverage is adequate for the change you're making.

### Phase 2: Execution
5.  **Fix/Refactor**: Proceed with the code changes.

### Phase 3: Knowledge Capture (MANDATORY END)
6.  **Save Learnings**: Before finishing, ask: *"Would a future developer be **surprised** by something I discovered?"*
    Save a note ONLY for **non-obvious** insights:
    - Hidden coupling between files that don't import each other
    - Surprising runtime behavior (e.g., "this function silently swallows errors")
    - Architectural constraints not evident from the code (e.g., "must deploy X before Y")
    - Environment-specific gotchas (e.g., "CI uses Node 18, which lacks this API")

    Do NOT save notes for:
    - Typo fixes, simple renames, or formatting changes
    - Bug fixes with obvious causes (e.g., off-by-one, null check)
    - Routine refactors where the code is self-explanatory
    - Changes that are already well-documented in comments or commit messages

    - **IF YES**: You MUST use `save_project_note` to persist this context for future sessions.
    - **IF NO**: Proceed to completion.

NEVER skip Step 1. NEVER skip Step 6 if valuable context was gained.
```

## Development

### Build from Source

Requires Rust (1.70+) and Node.js (18+).

```bash
npm run build:all    # Build Rust core + TypeScript adapter
npm run test:all     # Run all tests (Rust + adapter + E2E)
```

## Contributing

We welcome bug reports and community fixes. Please note that by contributing to this repository, you grant spectra-g a perpetual, irrevocable license to include your changes in both the public source and the commercially licensed versions of the software.

## License & Commercial Use

This project is licensed under the **PolyForm Noncommercial License 1.0.0**.

### Personal & Non-Profit Use
If you are using this for personal hobby projects, non-profit organizations, or educational research, it is completely free. We encourage you to audit the source code to see how we handle your data.

### Professional & Commercial Use
If you are using this tool in a for-profit workplace (at your job, for your company, or as a freelancer for a client), a commercial license is required.

This includes work on commercial open-source projects. If you are paid to write code, you need a license.

Using this tool for commercial work without a license requires a commercial license agreement.

[Purchase a Commercial License](https://engrampro.net/#pricing)

[View the Full License Text](LICENSE)

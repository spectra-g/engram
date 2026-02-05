# Engram

**The "Blast Radius" Detector for AI Agents.**

Engram gives your AI agent (Claude, Cursor, etc.) the one thing it lacks: **Organizational Memory.**

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
* **Why?** So the AI understands what behavior to preserve before making changes.

**Knowledge graph**

* Provides a persistence store where either you or the LLM can store/retrieve relevant notes concerning decisions, nuances, quirks, architecture etc
* **Why?** Lessons learnt aren't lost when you start a new conversation.

**Tool calls**

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
  "summary": "Changing src/Auth.ts may affect 2 files. 1 critical risk, 1 medium risk.\n\n⚠️ Critical Risk (0.89): src/Session.ts\n   Changed together in 48 of 50 commits (96%)\n   Notes: Session requires Redis connection\n\n⚠ High Risk (0.72): src/Auth.test.ts\n   Changed together in 31 of 50 commits (62%)\n   Current test behavior (may need updating):\n     - should login with valid credentials\n     - should reject invalid password\n     - should handle OAuth callback",
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
2.  **The Risk Algorithm:** We don't just count commits. Engram calculates a **Risk Score (0-1)** based on **Coupling** (frequency) and **Recency** (how lately it happened).
3.  **Context Injection:** Finally, it combines these insights with your stored notes and relevant test names, formatting them into a concise summary that fits perfectly within the AI's context window.

### Performance

-  **Cold start** (first run): < 2 seconds (includes full git indexing)
-  **Warm path** (cached DB): < 200ms
-  All data stored locally in `.engram/engram.db` at the repo root

## Installation & Usage

### Prerequisites

-  Rust (1.70+)
-  Node.js (18+)
-  Git repository

### Build

```bash
# Build the Rust core
npm run build:core

# Build the TypeScript adapter
npm run build:adapter

# Or build both
npm run build:all

```

### Running Tests

```bash
# Run all tests (Rust + adapter + E2E)
npm run test:all

# Or run individually
npm run test:core      # Rust unit tests
npm run test:adapter   # TypeScript unit tests
npm run test:e2e       # End-to-end integration tests

```

### Using with MCP Clients

#### 1. For Claude Code (CLI)

Register Engram with Claude Code, add to your config (`~/.claude.json` on macOS):

```json
{
  "mcpServers": {
    "engram": {
      "command": "node",
      "args": ["/path/to/engram/adapter/dist/index.js"],
      "env": {
        "ENGRAM_CORE_BINARY": "/path/to/engram/target/release/engram-core"
      }
    }
  }
}
```
To make the AI use Engram automatically, you must give it a "System Instruction."

1. Open (or create) the file `CLAUDE.md` in your project root.
2. Paste the following rule exactly:

```markdown
## Engram Workflow Policy
You have access to a tool called `engram` (specifically `get_impact_analysis` and `save_project_note`).
You MUST follow this strictly sequential workflow for EVERY code modification request:

### Phase 1: Analysis (MANDATORY START)
1.  **Blast Radius Check**: Before reading code or proposing changes, you MUST call `get_impact_analysis` on the target file(s).
2.  **Context Loading**: If the analysis reveals "High" or "Critical" risk coupled files, you must read those files (`read_file`) to prevent regressions.
3.  **Review Notes**: Pay close attention to any "Memories" returned in the analysis summary.

### Phase 2: Execution
4.  **Fix/Refactor**: Proceed with the code changes.

### Phase 3: Knowledge Capture (MANDATORY END)
5.  **Save Learnings**: Before finishing, ask: *"Did I discover a hidden dependency, a tricky bug cause, or an architectural quirk?"*
    - **IF YES**: You MUST use `save_project_note` to persist this context for future sessions.
    - **IF NO**: Proceed to completion.

NEVER skip Step 1. NEVER skip Step 5 if valuable context was gained.
```

Then restart Claude Code. The tools will be available to Claude.

#### 2. For Cursor
To get Engram running in Cursor, you can add it via the Cursor Settings. Here is how you do it:

1. Open MCP Settings
2. Open Cursor and go to Settings (the gear icon in the top right, or Cmd + Shift + J on macOS / Ctrl + Shift + J on Windows).
3. Navigate to General > MCP Servers.
4. Add the New Server
5. Click on "+ Add New MCP Server" and fill in the details based on your config:
   1. Name: `engram`
   2. Type: `command`
   3. Command: `ENGRAM_CORE_BINARY=/path/to/engram/target/release/engram-core node /path/to/engram/adapter/dist/index.js`

To make the AI use Engram automatically, you must give it a "System Instruction."

1. Create a file named `.cursorrules` in your project root.
2. Paste the exact same text block shown above (from the Claude section) into this file.

*Note: Without this file, the AI will likely be "lazy" and skip the analysis step to save time.*

#### Other MCP Clients

Any MCP-compatible client can connect to the adapter over stdio:

```javascript
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";

const client = new Client({ name: "my-client", version: "1.0.0" });
const transport = new StdioClientTransport({
  command: "node",
  args: ["/path/to/engram/adapter/dist/index.js"],
  env: {
    ENGRAM_CORE_BINARY: "/path/to/engram/target/release/engram-core"
  }
});

await client.connect(transport);
const result = await client.callTool({
  name: "get_impact_analysis",
  arguments: { file_path: "src/Auth.ts", repo_root: "/path/to/repo" }
});

```

## Development Status

### Planned Future Work

-  Distribution strategy (npm package + binary downloads)
-  Zombie process cleanup on adapter crash
-  LCOV / Full Code Coverage Integration (Deep validation)
-  Support for monorepos (multiple projects in one repo)
-  Configurable ignore patterns (custom lockfile/binary lists)

## Testing Strategy

The project uses a rigorous testing approach:

-  **Rust unit tests** - Test individual functions in isolation
-  **Adapter unit tests** - Mock the Rust binary, test TypeScript logic
-  **E2E tests** - Generate real git repositories with deterministic commit histories, run full analysis cycles
- **Performance tests** - Confirm all flows work to max 200ms latency

All tests run in CI via `npm run test:all`.

## Contributing

We welcome bug reports and community fixes. Please note that by contributing to this repository, you grant spectra-g a perpetual, irrevocable license to include your changes in both the public source and the commercially licensed versions of the software.

## License & Commercial Use

This project is licensed under the **PolyForm Noncommercial License 1.0.0**.

### Personal & Non-Profit Use
If you are using this for **personal hobby projects**, **non-profit organizations**, or **educational research**, it is completely free. We encourage you to audit the source code to see how we handle your data.

### Professional & Commercial Use
If you are using this tool in a **for-profit workplace** (at your job, for your company, or as a freelancer for a client), **a commercial license is required**.

**This includes work on commercial open-source projects.** If you are paid to write code, you need a license.

Using this tool for commercial work without a license requires a commercial license agreement.

**[Purchase a Commercial License](https://engrampro.net/#pricing)**

**[View the Full License Text](LICENSE)**

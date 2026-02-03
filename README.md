# Engram

**Context brain for your Agentic flow**

Engram helps AI agents understand the blast radius of code changes by analyzing git commit history to detect temporal coupling between files. When files are frequently changed together, it's a signal they're related - even if there's no direct code dependency.

## What It Does

Engram provides three MCP tools that agents can call:

### 1. `get_impact_analysis` - Predict what breaks when you change a file

Analyzes git history to find files that are frequently committed alongside your target file, ranked by risk score. Automatically extracts test titles from coupled test files to show which existing tests may need updating.

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

### The Analysis Flow

1. **Git Indexing** - Walks up to 1000 recent commits, storing which files changed together in each commit
   - Incremental (watermark-based) - only indexes new commits on subsequent runs
   - Filters out lockfiles (`package-lock.json`, `Cargo.lock`, etc.) and binaries to reduce noise
   - Detects file renames to preserve coupling history

2. **Risk Scoring** - Each coupled file gets a 0-1 risk score based on:
   - **Coupling** (50% weight) - What % of target's commits include this file
   - **Churn** (30% weight) - How actively the file is modified
   - **Recency** (20% weight) - How recently it was changed with the target
   - **Coupling gate**: Files with <50% coupling cannot be classified as "Critical" (capped at High risk max)

3. **Knowledge Graph** - Notes saved via `save_project_note` are stored in SQLite and automatically attached to coupled files in analysis results

4. **Test Intent Extraction** - When a coupled file is a test file (detected via filename patterns), Engram extracts test titles using regex:
   - **JS/TS**: `it('...')` and `test('...')` blocks
   - **Rust**: `#[test] fn test_name`
   - **Python**: `def test_name(`
   - **Go**: `func TestName(`
   - Caps at 5 test titles per file to stay within token budgets
   - Presented to the AI as "Current test behavior (may need updating)" with a qualification warning

5. **LLM Formatting** - Results include both structured data and human-readable summaries with emoji risk indicators

### Performance

- **Cold start** (first run): < 2 seconds (includes full git indexing)
- **Warm path** (cached DB): < 200ms
- All data stored locally in `.engram/engram.db` at the repo root

## Installation & Usage

### Prerequisites

- Rust (1.70+)
- Node.js (18+)
- Git repository

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

#### Claude Desktop

Add to your Claude Desktop MCP config (`~/Library/Application Support/Claude/claude_desktop_config.json` on macOS):

```json
{
  "mcpServers": {
    "engram": {
      "command": "node",
      "args": ["/path/to/engram/adapter/dist/index.js"],
      "env": {
        "ENGRAM_CORE_BINARY": "/path/to/engram/core/target/release/engram-core"
      }
    }
  }
}
```

Then restart Claude Desktop. The three tools will be available to Claude.

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
    ENGRAM_CORE_BINARY: "/path/to/engram/core/target/release/engram-core"
  }
});

await client.connect(transport);
const result = await client.callTool({
  name: "get_impact_analysis",
  arguments: { file_path: "src/Auth.ts", repo_root: "/path/to/repo" }
});
```

## Project Structure

```
engram/
├── core/                 # Rust binary (git indexing, SQLite, risk scoring)
│   ├── src/
│   │   ├── main.rs      # CLI routing
│   │   ├── lib.rs       # Public API
│   │   ├── temporal.rs  # Git history indexing
│   │   ├── persistence.rs # SQLite database layer
│   │   ├── risk.rs      # Risk scoring algorithm
│   │   ├── knowledge.rs # Memory (notes) business logic
│   │   ├── test_intents.rs # Test title extraction from coupled test files
│   │   ├── types.rs     # Core data structures
│   │   └── cli.rs       # CLI argument parsing
│   └── Cargo.toml
├── adapter/              # TypeScript MCP adapter
│   ├── src/
│   │   ├── index.ts     # Entry point
│   │   ├── mcp-server.ts # Tool registration
│   │   ├── process-bridge.ts # Spawns Rust binary
│   │   ├── formatter.ts # LLM-friendly output formatting
│   │   └── types.ts     # TypeScript type definitions
│   └── package.json
├── e2e/                  # End-to-end integration tests
├── fixtures/             # Test repository generators
└── package.json          # Root workspace scripts
```

## Development Status

### ✅ Completed (Phase 3A, 3B & 3C)

- Git history indexing with incremental updates
- Temporal coupling detection
- Multi-factor risk scoring (churn + recency + coupling)
- Rename detection (coupling history survives file renames)
- Lockfile and binary filtering
- Batch SQLite inserts for cold-start performance
- Knowledge graph (persistent notes)
- Test intent extraction (extracts test titles from coupled test files)
- LLM-friendly formatted output with risk classification
- Token budgeting (top 5 display, top 10 hard cap)
- Full MCP protocol implementation
- Comprehensive test coverage (103 tests: 52 Rust + 26 adapter + 25 E2E)
- Tuned risk scoring: coupling-first weighting with gate to prevent low-coupling files from being marked Critical

### 📋 Planned Future Work

- Distribution strategy (npm package + binary downloads)
- Zombie process cleanup on adapter crash
- Validation/coverage graph (deferred - scope too large for MVP)
- Support for monorepos (multiple projects in one repo)
- Configurable ignore patterns (custom lockfile/binary lists)

## Testing Strategy

The project uses a rigorous testing approach:

- **Rust unit tests** - Test individual functions in isolation
- **Adapter unit tests** - Mock the Rust binary, test TypeScript logic
- **E2E tests** - Generate real git repositories with deterministic commit histories, run full analysis cycles

All tests run in CI via `npm run test:all`.

## Contributing

This is an experimental project focused on helping AI agents understand codebases through temporal coupling analysis. Contributions welcome.

## License

MIT

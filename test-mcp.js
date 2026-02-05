#!/usr/bin/env node

/**
 * Interactive MCP client for testing engram locally.
 *
 * Usage:
 *   node test-mcp.js
 *
 * Then follow the prompts to test the MCP tools.
 */

import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import { fileURLToPath } from "url";
import { dirname, resolve } from "path";
import readline from "readline";

const __dirname = dirname(fileURLToPath(import.meta.url));

async function main() {
  console.log("🧠 Engram MCP Test Client\n");

  // Set up client
  const client = new Client({
    name: "engram-test-client",
    version: "1.0.0",
  });

  const transport = new StdioClientTransport({
    command: "node",
    args: [resolve(__dirname, "adapter/dist/index.js")],
    env: {
      ...process.env,
      ENGRAM_CORE_BINARY: resolve(__dirname, "target/release/engram-core"),
    },
  });

  console.log("Connecting to MCP server...");
  await client.connect(transport);
  console.log("✓ Connected\n");

  // List available tools
  const tools = await client.listTools();
  console.log("Available tools:");
  tools.tools.forEach((tool) => {
    console.log(`  - ${tool.name}: ${tool.description}`);
  });
  console.log();

  // Interactive prompt
  const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout,
  });

  function prompt(question) {
    return new Promise((resolve) => rl.question(question, resolve));
  }

  while (true) {
    console.log("\nWhat would you like to test?");
    console.log("  1. get_impact_analysis");
    console.log("  2. save_project_note");
    console.log("  3. read_project_notes");
    console.log("  4. get_usage_metrics");
    console.log("  5. Exit");

    const choice = await prompt("\nChoice (1-5): ");

    if (choice === "5") {
      console.log("\nGoodbye!");
      rl.close();
      process.exit(0);
    }

    try {
      if (choice === "1") {
        const filePath = await prompt("File path (relative to repo): ");
        const repoRoot = await prompt("Repo root (absolute path): ");

        console.log("\n⏳ Analyzing...");
        const result = await client.callTool({
          name: "get_impact_analysis",
          arguments: { file_path: filePath, repo_root: repoRoot },
        });

        console.log("\n✓ Result:");
        const data = JSON.parse(result.content[0].text);
        console.log(`\nFile: ${data.file_path}`);
        console.log(`Commits analyzed: ${data.commit_count}`);
        console.log(`Analysis time: ${data.analysis_time_ms}ms`);
        console.log(`\n${data.summary}`);

        if (data.coupled_files.length > 0) {
          console.log(`\nRaw coupled files (${data.coupled_files.length}):`);
          data.coupled_files.slice(0, 10).forEach((f) => {
            console.log(
              `  - ${f.path} (risk: ${f.risk_score.toFixed(2)}, coupling: ${f.coupling_score.toFixed(2)})`
            );
          });
        }
      } else if (choice === "2") {
        const filePath = await prompt("File path: ");
        const note = await prompt("Note content: ");
        const repoRoot = await prompt("Repo root: ");

        console.log("\n⏳ Saving note...");
        const result = await client.callTool({
          name: "save_project_note",
          arguments: { file_path: filePath, note, repo_root: repoRoot },
        });

        console.log("\n✓ Saved:");
        console.log(result.content[0].text);
      } else if (choice === "3") {
        const hasQuery = await prompt("Search by query? (y/n): ");

        let args = { repo_root: await prompt("Repo root: ") };

        if (hasQuery.toLowerCase() === "y") {
          args.query = await prompt("Search query: ");
        } else {
          const hasFile = await prompt("Filter by file? (y/n): ");
          if (hasFile.toLowerCase() === "y") {
            args.file_path = await prompt("File path: ");
          }
        }

        console.log("\n⏳ Reading notes...");
        const result = await client.callTool({
          name: "read_project_notes",
          arguments: args,
        });

        console.log("\n✓ Notes:");
        const data = JSON.parse(result.content[0].text);
        if (data.memories && data.memories.length > 0) {
          data.memories.forEach((m) => {
            console.log(`\n  [${m.file_path}]`);
            if (m.symbol_name) console.log(`  Symbol: ${m.symbol_name}`);
            console.log(`  ${m.content}`);
            console.log(`  (${m.created_at})`);
          });
        } else {
          console.log("  No notes found.");
        }
      } else if (choice === "4") {
        const repoRoot = await prompt("Repo root: ");

        console.log("\n⏳ Fetching usage metrics...");
        const result = await client.callTool({
          name: "get_usage_metrics",
          arguments: { repo_root: repoRoot },
        });

        console.log("\n✓ Usage Metrics:\n");

        // Handle both JSON and raw text responses
        let data;
        try {
          data = JSON.parse(result.content[0].text);
        } catch (e) {
          console.log("Raw response:", result.content[0].text);
          throw new Error("Failed to parse response as JSON");
        }

        console.log(`Repository: ${data.repo_root}`);
        console.log();

        const summary = data.summary;
        console.log("Analysis Metrics:");
        console.log(`  Total analyses: ${summary.total_analyses}`);
        console.log(`  Total coupled files: ${summary.total_coupled_files}`);
        console.log(`  Avg analysis time: ${summary.avg_analysis_time_ms}ms`);

        console.log("\nNotes Metrics:");
        console.log(`  Notes created: ${summary.notes_created}`);
        console.log(`  Searches performed: ${summary.searches_performed}`);
        console.log(`  Lists performed: ${summary.lists_performed}`);

        console.log("\nRisk Distribution:");
        console.log(`  Critical: ${summary.critical_risk_count}`);
        console.log(`  High: ${summary.high_risk_count}`);
        console.log(`  Medium: ${summary.medium_risk_count}`);
        console.log(`  Low: ${summary.low_risk_count}`);

        console.log("\nTest Intent Extraction:");
        console.log(`  Test files found: ${summary.test_files_found}`);
        console.log(`  Test intents extracted: ${summary.test_intents_extracted}`);
      } else {
        console.log("Invalid choice.");
      }
    } catch (error) {
      console.error("\n❌ Error:", error.message);
    }
  }
}

main().catch((error) => {
  console.error("Fatal error:", error);
  process.exit(1);
});

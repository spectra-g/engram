import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { McpTestClient } from "./helpers/mcp-client.js";
import { createMemoriesRepo } from "../../fixtures/src/scenarios/memories.js";
import { CORE_BINARY_PATH } from "./setup.js";
import { rmSync } from "node:fs";

describe("memories: knowledge graph", () => {
  let client: McpTestClient;
  let repoDir: string;

  beforeAll(async () => {
    repoDir = createMemoriesRepo();
    client = new McpTestClient();
    await client.connect({ coreBinaryPath: CORE_BINARY_PATH });
  });

  afterAll(async () => {
    await client.close();
    rmSync(repoDir, { recursive: true, force: true });
  });

  it("should save a note and retrieve it via list", async () => {
    // Save a note
    const saveResult = await client.callTool("save_project_note", {
      file_path: "src/Auth.ts",
      note: "Auth handles JWT tokens and OAuth flow",
      repo_root: repoDir,
    });

    expect(saveResult.content).toBeDefined();
    const saveData = JSON.parse(saveResult.content[0].text!);
    expect(saveData.id).toBeGreaterThan(0);
    expect(saveData.file_path).toBe("src/Auth.ts");

    // List notes and verify it appears
    const listResult = await client.callTool("read_project_notes", {
      file_path: "src/Auth.ts",
      repo_root: repoDir,
    });

    const listData = JSON.parse(listResult.content[0].text!);
    expect(listData.memories).toBeDefined();
    expect(listData.memories.length).toBeGreaterThan(0);
    expect(listData.memories[0].content).toContain("JWT");
  });

  it("should show memories in analysis results for coupled files", async () => {
    // Save a note for Session.ts (which is coupled with Auth.ts)
    await client.callTool("save_project_note", {
      file_path: "src/Session.ts",
      note: "Session requires Redis connection",
      repo_root: repoDir,
    });

    // Analyze Auth.ts — Session.ts should appear as coupled with the note
    const result = await client.callTool("get_impact_analysis", {
      file_path: "src/Auth.ts",
      repo_root: repoDir,
    });

    const data = JSON.parse(result.content[0].text!);
    const sessionFile = data.coupled_files.find(
      (f: { path: string }) => f.path === "src/Session.ts"
    );

    expect(sessionFile).toBeDefined();
    expect(sessionFile.memories).toBeDefined();
    expect(sessionFile.memories.length).toBeGreaterThan(0);
    expect(sessionFile.memories[0].content).toContain("Redis");
  });

  it("should search notes by content", async () => {
    const searchResult = await client.callTool("read_project_notes", {
      query: "Redis",
      repo_root: repoDir,
    });

    const searchData = JSON.parse(searchResult.content[0].text!);
    expect(searchData.memories).toBeDefined();
    expect(searchData.memories.length).toBeGreaterThan(0);
    expect(searchData.memories[0].content).toContain("Redis");
  });
});

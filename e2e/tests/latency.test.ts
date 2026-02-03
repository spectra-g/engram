import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { McpTestClient } from "./helpers/mcp-client.js";
import { createCoupledFilesRepo } from "../../fixtures/src/scenarios/coupled-files.js";
import { CORE_BINARY_PATH } from "./setup.js";
import { rmSync } from "node:fs";

describe("NFR: latency", () => {
  let client: McpTestClient;
  let repoDir: string;

  beforeAll(async () => {
    repoDir = createCoupledFilesRepo();
    client = new McpTestClient();
    await client.connect({ coreBinaryPath: CORE_BINARY_PATH });
  });

  afterAll(async () => {
    await client.close();
    rmSync(repoDir, { recursive: true, force: true });
  });

  it("cold start should complete in under 2 seconds", async () => {
    const start = performance.now();

    await client.callTool("get_impact_analysis", {
      file_path: "src/Auth.ts",
      repo_root: repoDir,
    });

    const elapsed = performance.now() - start;
    expect(elapsed).toBeLessThan(2000);
  });

  it("warm path should complete in under 200ms", async () => {
    // First call populates the cache / SQLite index
    await client.callTool("get_impact_analysis", {
      file_path: "src/Auth.ts",
      repo_root: repoDir,
    });

    // Second call should be fast (warm path)
    const start = performance.now();

    await client.callTool("get_impact_analysis", {
      file_path: "src/Auth.ts",
      repo_root: repoDir,
    });

    const elapsed = performance.now() - start;
    expect(elapsed).toBeLessThan(200);
  });
});

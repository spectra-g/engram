import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { McpTestClient } from "./helpers/mcp-client.js";
import { createCoupledFilesRepo } from "../../fixtures/src/scenarios/coupled-files.js";
import { CORE_BINARY_PATH } from "./setup.js";
import { rmSync } from "node:fs";

describe("error handling: graceful failures", () => {
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

  it("should return empty coupled_files for a file not in the repo", async () => {
    const result = await client.callTool("get_impact_analysis", {
      file_path: "src/DoesNotExist.ts",
      repo_root: repoDir,
    });

    expect(result.content).toBeDefined();
    expect(result.content.length).toBeGreaterThan(0);

    const text = result.content[0].text!;
    const data = JSON.parse(text);

    expect(data.file_path).toBe("src/DoesNotExist.ts");
    expect(data.coupled_files).toEqual([]);
    expect(data.commit_count).toBe(0);
  });

  it("should return an error for an invalid repo root", async () => {
    const result = await client.callTool("get_impact_analysis", {
      file_path: "src/Auth.ts",
      repo_root: "/tmp/not-a-real-git-repo-xyz",
    });

    expect(result.content).toBeDefined();
    const text = result.content[0].text!;
    const data = JSON.parse(text);

    expect(data.error).toBeDefined();
    expect(data.error).toMatch(/repository|exited with code/i);
  });
});

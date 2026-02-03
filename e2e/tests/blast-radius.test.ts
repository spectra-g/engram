import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { McpTestClient } from "./helpers/mcp-client.js";
import { createCoupledFilesRepo } from "../../fixtures/src/scenarios/coupled-files.js";
import { CORE_BINARY_PATH } from "./setup.js";
import { rmSync } from "node:fs";

describe("blast-radius: coupled file detection", () => {
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

  it("should detect src/Session.db as coupled to src/Auth.ts", async () => {
    const result = await client.callTool("get_impact_analysis", {
      file_path: "src/Auth.ts",
      repo_root: repoDir,
    });

    expect(result.content).toBeDefined();
    expect(result.content.length).toBeGreaterThan(0);

    const text = result.content[0].text!;
    const data = JSON.parse(text);

    // Should find Session.db as a coupled file
    const coupledFiles = data.coupled_files as Array<{
      path: string;
      coupling_score: number;
    }>;
    expect(coupledFiles).toBeDefined();
    expect(coupledFiles.length).toBeGreaterThan(0);

    const sessionFile = coupledFiles.find((f) => f.path === "src/Session.db");
    expect(sessionFile).toBeDefined();
    expect(sessionFile!.coupling_score).toBeGreaterThan(0.8);
  });

  it("should NOT include unrelated files with high coupling", async () => {
    const result = await client.callTool("get_impact_analysis", {
      file_path: "src/Auth.ts",
      repo_root: repoDir,
    });

    const text = result.content[0].text!;
    const data = JSON.parse(text);

    const coupledFiles = data.coupled_files as Array<{
      path: string;
      coupling_score: number;
    }>;

    // Utils.ts was only in the initial commit — coupling should be low
    const utilsFile = coupledFiles.find((f) => f.path === "src/Utils.ts");
    if (utilsFile) {
      expect(utilsFile.coupling_score).toBeLessThan(0.1);
    }
  });

  it("should return the queried file path in the response", async () => {
    const result = await client.callTool("get_impact_analysis", {
      file_path: "src/Auth.ts",
      repo_root: repoDir,
    });

    const text = result.content[0].text!;
    const data = JSON.parse(text);
    expect(data.file_path).toBe("src/Auth.ts");
  });

  it("should include a summary string in the response", async () => {
    const result = await client.callTool("get_impact_analysis", {
      file_path: "src/Auth.ts",
      repo_root: repoDir,
    });

    const text = result.content[0].text!;
    const data = JSON.parse(text);
    expect(typeof data.summary).toBe("string");
    expect(data.summary.length).toBeGreaterThan(0);
  });
});

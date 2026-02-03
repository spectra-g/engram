import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { McpTestClient } from "./helpers/mcp-client.js";
import { createRiskScoringRepo } from "../../fixtures/src/scenarios/risk-scoring.js";
import { CORE_BINARY_PATH } from "./setup.js";
import { rmSync } from "node:fs";

describe("risk-scoring: coupled files ranked by risk", () => {
  let client: McpTestClient;
  let repoDir: string;

  beforeAll(async () => {
    repoDir = createRiskScoringRepo();
    client = new McpTestClient();
    await client.connect({ coreBinaryPath: CORE_BINARY_PATH });
  });

  afterAll(async () => {
    await client.close();
    rmSync(repoDir, { recursive: true, force: true });
  });

  it("should include risk_score on all coupled files in [0.0, 1.0]", async () => {
    const result = await client.callTool("get_impact_analysis", {
      file_path: "src/Core.ts",
      repo_root: repoDir,
    });

    const text = result.content[0].text!;
    const data = JSON.parse(text);
    const coupledFiles = data.coupled_files as Array<{
      path: string;
      risk_score: number;
    }>;

    expect(coupledFiles.length).toBeGreaterThan(0);
    for (const f of coupledFiles) {
      expect(f.risk_score).toBeGreaterThanOrEqual(0.0);
      expect(f.risk_score).toBeLessThanOrEqual(1.0);
    }
  });

  it("should return coupled files sorted by risk_score descending", async () => {
    const result = await client.callTool("get_impact_analysis", {
      file_path: "src/Core.ts",
      repo_root: repoDir,
    });

    const text = result.content[0].text!;
    const data = JSON.parse(text);
    const coupledFiles = data.coupled_files as Array<{
      path: string;
      risk_score: number;
    }>;

    for (let i = 1; i < coupledFiles.length; i++) {
      expect(coupledFiles[i - 1].risk_score).toBeGreaterThanOrEqual(
        coupledFiles[i].risk_score
      );
    }
  });

  it("should rank HighRisk.ts above LowRisk.ts", async () => {
    const result = await client.callTool("get_impact_analysis", {
      file_path: "src/Core.ts",
      repo_root: repoDir,
    });

    const text = result.content[0].text!;
    const data = JSON.parse(text);
    const coupledFiles = data.coupled_files as Array<{
      path: string;
      risk_score: number;
    }>;

    const highRisk = coupledFiles.find((f) => f.path === "src/HighRisk.ts");
    const lowRisk = coupledFiles.find((f) => f.path === "src/LowRisk.ts");

    expect(highRisk).toBeDefined();
    expect(lowRisk).toBeDefined();
    expect(highRisk!.risk_score).toBeGreaterThan(lowRisk!.risk_score);
  });

  it("should include formatted_files with risk_level capped at 5", async () => {
    const result = await client.callTool("get_impact_analysis", {
      file_path: "src/Core.ts",
      repo_root: repoDir,
    });

    const text = result.content[0].text!;
    const data = JSON.parse(text);

    expect(data.formatted_files).toBeDefined();
    expect(data.formatted_files.length).toBeLessThanOrEqual(5);

    for (const f of data.formatted_files) {
      expect(["Critical", "High", "Medium", "Low"]).toContain(f.risk_level);
    }
  });

  it("should classify HighRisk.ts as Critical or High based on commit pattern", async () => {
    const result = await client.callTool("get_impact_analysis", {
      file_path: "src/Core.ts",
      repo_root: repoDir,
    });

    const text = result.content[0].text!;
    const data = JSON.parse(text);

    // HighRisk.ts: 40 co-commits out of 65 total (61.5% coupling) + recent changes
    // Should result in Critical or High risk
    const highRisk = data.formatted_files.find(
      (f: any) => f.path === "src/HighRisk.ts"
    );

    expect(highRisk).toBeDefined();
    expect(["Critical", "High"]).toContain(highRisk.risk_level);
    expect(highRisk.risk_score).toBeGreaterThanOrEqual(0.6); // At least High threshold
  });

  it("should classify LowRisk.ts as Low or Medium based on commit pattern", async () => {
    const result = await client.callTool("get_impact_analysis", {
      file_path: "src/Core.ts",
      repo_root: repoDir,
    });

    const text = result.content[0].text!;
    const data = JSON.parse(text);

    // LowRisk.ts: 5 co-commits out of 65 total (7.7% coupling) + old changes
    // Should result in Low or Medium risk
    const lowRisk = data.formatted_files.find(
      (f: any) => f.path === "src/LowRisk.ts"
    );

    expect(lowRisk).toBeDefined();
    expect(["Low", "Medium"]).toContain(lowRisk.risk_level);
    expect(lowRisk.risk_score).toBeLessThan(0.6); // Below High threshold
  });

  it("should classify files based on weighted formula: coupling 50%, churn 30%, recency 20%", async () => {
    const result = await client.callTool("get_impact_analysis", {
      file_path: "src/Core.ts",
      repo_root: repoDir,
    });

    const text = result.content[0].text!;
    const data = JSON.parse(text);
    const files = data.coupled_files as Array<{
      path: string;
      risk_score: number;
      coupling_score: number;
      co_change_count: number;
    }>;

    // Verify that files with higher coupling score higher
    const highRisk = files.find((f) => f.path === "src/HighRisk.ts");
    const mediumRisk = files.find((f) => f.path === "src/MediumRisk.ts");
    const lowRisk = files.find((f) => f.path === "src/LowRisk.ts");

    expect(highRisk).toBeDefined();
    expect(mediumRisk).toBeDefined();
    expect(lowRisk).toBeDefined();

    // Coupling dominates (50% weight), so order should follow coupling
    expect(highRisk!.coupling_score).toBeGreaterThan(mediumRisk!.coupling_score);
    expect(mediumRisk!.coupling_score).toBeGreaterThan(lowRisk!.coupling_score);

    // Risk scores should follow the same order due to coupling weight
    expect(highRisk!.risk_score).toBeGreaterThan(mediumRisk!.risk_score);
    expect(mediumRisk!.risk_score).toBeGreaterThan(lowRisk!.risk_score);
  });
});

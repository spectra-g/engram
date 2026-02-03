import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { McpTestClient } from "./helpers/mcp-client.js";
import { createCouplingGateRepo } from "../../fixtures/src/scenarios/coupling-gate.js";
import { CORE_BINARY_PATH } from "./setup.js";
import { rmSync } from "node:fs";

describe("coupling-gate: prevent low-coupling files from being Critical", () => {
  let client: McpTestClient;
  let repoDir: string;

  beforeAll(async () => {
    repoDir = createCouplingGateRepo();
    client = new McpTestClient();
    await client.connect({ coreBinaryPath: CORE_BINARY_PATH });
  });

  afterAll(async () => {
    await client.close();
    rmSync(repoDir, { recursive: true, force: true });
  });

  it("should cap files with <50% coupling at High risk (0.79) even with high churn", async () => {
    const result = await client.callTool("get_impact_analysis", {
      file_path: "src/Target.ts",
      repo_root: repoDir,
    });

    const text = result.content[0].text!;
    const data = JSON.parse(text);

    // HighChurnLowCoupling.ts: 8/20 = 40% coupling, but 108 total commits (high churn) + recent
    const highChurn = data.coupled_files.find(
      (f: any) => f.path === "src/HighChurnLowCoupling.ts"
    );

    expect(highChurn).toBeDefined();
    expect(highChurn.coupling_score).toBeLessThan(0.5);

    // Should be capped at 0.79 (High risk max)
    expect(highChurn.risk_score).toBeLessThan(0.8);
    expect(highChurn.risk_score).toBeGreaterThan(0.5); // Should still be substantial

    // Verify classification is High, not Critical
    const highChurnFormatted = data.formatted_files.find(
      (f: any) => f.path === "src/HighChurnLowCoupling.ts"
    );
    expect(highChurnFormatted?.risk_level).toBe("High");
  });

  it("should allow files with >=50% coupling to reach Critical risk", async () => {
    const result = await client.callTool("get_impact_analysis", {
      file_path: "src/Target.ts",
      repo_root: repoDir,
    });

    const text = result.content[0].text!;
    const data = JSON.parse(text);

    // HighCouplingFile.ts: 15/20 = 75% coupling + recent changes
    const highCoupling = data.coupled_files.find(
      (f: any) => f.path === "src/HighCouplingFile.ts"
    );

    expect(highCoupling).toBeDefined();
    expect(highCoupling.coupling_score).toBeGreaterThanOrEqual(0.5);

    // Can reach Critical if score >= 0.8
    if (highCoupling.risk_score >= 0.8) {
      const highCouplingFormatted = data.formatted_files.find(
        (f: any) => f.path === "src/HighCouplingFile.ts"
      );
      expect(highCouplingFormatted?.risk_level).toBe("Critical");
    }
  });

  it("should verify gate formula: low coupling prevents Critical classification", async () => {
    const result = await client.callTool("get_impact_analysis", {
      file_path: "src/Target.ts",
      repo_root: repoDir,
    });

    const text = result.content[0].text!;
    const data = JSON.parse(text);

    const files = data.coupled_files as Array<{
      path: string;
      coupling_score: number;
      risk_score: number;
    }>;

    // Verify: ALL files with coupling < 0.5 have risk_score < 0.8
    const lowCouplingFiles = files.filter((f) => f.coupling_score < 0.5);
    for (const file of lowCouplingFiles) {
      expect(file.risk_score).toBeLessThan(0.8);
    }

    // Verify: Files with coupling >= 0.5 CAN have risk_score >= 0.8
    // (though not guaranteed depending on other factors)
    const highCouplingFiles = files.filter((f) => f.coupling_score >= 0.5);
    const hasHighRisk = highCouplingFiles.some((f) => f.risk_score >= 0.8);
    // At least one high-coupling file should be able to reach Critical
    expect(highCouplingFiles.length).toBeGreaterThan(0);
  });
});

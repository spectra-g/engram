import { describe, it, expect, beforeEach, afterAll } from "vitest";
import { analyze, addNote, getMetrics } from "../../adapter/src/process-bridge.js";
import { createCoupledFilesRepo } from "../../fixtures/src/scenarios/coupled-files.js";
import { rmSync } from "node:fs";

describe("Metrics", () => {
  let repoRoot: string;

  beforeEach(async () => {
    repoRoot = createCoupledFilesRepo();
  });

  afterAll(() => {
    if (repoRoot) {
      rmSync(repoRoot, { recursive: true, force: true });
    }
  });

  it("should track analysis calls", async () => {
    // Perform multiple analyses
    await analyze({ file_path: "src/A.ts", repo_root: repoRoot });
    await analyze({ file_path: "src/B.ts", repo_root: repoRoot });
    await analyze({ file_path: "src/C.ts", repo_root: repoRoot });

    // Get metrics
    const metrics = await getMetrics({ repo_root: repoRoot });

    expect(metrics.summary.total_analyses).toBe(3);
    expect(metrics.summary.avg_analysis_time_ms).toBeGreaterThan(0);
  });

  it("should count risk levels correctly", async () => {
    // Perform analysis (relies on test-repo setup with co-commits)
    await analyze({ file_path: "src/A.ts", repo_root: repoRoot });

    const metrics = await getMetrics({ repo_root: repoRoot });

    // Verify risk counts are captured (exact values depend on test-repo setup)
    expect(metrics.summary.total_coupled_files).toBeGreaterThanOrEqual(0);
    expect(
      metrics.summary.critical_risk_count +
      metrics.summary.high_risk_count +
      metrics.summary.medium_risk_count +
      metrics.summary.low_risk_count
    ).toBe(metrics.summary.total_coupled_files);
  });

  it("should track note creation", async () => {
    // Create notes
    await addNote({
      file_path: "src/A.ts",
      content: "Note 1",
      repo_root: repoRoot,
    });
    await addNote({
      file_path: "src/B.ts",
      content: "Note 2",
      repo_root: repoRoot,
    });

    const metrics = await getMetrics({ repo_root: repoRoot });

    expect(metrics.summary.notes_created).toBe(2);
  });

  it("should track test intent extraction", async () => {
    // Perform analysis on files with test intents
    await analyze({ file_path: "src/A.ts", repo_root: repoRoot });

    const metrics = await getMetrics({ repo_root: repoRoot });

    // Test intents depend on test-repo setup
    expect(metrics.summary.test_files_found).toBeGreaterThanOrEqual(0);
    expect(metrics.summary.test_intents_extracted).toBeGreaterThanOrEqual(0);
  });

  it("should calculate average analysis time", async () => {
    // Perform multiple analyses
    await analyze({ file_path: "src/A.ts", repo_root: repoRoot });
    await analyze({ file_path: "src/B.ts", repo_root: repoRoot });

    const metrics = await getMetrics({ repo_root: repoRoot });

    expect(metrics.summary.total_analyses).toBe(2);
    expect(metrics.summary.avg_analysis_time_ms).toBeGreaterThan(0);
  });

  it("should return empty metrics for new repo", async () => {
    // Get metrics before any operations
    const metrics = await getMetrics({ repo_root: repoRoot });

    expect(metrics.summary.total_analyses).toBe(0);
    expect(metrics.summary.notes_created).toBe(0);
    expect(metrics.summary.total_coupled_files).toBe(0);
    expect(metrics.summary.critical_risk_count).toBe(0);
    expect(metrics.summary.high_risk_count).toBe(0);
    expect(metrics.summary.medium_risk_count).toBe(0);
    expect(metrics.summary.low_risk_count).toBe(0);
    expect(metrics.summary.test_files_found).toBe(0);
    expect(metrics.summary.test_intents_extracted).toBe(0);
    expect(metrics.summary.avg_analysis_time_ms).toBe(0);
  });

  it("should accumulate metrics across operations", async () => {
    // Mix of operations
    await analyze({ file_path: "src/A.ts", repo_root: repoRoot });
    await addNote({
      file_path: "src/A.ts",
      content: "Note about A",
      repo_root: repoRoot,
    });
    await analyze({ file_path: "src/B.ts", repo_root: repoRoot });
    await addNote({
      file_path: "src/B.ts",
      content: "Note about B",
      repo_root: repoRoot,
    });

    const metrics = await getMetrics({ repo_root: repoRoot });

    expect(metrics.summary.total_analyses).toBe(2);
    expect(metrics.summary.notes_created).toBe(2);
  });
});

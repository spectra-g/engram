import { describe, it, expect } from "vitest";
import { classifyRisk, describeFile, buildSummaryLine, buildFileDetails, formatAnalysisResponse } from "../src/formatter.js";
import type { AnalysisResponse, CoupledFile, FormattedCoupledFile } from "../src/types.js";

describe("classifyRisk", () => {
  it("should return Critical for scores >= 0.8", () => {
    expect(classifyRisk(0.8)).toBe("Critical");
    expect(classifyRisk(0.95)).toBe("Critical");
    expect(classifyRisk(1.0)).toBe("Critical");
  });

  it("should return High for scores >= 0.6 and < 0.8", () => {
    expect(classifyRisk(0.6)).toBe("High");
    expect(classifyRisk(0.79)).toBe("High");
  });

  it("should return Medium for scores >= 0.3 and < 0.6", () => {
    expect(classifyRisk(0.3)).toBe("Medium");
    expect(classifyRisk(0.59)).toBe("Medium");
  });

  it("should return Low for scores < 0.3", () => {
    expect(classifyRisk(0.29)).toBe("Low");
    expect(classifyRisk(0.0)).toBe("Low");
  });
});

describe("describeFile", () => {
  it("should calculate percentage correctly", () => {
    const file: CoupledFile = { path: "a.ts", coupling_score: 0.5, co_change_count: 48, risk_score: 0.89 };
    const desc = describeFile(file, 50);
    expect(desc).toBe("Changed together in 48 of 50 commits (96%)");
  });

  it("should handle zero commit_count without division by zero", () => {
    const file: CoupledFile = { path: "a.ts", coupling_score: 0, co_change_count: 0, risk_score: 0 };
    const desc = describeFile(file, 0);
    expect(desc).toBe("Changed together in 0 of 0 commits (0%)");
  });
});

describe("formatAnalysisResponse", () => {
  const makeResponse = (fileCount: number): AnalysisResponse => {
    const coupled_files: CoupledFile[] = [];
    for (let i = 0; i < fileCount; i++) {
      coupled_files.push({
        path: `src/File${i}.ts`,
        coupling_score: 0.9 - i * 0.1,
        co_change_count: 48 - i * 5,
        risk_score: 0.9 - i * 0.1,
      });
    }
    return {
      file_path: "src/Auth.ts",
      repo_root: "/tmp/test",
      coupled_files,
      commit_count: 50,
      analysis_time_ms: 12,
    };
  };

  it("should return valid JSON with summary, formatted_files, and raw fields", () => {
    const response = makeResponse(3);
    const formatted = formatAnalysisResponse(response);
    const parsed = JSON.parse(formatted);

    expect(typeof parsed.summary).toBe("string");
    expect(Array.isArray(parsed.formatted_files)).toBe(true);
    expect(parsed.file_path).toBe("src/Auth.ts");
    expect(parsed.repo_root).toBe("/tmp/test");
    expect(parsed.commit_count).toBe(50);
    expect(parsed.analysis_time_ms).toBe(12);
    expect(Array.isArray(parsed.coupled_files)).toBe(true);
    expect(parsed.coupled_files).toHaveLength(3);
  });

  it("should include percentage description in formatted files", () => {
    const response = makeResponse(1);
    const parsed = JSON.parse(formatAnalysisResponse(response));
    expect(parsed.formatted_files[0].description).toContain("of 50 commits");
    expect(parsed.formatted_files[0].description).toContain("%");
  });

  it("should include risk level counts in summary", () => {
    const response = makeResponse(3);
    const parsed = JSON.parse(formatAnalysisResponse(response));
    // File0: 0.9 = Critical, File1: 0.8 = Critical, File2: 0.7 = High
    expect(parsed.summary).toContain("critical risk");
    expect(parsed.summary).toContain("high risk");
  });

  it("should cap formatted_files at 5 while preserving full coupled_files", () => {
    const response = makeResponse(8);
    const parsed = JSON.parse(formatAnalysisResponse(response));

    expect(parsed.formatted_files.length).toBeLessThanOrEqual(5);
    expect(parsed.coupled_files).toHaveLength(8);
  });

  it("should produce no-coupled-files message for empty list", () => {
    const response = makeResponse(0);
    const parsed = JSON.parse(formatAnalysisResponse(response));

    expect(parsed.summary).toContain("no coupled files");
    expect(parsed.formatted_files).toHaveLength(0);
  });

  it("should preserve raw data fields at top level", () => {
    const response = makeResponse(2);
    const parsed = JSON.parse(formatAnalysisResponse(response));

    expect(parsed.file_path).toBe("src/Auth.ts");
    expect(parsed.coupled_files).toHaveLength(2);
    expect(parsed.commit_count).toBe(50);
  });

  it("should include risk_level on each formatted file", () => {
    const response = makeResponse(3);
    const parsed = JSON.parse(formatAnalysisResponse(response));

    for (const f of parsed.formatted_files) {
      expect(["Critical", "High", "Medium", "Low"]).toContain(f.risk_level);
    }
  });

  it("should include memories in formatted output when present", () => {
    const response = makeResponse(1);
    response.coupled_files[0].memories = [
      { id: 1, file_path: "src/File0.ts", content: "Important note", created_at: "2025-01-01" },
    ];
    const parsed = JSON.parse(formatAnalysisResponse(response));

    expect(parsed.formatted_files[0].memories).toEqual(["Important note"]);
    expect(parsed.summary).toContain("Notes:");
  });

  it("should include test_intents in formatted output when present", () => {
    const response = makeResponse(1);
    response.coupled_files[0].test_intents = [
      { title: "should login with valid credentials" },
      { title: "should reject invalid password" },
    ];
    const parsed = JSON.parse(formatAnalysisResponse(response));

    expect(parsed.formatted_files[0].test_intents).toEqual([
      "should login with valid credentials",
      "should reject invalid password",
    ]);
    expect(parsed.summary).toContain("Current test behavior (may need updating):");
    expect(parsed.summary).toContain("- should login with valid credentials");
  });

  it("should omit test_intents key when not present", () => {
    const response = makeResponse(1);
    const parsed = JSON.parse(formatAnalysisResponse(response));

    expect(parsed.formatted_files[0].test_intents).toBeUndefined();
    expect(parsed.summary).not.toContain("Current test behavior");
  });
});

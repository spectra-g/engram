import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { McpTestClient } from "./helpers/mcp-client.js";
import { createTestIntentsRepo, createDunderTestsRepo } from "../../fixtures/src/scenarios/test-intents.js";
import { CORE_BINARY_PATH } from "./setup.js";
import { rmSync } from "node:fs";

describe("test-intents: test intent extraction", () => {
  let client: McpTestClient;
  let repoDir: string;

  beforeAll(async () => {
    repoDir = createTestIntentsRepo();
    client = new McpTestClient();
    await client.connect({ coreBinaryPath: CORE_BINARY_PATH });
  });

  afterAll(async () => {
    await client.close();
    rmSync(repoDir, { recursive: true, force: true });
  });

  it("should extract test intents from coupled test files", async () => {
    const result = await client.callTool("get_impact_analysis", {
      file_path: "src/Auth.ts",
      repo_root: repoDir,
    });

    const data = JSON.parse(result.content[0].text!);
    const testFile = data.coupled_files.find(
      (f: { path: string }) => f.path === "src/Auth.test.ts"
    );

    expect(testFile).toBeDefined();
    expect(testFile.test_intents).toBeDefined();
    expect(testFile.test_intents.length).toBeGreaterThan(0);

    const titles = testFile.test_intents.map((t: { title: string }) => t.title);
    expect(titles).toContain("should login with valid credentials");
    expect(titles).toContain("should reject invalid password");
  });

  it("should not include test_intents on non-test coupled files", async () => {
    const result = await client.callTool("get_impact_analysis", {
      file_path: "src/Auth.ts",
      repo_root: repoDir,
    });

    const data = JSON.parse(result.content[0].text!);
    const sessionFile = data.coupled_files.find(
      (f: { path: string }) => f.path === "src/Session.ts"
    );

    expect(sessionFile).toBeDefined();
    // Non-test files should have empty or no test_intents
    expect(sessionFile.test_intents?.length ?? 0).toBe(0);
  });

  it("should include qualification message in summary", async () => {
    const result = await client.callTool("get_impact_analysis", {
      file_path: "src/Auth.ts",
      repo_root: repoDir,
    });

    const data = JSON.parse(result.content[0].text!);
    expect(data.summary).toContain("Current test behavior");
    expect(data.summary).toContain("may need updating");
  });

  it("should cap test intents at 5 per file", async () => {
    const result = await client.callTool("get_impact_analysis", {
      file_path: "src/Auth.ts",
      repo_root: repoDir,
    });

    const data = JSON.parse(result.content[0].text!);
    const testFile = data.coupled_files.find(
      (f: { path: string }) => f.path === "src/Auth.test.ts"
    );

    expect(testFile).toBeDefined();
    expect(testFile.test_intents.length).toBeLessThanOrEqual(5);
  });
});

describe("test-intents: proactive test discovery via __tests__/", () => {
  let client: McpTestClient;
  let repoDir: string;

  beforeAll(async () => {
    repoDir = createDunderTestsRepo();
    client = new McpTestClient();
    await client.connect({ coreBinaryPath: CORE_BINARY_PATH });
  });

  afterAll(async () => {
    await client.close();
    rmSync(repoDir, { recursive: true, force: true });
  });

  it("should discover test files in __tests__/ via proactive discovery", async () => {
    const result = await client.callTool("get_impact_analysis", {
      file_path: "src/tools/base64/Base64Tool.tsx",
      repo_root: repoDir,
    });

    const data = JSON.parse(result.content[0].text!);

    // test_info should discover the __tests__/ file via naming convention
    expect(data.test_info).toBeDefined();
    expect(data.test_info.test_files.length).toBeGreaterThan(0);

    const discoveredTest = data.test_info.test_files.find(
      (f: { path: string }) => f.path.includes("Base64Tool.test.tsx")
    );
    expect(discoveredTest).toBeDefined();
    expect(discoveredTest.test_count).toBe(3);

    const titles = discoveredTest.test_intents.map((t: { title: string }) => t.title);
    expect(titles).toContain("should encode string to base64");
    expect(titles).toContain("should decode base64 to string");
  });

  it("should include coverage hint in test_info", async () => {
    const result = await client.callTool("get_impact_analysis", {
      file_path: "src/tools/base64/Base64Tool.tsx",
      repo_root: repoDir,
    });

    const data = JSON.parse(result.content[0].text!);
    expect(data.test_info).toBeDefined();
    expect(data.test_info.coverage_hint).toBeDefined();
    expect(data.test_info.coverage_hint).toContain("3 tests");
    expect(data.test_info.coverage_hint).toContain("source file");
  });

  it("should include test_info in summary output", async () => {
    const result = await client.callTool("get_impact_analysis", {
      file_path: "src/tools/base64/Base64Tool.tsx",
      repo_root: repoDir,
    });

    const data = JSON.parse(result.content[0].text!);
    expect(data.summary).toContain("Test coverage:");
    expect(data.summary).toContain("3 tests");
  });
});

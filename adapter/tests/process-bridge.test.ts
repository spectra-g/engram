import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { EventEmitter } from "node:events";
import type { AnalysisResponse, AddNoteResponse, SearchNotesResponse, ListNotesResponse } from "../src/types.js";

// We need to mock child_process before importing the module under test
vi.mock("node:child_process", () => ({
  spawn: vi.fn(),
}));

// Dynamic import so the mock is in place first
const { spawn } = await import("node:child_process");
const { runCore, analyze, addNote, searchNotes, listNotes } = await import("../src/process-bridge.js");

const mockSpawn = vi.mocked(spawn);

/** Helper: create a fake ChildProcess that we control */
function createFakeChild() {
  const stdout = new EventEmitter();
  const stderr = new EventEmitter();
  const child = new EventEmitter() as any;
  child.stdout = stdout;
  child.stderr = stderr;
  child.kill = vi.fn();
  return child;
}

describe("process-bridge", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    mockSpawn.mockReset();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe("runCore", () => {
    it("should resolve with stdout/stderr on successful exit", async () => {
      const child = createFakeChild();
      mockSpawn.mockReturnValue(child);

      const promise = runCore(["--analyze", "src/A.ts"]);

      child.stdout.emit("data", Buffer.from('{"ok":true}'));
      child.emit("close", 0);

      const result = await promise;
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toBe('{"ok":true}');
      expect(result.stderr).toBe("");
    });

    it("should capture stderr from the process", async () => {
      const child = createFakeChild();
      mockSpawn.mockReturnValue(child);

      const promise = runCore(["--analyze", "bad"]);

      child.stderr.emit("data", Buffer.from("Error: something broke"));
      child.emit("close", 1);

      const result = await promise;
      expect(result.exitCode).toBe(1);
      expect(result.stderr).toBe("Error: something broke");
    });

    it("should reject with timeout when process hangs", async () => {
      const child = createFakeChild();
      mockSpawn.mockReturnValue(child);

      const promise = runCore(["--analyze", "src/A.ts"]);

      // Advance past the 5000ms timeout
      vi.advanceTimersByTime(6000);

      await expect(promise).rejects.toThrow("timed out");
      expect(child.kill).toHaveBeenCalledWith("SIGKILL");
    });

    it("should reject when spawn itself fails", async () => {
      const child = createFakeChild();
      mockSpawn.mockReturnValue(child);

      const promise = runCore(["--analyze", "src/A.ts"]);

      child.emit("error", new Error("ENOENT: binary not found"));

      await expect(promise).rejects.toThrow("Failed to spawn engram-core");
    });
  });

  describe("analyze", () => {
    it("should parse valid JSON output from core binary", async () => {
      const child = createFakeChild();
      mockSpawn.mockReturnValue(child);

      const mockResponse: AnalysisResponse = {
        file_path: "src/Auth.ts",
        repo_root: "/tmp/test-repo",
        coupled_files: [
          { path: "src/Session.db", coupling_score: 0.95, co_change_count: 48, risk_score: 0.89 },
        ],
        commit_count: 50,
        analysis_time_ms: 15,
      };

      const promise = analyze({ file_path: "src/Auth.ts", repo_root: "/tmp/test-repo" });

      child.stdout.emit("data", Buffer.from(JSON.stringify(mockResponse)));
      child.emit("close", 0);

      const result = await promise;
      expect(result.coupled_files).toHaveLength(1);
      expect(result.coupled_files[0].path).toBe("src/Session.db");
      expect(result.coupled_files[0].risk_score).toBe(0.89);
    });

    it("should throw on non-zero exit code with stderr message", async () => {
      const child = createFakeChild();
      mockSpawn.mockReturnValue(child);

      const promise = analyze({ file_path: "src/Auth.ts", repo_root: "/tmp/test-repo" });

      child.stderr.emit("data", Buffer.from("Error: database locked"));
      child.emit("close", 1);

      await expect(promise).rejects.toThrow("engram-core exited with code 1");
      await expect(promise).rejects.toThrow("database locked");
    });

    it("should throw on invalid JSON output", async () => {
      const child = createFakeChild();
      mockSpawn.mockReturnValue(child);

      const promise = analyze({ file_path: "src/Auth.ts", repo_root: "/tmp/test-repo" });

      child.stdout.emit("data", Buffer.from("not valid json{{{"));
      child.emit("close", 0);

      await expect(promise).rejects.toThrow("Failed to parse engram-core output");
    });
  });

  describe("addNote", () => {
    it("should parse valid addNote response", async () => {
      const child = createFakeChild();
      mockSpawn.mockReturnValue(child);

      const mockResponse: AddNoteResponse = {
        id: 1,
        file_path: "src/Auth.ts",
        content: "Handles JWT tokens",
      };

      const promise = addNote({
        file_path: "src/Auth.ts",
        content: "Handles JWT tokens",
        repo_root: "/tmp/test-repo",
      });

      child.stdout.emit("data", Buffer.from(JSON.stringify(mockResponse)));
      child.emit("close", 0);

      const result = await promise;
      expect(result.id).toBe(1);
      expect(result.file_path).toBe("src/Auth.ts");
      expect(result.content).toBe("Handles JWT tokens");
    });
  });

  describe("searchNotes", () => {
    it("should parse valid searchNotes response", async () => {
      const child = createFakeChild();
      mockSpawn.mockReturnValue(child);

      const mockResponse: SearchNotesResponse = {
        query: "JWT",
        memories: [
          { id: 1, file_path: "src/Auth.ts", symbol_name: undefined, content: "Handles JWT", created_at: "2025-01-01" },
        ],
      };

      const promise = searchNotes({ query: "JWT", repo_root: "/tmp/test-repo" });

      child.stdout.emit("data", Buffer.from(JSON.stringify(mockResponse)));
      child.emit("close", 0);

      const result = await promise;
      expect(result.query).toBe("JWT");
      expect(result.memories).toHaveLength(1);
    });
  });

  describe("listNotes", () => {
    it("should parse valid listNotes response", async () => {
      const child = createFakeChild();
      mockSpawn.mockReturnValue(child);

      const mockResponse: ListNotesResponse = {
        file_path: "src/Auth.ts",
        memories: [
          { id: 1, file_path: "src/Auth.ts", content: "Note 1", created_at: "2025-01-01" },
        ],
      };

      const promise = listNotes({ repo_root: "/tmp/test-repo", file_path: "src/Auth.ts" });

      child.stdout.emit("data", Buffer.from(JSON.stringify(mockResponse)));
      child.emit("close", 0);

      const result = await promise;
      expect(result.file_path).toBe("src/Auth.ts");
      expect(result.memories).toHaveLength(1);
    });
  });
});

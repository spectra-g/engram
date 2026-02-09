import { spawn } from "node:child_process";
import { resolve, dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { existsSync } from "node:fs";
import { createRequire } from "node:module";
import type {
  AnalysisRequest,
  AnalysisResponse,
  AddNoteRequest,
  AddNoteResponse,
  SearchNotesRequest,
  SearchNotesResponse,
  ListNotesRequest,
  ListNotesResponse,
  GetMetricsRequest,
  MetricsResponse,
  ProcessResult,
} from "./types.js";

const __dirname = dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);

const TIMEOUT_MS = 5000;

const PLATFORM_PACKAGES: Record<string, string> = {
  "darwin-arm64": "@spectra-g/engram-core-darwin-arm64",
  "darwin-x64": "@spectra-g/engram-core-darwin-x64",
  "linux-x64": "@spectra-g/engram-core-linux-x64",
  "linux-arm64": "@spectra-g/engram-core-linux-arm64",
  "win32-x64": "@spectra-g/engram-core-win32-x64",
};

function getBinaryPath(): string {
  // Tier 1: Explicit env var override
  if (process.env.ENGRAM_CORE_BINARY) {
    return process.env.ENGRAM_CORE_BINARY;
  }

  // Tier 2: Platform-specific npm package
  const platformKey = `${process.platform}-${process.arch}`;
  const packageName = PLATFORM_PACKAGES[platformKey];
  if (packageName) {
    try {
      const packageJsonPath = require.resolve(`${packageName}/package.json`);
      const packageDir = dirname(packageJsonPath);
      const ext = process.platform === "win32" ? ".exe" : "";
      const binaryPath = join(packageDir, "bin", `engram-core${ext}`);
      if (existsSync(binaryPath)) {
        return binaryPath;
      }
    } catch {
      // Package not installed, fall through to dev path
    }
  }

  // Tier 3: Development fallback
  return resolve(__dirname, "../../target/release/engram-core");
}

export function runCore(args: string[]): Promise<ProcessResult> {
  const binaryPath = getBinaryPath();

  return new Promise((resolve, reject) => {
    const child = spawn(binaryPath, args, {
      stdio: ["ignore", "pipe", "pipe"],
    });
    let stdoutBuffer = "";
    let stderr = "";
    let resolved = false;

    const timer = setTimeout(() => {
      if (!resolved) {
        child.kill("SIGKILL");
        reject(new Error(`engram-core timed out after ${TIMEOUT_MS}ms`));
      }
    }, TIMEOUT_MS);

    child.stdout.on("data", (data: Buffer) => {
      if (resolved) return;

      stdoutBuffer += data.toString();
      if (stdoutBuffer.includes("\n")) {
        const line = stdoutBuffer.split("\n")[0];
        try {
          JSON.parse(line); // Validate before resolving
          resolved = true;
          clearTimeout(timer);
          resolve({ stdout: line, stderr: "", exitCode: 0 });

          // Detach: let background indexing continue
          (child.stdout as any).unref?.();
          (child.stderr as any).unref?.();
          child.unref();
        } catch {
          // Not valid JSON yet, wait for close event
        }
      }
    });

    child.stderr.on("data", (data: Buffer) => {
      stderr += data.toString();
    });

    child.on("error", (err) => {
      if (!resolved) {
        clearTimeout(timer);
        reject(new Error(`Failed to spawn engram-core: ${err.message}`));
      }
    });

    child.on("close", (code) => {
      if (!resolved) {
        clearTimeout(timer);
        resolve({ stdout: stdoutBuffer, stderr, exitCode: code ?? 1 });
      }
    });
  });
}

export async function analyze(
  request: AnalysisRequest
): Promise<AnalysisResponse> {
  const result = await runCore([
    "analyze",
    "--file",
    request.file_path,
    "--repo-root",
    request.repo_root,
  ]);

  if (result.exitCode !== 0) {
    throw new Error(
      `engram-core exited with code ${result.exitCode}: ${result.stderr}`
    );
  }

  try {
    return JSON.parse(result.stdout) as AnalysisResponse;
  } catch {
    throw new Error(
      `Failed to parse engram-core output: ${result.stdout.slice(0, 200)}`
    );
  }
}

export async function addNote(
  request: AddNoteRequest
): Promise<AddNoteResponse> {
  const args = [
    "add-note",
    "--file",
    request.file_path,
    "--content",
    request.content,
    "--repo-root",
    request.repo_root,
  ];

  if (request.symbol_name) {
    args.push("--symbol", request.symbol_name);
  }

  const result = await runCore(args);

  if (result.exitCode !== 0) {
    throw new Error(
      `engram-core exited with code ${result.exitCode}: ${result.stderr}`
    );
  }

  try {
    return JSON.parse(result.stdout) as AddNoteResponse;
  } catch {
    throw new Error(
      `Failed to parse engram-core output: ${result.stdout.slice(0, 200)}`
    );
  }
}

export async function searchNotes(
  request: SearchNotesRequest
): Promise<SearchNotesResponse> {
  const result = await runCore([
    "search-notes",
    "--query",
    request.query,
    "--repo-root",
    request.repo_root,
  ]);

  if (result.exitCode !== 0) {
    throw new Error(
      `engram-core exited with code ${result.exitCode}: ${result.stderr}`
    );
  }

  try {
    return JSON.parse(result.stdout) as SearchNotesResponse;
  } catch {
    throw new Error(
      `Failed to parse engram-core output: ${result.stdout.slice(0, 200)}`
    );
  }
}

export async function listNotes(
  request: ListNotesRequest
): Promise<ListNotesResponse> {
  const args = [
    "list-notes",
    "--repo-root",
    request.repo_root,
  ];

  if (request.file_path) {
    args.push("--file", request.file_path);
  }

  const result = await runCore(args);

  if (result.exitCode !== 0) {
    throw new Error(
      `engram-core exited with code ${result.exitCode}: ${result.stderr}`
    );
  }

  try {
    return JSON.parse(result.stdout) as ListNotesResponse;
  } catch {
    throw new Error(
      `Failed to parse engram-core output: ${result.stdout.slice(0, 200)}`
    );
  }
}

export async function getMetrics(
  request: GetMetricsRequest
): Promise<MetricsResponse> {
  const result = await runCore([
    "get-metrics",
    "--repo-root",
    request.repo_root,
  ]);

  if (result.exitCode !== 0) {
    throw new Error(
      `engram-core exited with code ${result.exitCode}: ${result.stderr}`
    );
  }

  try {
    return JSON.parse(result.stdout) as MetricsResponse;
  } catch {
    throw new Error(
      `Failed to parse engram-core output: ${result.stdout.slice(0, 200)}`
    );
  }
}

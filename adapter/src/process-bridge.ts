import { spawn } from "node:child_process";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import type {
  AnalysisRequest,
  AnalysisResponse,
  AddNoteRequest,
  AddNoteResponse,
  SearchNotesRequest,
  SearchNotesResponse,
  ListNotesRequest,
  ListNotesResponse,
  ProcessResult,
} from "./types.js";

const __dirname = dirname(fileURLToPath(import.meta.url));

const DEFAULT_BINARY_PATH = resolve(
  __dirname,
  "../../core/target/release/engram-core"
);

const TIMEOUT_MS = 5000;

function getBinaryPath(): string {
  return process.env.ENGRAM_CORE_BINARY || DEFAULT_BINARY_PATH;
}

export function runCore(args: string[]): Promise<ProcessResult> {
  const binaryPath = getBinaryPath();

  return new Promise((resolve, reject) => {
    const child = spawn(binaryPath, args);
    let stdout = "";
    let stderr = "";

    const timer = setTimeout(() => {
      child.kill("SIGKILL");
      reject(new Error(`engram-core timed out after ${TIMEOUT_MS}ms`));
    }, TIMEOUT_MS);

    child.stdout.on("data", (data: Buffer) => {
      stdout += data.toString();
    });

    child.stderr.on("data", (data: Buffer) => {
      stderr += data.toString();
    });

    child.on("error", (err) => {
      clearTimeout(timer);
      reject(new Error(`Failed to spawn engram-core: ${err.message}`));
    });

    child.on("close", (code) => {
      clearTimeout(timer);
      resolve({ stdout, stderr, exitCode: code ?? 1 });
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

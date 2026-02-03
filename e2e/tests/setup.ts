import { execSync } from "node:child_process";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { existsSync } from "node:fs";

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, "../..");
const CORE_DIR = resolve(ROOT, "core");
const ADAPTER_DIR = resolve(ROOT, "adapter");

/**
 * Global setup: build the Rust binary and TypeScript adapter
 * before any E2E tests run.
 */
export function globalSetup(): string {
  // Build Rust core (release for accurate latency measurements)
  const binaryPath = resolve(CORE_DIR, "../target/release/engram-core");

  if (!existsSync(binaryPath)) {
    console.log("Building engram-core (release)...");
    execSync("cargo build --release", {
      cwd: CORE_DIR,
      stdio: "inherit",
      env: {
        ...process.env,
        PATH: `${process.env.HOME}/.cargo/bin:${process.env.PATH}`,
      },
    });
  }

  // Build adapter TypeScript
  const adapterDist = resolve(ADAPTER_DIR, "dist/index.js");
  if (!existsSync(adapterDist)) {
    console.log("Building adapter...");
    execSync("npm run build", { cwd: ADAPTER_DIR, stdio: "inherit" });
  }

  return binaryPath;
}

export const CORE_BINARY_PATH = globalSetup();

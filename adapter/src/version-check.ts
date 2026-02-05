import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));

const REGISTRY_URL = "https://registry.npmjs.org/@spectra-g/engram-adapter/latest";
const TIMEOUT_MS = 3000;

function getLocalVersion(): string {
  const packageJsonPath = join(__dirname, "..", "package.json");
  const packageJson = JSON.parse(readFileSync(packageJsonPath, "utf-8"));
  return packageJson.version;
}

export function checkForUpdates(): void {
  const localVersion = getLocalVersion();

  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), TIMEOUT_MS);

  fetch(REGISTRY_URL, { signal: controller.signal })
    .then((res) => {
      if (!res.ok) return;
      return res.json();
    })
    .then((data) => {
      if (!data || !data.version) return;
      if (data.version !== localVersion) {
        process.stderr.write(
          `\n[engram] Update available: ${localVersion} → ${data.version}\n` +
          `[engram] Run: npm install -g @spectra-g/engram-adapter\n\n`
        );
      }
    })
    .catch(() => {
      // Silently ignore network errors — this is non-blocking and best-effort
    })
    .finally(() => {
      clearTimeout(timeout);
    });
}

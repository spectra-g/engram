import { execSync } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";

export interface CommitSpec {
  files: Record<string, string>; // path -> content
  message: string;
}

export interface RepoOptions {
  commits: CommitSpec[];
}

/**
 * Creates a deterministic git repository in a temp directory.
 * Returns the path to the repo root.
 */
export function createRepo(options: RepoOptions): string {
  const repoDir = mkdtempSync(join(tmpdir(), "engram-fixture-"));

  const git = (cmd: string) =>
    execSync(`git ${cmd}`, {
      cwd: repoDir,
      stdio: "pipe",
      env: {
        ...process.env,
        GIT_AUTHOR_NAME: "Test",
        GIT_AUTHOR_EMAIL: "test@test.com",
        GIT_COMMITTER_NAME: "Test",
        GIT_COMMITTER_EMAIL: "test@test.com",
      },
    });

  git("init");
  git("config user.name Test");
  git("config user.email test@test.com");

  for (const commit of options.commits) {
    for (const [filePath, content] of Object.entries(commit.files)) {
      const fullPath = join(repoDir, filePath);
      mkdirSync(dirname(fullPath), { recursive: true });
      writeFileSync(fullPath, content);
      git(`add "${filePath}"`);
    }
    git(`commit -m "${commit.message}" --allow-empty`);
  }

  return repoDir;
}

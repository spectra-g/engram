import { createRepo, type CommitSpec } from "../repo-generator.js";

/**
 * Creates a repo where src/Auth.ts and src/Session.db are committed
 * together 50 times. Also includes an unrelated file (src/Utils.ts)
 * that is only committed once.
 */
export function createCoupledFilesRepo(): string {
  const commits: CommitSpec[] = [];

  // Initial commit with all files
  commits.push({
    files: {
      "src/Auth.ts": "// Auth module v0\nexport class Auth {}",
      "src/Session.db": "// Session store v0\nexport class Session {}",
      "src/Utils.ts": "// Utility functions\nexport function noop() {}",
    },
    message: "initial commit",
  });

  // 50 coupled commits: Auth.ts + Session.db always together
  for (let i = 1; i <= 50; i++) {
    commits.push({
      files: {
        "src/Auth.ts": `// Auth module v${i}\nexport class Auth { version = ${i}; }`,
        "src/Session.db": `// Session store v${i}\nexport class Session { version = ${i}; }`,
      },
      message: `update auth and session v${i}`,
    });
  }

  return createRepo({ commits });
}

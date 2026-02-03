import { createRepo, type CommitSpec } from "../repo-generator.js";

/**
 * Creates a repo suitable for testing the memories (knowledge graph) feature.
 * Has coupled files so we can verify memories appear in analysis results.
 */
export function createMemoriesRepo(): string {
  const commits: CommitSpec[] = [];

  // Initial commit with all files
  commits.push({
    files: {
      "src/Auth.ts": "// Auth module v0\nexport class Auth {}",
      "src/Session.ts": "// Session module v0\nexport class Session {}",
    },
    message: "initial commit",
  });

  // 10 coupled commits: Auth.ts + Session.ts always together
  for (let i = 1; i <= 10; i++) {
    commits.push({
      files: {
        "src/Auth.ts": `// Auth module v${i}\nexport class Auth { version = ${i}; }`,
        "src/Session.ts": `// Session module v${i}\nexport class Session { version = ${i}; }`,
      },
      message: `update auth and session v${i}`,
    });
  }

  return createRepo({ commits });
}

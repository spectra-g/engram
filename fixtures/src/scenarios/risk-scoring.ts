import { createRepo, type CommitSpec } from "../repo-generator.js";

/**
 * Creates a repo where src/Core.ts is coupled with three files at
 * different frequencies and recencies, producing distinct risk scores:
 *
 * - HighRisk.ts: 40 co-commits with Core.ts, changed recently
 * - MediumRisk.ts: 20 co-commits with Core.ts, changed in middle period
 * - LowRisk.ts: 5 co-commits with Core.ts, only early commits
 */
export function createRiskScoringRepo(): string {
  const commits: CommitSpec[] = [];

  // Initial commit with all files
  commits.push({
    files: {
      "src/Core.ts": "// Core module v0\nexport class Core {}",
      "src/HighRisk.ts": "// HighRisk v0\nexport class HighRisk {}",
      "src/MediumRisk.ts": "// MediumRisk v0\nexport class MediumRisk {}",
      "src/LowRisk.ts": "// LowRisk v0\nexport class LowRisk {}",
    },
    message: "initial commit",
  });

  // Early period (commits 1-5): LowRisk co-committed with Core
  for (let i = 1; i <= 5; i++) {
    commits.push({
      files: {
        "src/Core.ts": `// Core module v${i}\nexport class Core { version = ${i}; }`,
        "src/LowRisk.ts": `// LowRisk v${i}\nexport class LowRisk { version = ${i}; }`,
      },
      message: `early: update core and low-risk v${i}`,
    });
  }

  // Middle period (commits 6-25): MediumRisk co-committed with Core
  for (let i = 6; i <= 25; i++) {
    commits.push({
      files: {
        "src/Core.ts": `// Core module v${i}\nexport class Core { version = ${i}; }`,
        "src/MediumRisk.ts": `// MediumRisk v${i}\nexport class MediumRisk { version = ${i}; }`,
      },
      message: `middle: update core and medium-risk v${i}`,
    });
  }

  // Recent period (commits 26-65): HighRisk co-committed with Core
  for (let i = 26; i <= 65; i++) {
    commits.push({
      files: {
        "src/Core.ts": `// Core module v${i}\nexport class Core { version = ${i}; }`,
        "src/HighRisk.ts": `// HighRisk v${i}\nexport class HighRisk { version = ${i}; }`,
      },
      message: `recent: update core and high-risk v${i}`,
    });
  }

  return createRepo({ commits });
}

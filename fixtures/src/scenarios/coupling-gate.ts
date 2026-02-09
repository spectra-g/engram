import { createRepo, type CommitSpec } from "../repo-generator.js";

/**
 * Creates a repo to test the coupling gate: files with < 50% coupling
 * should be capped at risk_score 0.79 (High risk max) even if their
 * churn + recency would push them to >= 0.8 (Critical).
 *
 * Setup (including the initial commit that co-commits all files):
 * - Target.ts: committed 28 times
 * - HighChurnLowCoupling.ts:
 *   - Co-committed with Target.ts 9 times (32% coupling, < 50%)
 *   - But has 109 total commits (very high churn)
 *   - Co-changes are the most recent commits (ensures high recency)
 *   - With gate: capped at High risk even with high churn + recency
 * - HighCouplingFile.ts:
 *   - Co-committed with Target.ts 16 times (57% coupling, >= 50%)
 *   - Can reach Critical (>= 0.8) if other factors are high
 *
 * Phase ordering matters for recency: HighChurn co-commits are LAST so
 * their `last_timestamp` matches the repo's newest commit, guaranteeing
 * high recency regardless of CI timing / git second-precision timestamps.
 */
export function createCouplingGateRepo(): string {
  const commits: CommitSpec[] = [];

  // Initial commit (all 3 files co-committed)
  commits.push({
    files: {
      "src/Target.ts": "// Target v0\nexport class Target {}",
      "src/HighChurnLowCoupling.ts": "// HighChurn v0\nexport class HighChurn {}",
      "src/HighCouplingFile.ts": "// HighCoupling v0\nexport class HighCoupling {}",
    },
    message: "initial commit",
  });

  // Phase 1: HighChurnLowCoupling gets 100 solo commits (high churn)
  for (let i = 1; i <= 100; i++) {
    commits.push({
      files: {
        "src/HighChurnLowCoupling.ts": `// HighChurn v${i}\nexport class HighChurn { version = ${i}; }`,
      },
      message: `solo: update high-churn v${i}`,
    });
  }

  // Phase 2: HighCouplingFile co-committed with Target 15 times
  for (let i = 101; i <= 115; i++) {
    commits.push({
      files: {
        "src/Target.ts": `// Target v${i}\nexport class Target { version = ${i}; }`,
        "src/HighCouplingFile.ts": `// HighCoupling v${i}\nexport class HighCoupling { version = ${i}; }`,
      },
      message: `co-change: target + high-coupling v${i}`,
    });
  }

  // Phase 3: A few solo Target commits
  for (let i = 116; i <= 119; i++) {
    commits.push({
      files: {
        "src/Target.ts": `// Target v${i}\nexport class Target { version = ${i}; }`,
      },
      message: `solo: update target v${i}`,
    });
  }

  // Phase 4: HighChurnLowCoupling co-committed with Target 8 times (LAST phase
  // so co-change timestamps are the most recent — robust against CI timing)
  for (let i = 120; i <= 127; i++) {
    commits.push({
      files: {
        "src/Target.ts": `// Target v${i}\nexport class Target { version = ${i}; }`,
        "src/HighChurnLowCoupling.ts": `// HighChurn v${i}\nexport class HighChurn { version = ${i}; }`,
      },
      message: `co-change: target + high-churn v${i}`,
    });
  }

  return createRepo({ commits });
}

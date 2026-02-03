import { createRepo, type CommitSpec } from "../repo-generator.js";

/**
 * Creates a repo to test the coupling gate: files with < 50% coupling
 * should be capped at risk_score 0.79 (High risk max) even if their
 * churn + recency would push them to >= 0.8 (Critical).
 *
 * Setup:
 * - Target.ts: committed 20 times
 * - HighChurnLowCoupling.ts:
 *   - Co-committed with Target.ts only 8 times (40% coupling)
 *   - But has 100 solo commits (very high churn)
 *   - All changes are recent
 *   - Without gate: would score >= 0.8
 *   - With gate: capped at 0.79 (High risk)
 * - HighCouplingFile.ts:
 *   - Co-committed with Target.ts 15 times (75% coupling)
 *   - Should be able to reach Critical (>= 0.8) if other factors are high
 */
export function createCouplingGateRepo(): string {
  const commits: CommitSpec[] = [];

  // Initial commit
  commits.push({
    files: {
      "src/Target.ts": "// Target v0\nexport class Target {}",
      "src/HighChurnLowCoupling.ts": "// HighChurn v0\nexport class HighChurn {}",
      "src/HighCouplingFile.ts": "// HighCoupling v0\nexport class HighCoupling {}",
    },
    message: "initial commit",
  });

  // Phase 1: HighChurnLowCoupling gets 100 solo commits (high churn, recent)
  for (let i = 1; i <= 100; i++) {
    commits.push({
      files: {
        "src/HighChurnLowCoupling.ts": `// HighChurn v${i}\nexport class HighChurn { version = ${i}; }`,
      },
      message: `solo: update high-churn v${i}`,
    });
  }

  // Phase 2: HighChurnLowCoupling co-committed with Target 8 times (40% coupling)
  for (let i = 101; i <= 108; i++) {
    commits.push({
      files: {
        "src/Target.ts": `// Target v${i}\nexport class Target { version = ${i}; }`,
        "src/HighChurnLowCoupling.ts": `// HighChurn v${i}\nexport class HighChurn { version = ${i}; }`,
      },
      message: `co-change: target + high-churn v${i}`,
    });
  }

  // Phase 3: HighCouplingFile co-committed with Target 15 times (75% coupling)
  for (let i = 109; i <= 123; i++) {
    commits.push({
      files: {
        "src/Target.ts": `// Target v${i}\nexport class Target { version = ${i}; }`,
        "src/HighCouplingFile.ts": `// HighCoupling v${i}\nexport class HighCoupling { version = ${i}; }`,
      },
      message: `co-change: target + high-coupling v${i}`,
    });
  }

  // Phase 4: A few more Target solo commits to reach 20 total
  for (let i = 124; i <= 127; i++) {
    commits.push({
      files: {
        "src/Target.ts": `// Target v${i}\nexport class Target { version = ${i}; }`,
      },
      message: `solo: update target v${i}`,
    });
  }

  return createRepo({ commits });
}

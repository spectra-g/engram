import { createRepo, type CommitSpec } from "../repo-generator.js";

/**
 * Creates a repo for testing test intent extraction.
 * Auth.ts is coupled with Auth.test.ts (test file with 7 it() blocks)
 * and Session.ts (non-test file).
 */
export function createTestIntentsRepo(): string {
  const testFileContent = `
import { Auth } from './Auth';

describe('Auth', () => {
  it('should login with valid credentials', () => {
    expect(Auth.login('user', 'pass')).toBeTruthy();
  });

  it('should reject invalid password', () => {
    expect(Auth.login('user', 'wrong')).toBeFalsy();
  });

  it('should handle OAuth callback', () => {
    expect(Auth.handleOAuthCallback('code')).toBeDefined();
  });

  it('should refresh expired tokens', () => {
    expect(Auth.refreshToken('expired')).toBeDefined();
  });

  it('should logout and clear session', () => {
    Auth.logout();
    expect(Auth.isAuthenticated()).toBeFalsy();
  });

  it('should validate JWT signature', () => {
    expect(Auth.validateJWT('token')).toBeTruthy();
  });

  it('should enforce rate limiting on login', () => {
    expect(Auth.isRateLimited('user')).toBeFalsy();
  });
});
`;

  const commits: CommitSpec[] = [];

  // Initial commit with all files
  commits.push({
    files: {
      "src/Auth.ts": "// Auth module v0\nexport class Auth {}",
      "src/Auth.test.ts": testFileContent,
      "src/Session.ts": "// Session module v0\nexport class Session {}",
    },
    message: "initial commit",
  });

  // 30 coupled commits: Auth.ts + Auth.test.ts always together
  for (let i = 1; i <= 30; i++) {
    commits.push({
      files: {
        "src/Auth.ts": `// Auth module v${i}\nexport class Auth { version = ${i}; }`,
        "src/Auth.test.ts": testFileContent.replace(
          "describe('Auth'",
          `// v${i}\ndescribe('Auth'`
        ),
      },
      message: `update auth and tests v${i}`,
    });
  }

  // 20 coupled commits: Auth.ts + Session.ts together
  for (let i = 1; i <= 20; i++) {
    commits.push({
      files: {
        "src/Auth.ts": `// Auth module v${30 + i}\nexport class Auth { version = ${30 + i}; }`,
        "src/Session.ts": `// Session module v${i}\nexport class Session { version = ${i}; }`,
      },
      message: `update auth and session v${i}`,
    });
  }

  return createRepo({ commits });
}

/**
 * Creates a repo where the test file lives in __tests__/ and is NOT
 * co-committed with the source file (so it won't appear in coupled_files).
 * This tests proactive test discovery via find_test_files.
 */
export function createDunderTestsRepo(): string {
  const testFileContent = `
import { Base64 } from '../Base64Tool';

describe('Base64Tool', () => {
  it('should encode string to base64', () => {
    expect(Base64.encode('hello')).toBe('aGVsbG8=');
  });

  it('should decode base64 to string', () => {
    expect(Base64.decode('aGVsbG8=')).toBe('hello');
  });

  it('should handle empty input', () => {
    expect(Base64.encode('')).toBe('');
  });
});
`;

  const commits: CommitSpec[] = [];

  // Initial commit: source + test in __tests__/
  commits.push({
    files: {
      "src/tools/base64/Base64Tool.tsx": "// Base64 module v0\nexport class Base64 {}",
      "src/tools/base64/__tests__/Base64Tool.test.tsx": testFileContent,
      "src/tools/base64/helpers.ts": "// helpers v0\nexport function pad() {}",
    },
    message: "initial commit",
  });

  // 10 commits changing ONLY source + helpers (NOT the test file)
  // This ensures the test file won't appear in coupled_files
  for (let i = 1; i <= 10; i++) {
    commits.push({
      files: {
        "src/tools/base64/Base64Tool.tsx": `// Base64 module v${i}\nexport class Base64 { version = ${i}; }`,
        "src/tools/base64/helpers.ts": `// helpers v${i}\nexport function pad() { return ${i}; }`,
      },
      message: `update base64 and helpers v${i}`,
    });
  }

  return createRepo({ commits });
}

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::types::{CoupledFile, TestIntent};

const MAX_INTENTS_PER_FILE: usize = 5;

// Compiled regexes for test title extraction
static JS_TEST_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?:^|\s)(?:it|test)\(\s*(?:'([^']*)'|"([^"]*)")"#).unwrap()
});

static RUST_TEST_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"#\[test\]\s*(?:\n\s*)*fn\s+(\w+)").unwrap()
});

static PYTHON_TEST_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"def\s+(test_\w+)\s*\(").unwrap()
});

static GO_TEST_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"func\s+(Test\w+)\s*\(").unwrap()
});

/// Check if a file path looks like a test file based on naming conventions.
pub fn is_test_file(path: &str) -> bool {
    let Some(filename) = Path::new(path).file_name().and_then(|f| f.to_str()) else {
        return false;
    };

    // JS/TS: *.test.ts, *.spec.ts, *.test.js, *.spec.js, *.test.tsx, *.spec.tsx, etc.
    if filename.ends_with(".test.ts")
        || filename.ends_with(".spec.ts")
        || filename.ends_with(".test.js")
        || filename.ends_with(".spec.js")
        || filename.ends_with(".test.tsx")
        || filename.ends_with(".spec.tsx")
        || filename.ends_with(".test.jsx")
        || filename.ends_with(".spec.jsx")
    {
        return true;
    }

    // Go: *_test.go
    if filename.ends_with("_test.go") {
        return true;
    }

    // Python: test_*.py or *_test.py
    if filename.ends_with(".py") && (filename.starts_with("test_") || filename.ends_with("_test.py")) {
        return true;
    }

    // Rust: files inside a /tests/ directory
    if path.contains("/tests/") && filename.ends_with(".rs") {
        return true;
    }

    false
}

/// Humanize a snake_case or camelCase test name by stripping the "test_"/"Test" prefix
/// and replacing underscores with spaces.
fn humanize(name: &str) -> String {
    let stripped = name
        .strip_prefix("test_")
        .or_else(|| name.strip_prefix("Test"))
        .unwrap_or(name);
    stripped.replace('_', " ")
}

/// Extract test intent titles from file content using regex.
/// Returns at most `MAX_INTENTS_PER_FILE` results.
pub fn extract_test_intents(content: &str, path: &str) -> Vec<TestIntent> {
    let filename = Path::new(path)
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("");

    let mut intents: Vec<TestIntent> = Vec::new();

    if filename.ends_with(".ts")
        || filename.ends_with(".tsx")
        || filename.ends_with(".js")
        || filename.ends_with(".jsx")
    {
        for cap in JS_TEST_RE.captures_iter(content) {
            let title = cap.get(1).or_else(|| cap.get(2)).map(|m| m.as_str().to_string());
            if let Some(t) = title {
                intents.push(TestIntent { title: t });
                if intents.len() >= MAX_INTENTS_PER_FILE {
                    break;
                }
            }
        }
    } else if filename.ends_with(".rs") || path.contains("/tests/") {
        for cap in RUST_TEST_RE.captures_iter(content) {
            if let Some(name) = cap.get(1) {
                intents.push(TestIntent {
                    title: humanize(name.as_str()),
                });
                if intents.len() >= MAX_INTENTS_PER_FILE {
                    break;
                }
            }
        }
    } else if filename.ends_with(".py") {
        for cap in PYTHON_TEST_RE.captures_iter(content) {
            if let Some(name) = cap.get(1) {
                intents.push(TestIntent {
                    title: humanize(name.as_str()),
                });
                if intents.len() >= MAX_INTENTS_PER_FILE {
                    break;
                }
            }
        }
    } else if filename.ends_with(".go") {
        for cap in GO_TEST_RE.captures_iter(content) {
            if let Some(name) = cap.get(1) {
                intents.push(TestIntent {
                    title: humanize(name.as_str()),
                });
                if intents.len() >= MAX_INTENTS_PER_FILE {
                    break;
                }
            }
        }
    }

    intents
}

/// Enrich coupled files with test intents by reading test files from disk.
/// Silently ignores file read errors.
pub fn enrich_with_test_intents(repo_root: &Path, coupled_files: &mut [CoupledFile]) {
    for file in coupled_files.iter_mut() {
        if !is_test_file(&file.path) {
            continue;
        }

        let full_path = repo_root.join(&file.path);
        let Ok(content) = std::fs::read_to_string(&full_path) else {
            continue;
        };

        file.test_intents = extract_test_intents(&content, &file.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // --- is_test_file tests ---

    #[test]
    fn test_detects_js_ts_test_files() {
        assert!(is_test_file("src/Auth.test.ts"));
        assert!(is_test_file("src/Auth.spec.ts"));
        assert!(is_test_file("src/Auth.test.js"));
        assert!(is_test_file("src/Auth.spec.js"));
        assert!(is_test_file("src/Auth.test.tsx"));
        assert!(is_test_file("src/Auth.spec.tsx"));
        assert!(is_test_file("src/Auth.test.jsx"));
        assert!(is_test_file("src/Auth.spec.jsx"));
    }

    #[test]
    fn test_detects_go_test_files() {
        assert!(is_test_file("pkg/auth/auth_test.go"));
        assert!(is_test_file("handler_test.go"));
    }

    #[test]
    fn test_detects_python_test_files() {
        assert!(is_test_file("tests/test_auth.py"));
        assert!(is_test_file("tests/auth_test.py"));
    }

    #[test]
    fn test_detects_rust_test_dirs() {
        assert!(is_test_file("src/tests/integration.rs"));
        assert!(is_test_file("crate/tests/helpers.rs"));
    }

    #[test]
    fn test_rejects_non_test_files() {
        assert!(!is_test_file("src/Auth.ts"));
        assert!(!is_test_file("src/main.rs"));
        assert!(!is_test_file("pkg/auth/auth.go"));
        assert!(!is_test_file("src/utils.py"));
        assert!(!is_test_file("README.md"));
    }

    // --- extract_test_intents tests ---

    #[test]
    fn test_extracts_js_it_and_test_blocks() {
        let content = r#"
describe("Auth", () => {
  it('should login with valid credentials', () => {});
  it("should reject invalid password", () => {});
  test('should handle OAuth callback', () => {});
});
"#;
        let intents = extract_test_intents(content, "src/Auth.test.ts");
        assert_eq!(intents.len(), 3);
        assert_eq!(intents[0].title, "should login with valid credentials");
        assert_eq!(intents[1].title, "should reject invalid password");
        assert_eq!(intents[2].title, "should handle OAuth callback");
    }

    #[test]
    fn test_extracts_rust_test_fns() {
        let content = r#"
#[cfg(test)]
mod tests {
    #[test]
    fn test_auth_flow() {
        assert!(true);
    }

    #[test]
    fn test_session_expiry() {
        assert!(true);
    }
}
"#;
        let intents = extract_test_intents(content, "src/tests/auth.rs");
        assert_eq!(intents.len(), 2);
        assert_eq!(intents[0].title, "auth flow");
        assert_eq!(intents[1].title, "session expiry");
    }

    #[test]
    fn test_extracts_python_test_defs() {
        let content = r#"
def test_login_success(client):
    pass

def test_login_failure(client):
    pass

def helper_function():
    pass
"#;
        let intents = extract_test_intents(content, "tests/test_auth.py");
        assert_eq!(intents.len(), 2);
        assert_eq!(intents[0].title, "login success");
        assert_eq!(intents[1].title, "login failure");
    }

    #[test]
    fn test_extracts_go_test_funcs() {
        let content = r#"
func TestLoginSuccess(t *testing.T) {}
func TestSessionExpiry(t *testing.T) {}
func helperFunc() {}
"#;
        let intents = extract_test_intents(content, "auth_test.go");
        assert_eq!(intents.len(), 2);
        assert_eq!(intents[0].title, "LoginSuccess");
        assert_eq!(intents[1].title, "SessionExpiry");
    }

    #[test]
    fn test_caps_at_five() {
        let content = r#"
describe("Many tests", () => {
  it('test 1', () => {});
  it('test 2', () => {});
  it('test 3', () => {});
  it('test 4', () => {});
  it('test 5', () => {});
  it('test 6', () => {});
  it('test 7', () => {});
});
"#;
        let intents = extract_test_intents(content, "src/Auth.test.ts");
        assert_eq!(intents.len(), 5);
    }

    #[test]
    fn test_returns_empty_for_non_test_extension() {
        let content = "some random content";
        let intents = extract_test_intents(content, "src/Auth.txt");
        assert!(intents.is_empty());
    }

    // --- enrich_with_test_intents tests ---

    #[test]
    fn test_enrich_adds_intents_to_test_files() {
        let tmp = TempDir::new().unwrap();
        let test_dir = tmp.path().join("src");
        fs::create_dir_all(&test_dir).unwrap();

        let test_content = r#"
describe("Auth", () => {
  it('should login', () => {});
  it('should logout', () => {});
});
"#;
        fs::write(test_dir.join("Auth.test.ts"), test_content).unwrap();

        let mut files = vec![CoupledFile {
            path: "src/Auth.test.ts".to_string(),
            coupling_score: 0.8,
            co_change_count: 20,
            risk_score: 0.75,
            memories: Vec::new(),
            test_intents: Vec::new(),
        }];

        enrich_with_test_intents(tmp.path(), &mut files);

        assert_eq!(files[0].test_intents.len(), 2);
        assert_eq!(files[0].test_intents[0].title, "should login");
        assert_eq!(files[0].test_intents[1].title, "should logout");
    }

    #[test]
    fn test_enrich_skips_non_test_files() {
        let tmp = TempDir::new().unwrap();

        let mut files = vec![CoupledFile {
            path: "src/Auth.ts".to_string(),
            coupling_score: 0.8,
            co_change_count: 20,
            risk_score: 0.75,
            memories: Vec::new(),
            test_intents: Vec::new(),
        }];

        enrich_with_test_intents(tmp.path(), &mut files);
        assert!(files[0].test_intents.is_empty());
    }

    #[test]
    fn test_enrich_handles_missing_files() {
        let tmp = TempDir::new().unwrap();

        let mut files = vec![CoupledFile {
            path: "src/Deleted.test.ts".to_string(),
            coupling_score: 0.8,
            co_change_count: 20,
            risk_score: 0.75,
            memories: Vec::new(),
            test_intents: Vec::new(),
        }];

        enrich_with_test_intents(tmp.path(), &mut files);
        assert!(files[0].test_intents.is_empty());
    }
}

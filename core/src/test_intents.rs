use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::types::{CoupledFile, DiscoveredTestFile, TestInfo, TestIntent};

const MAX_INTENTS_PER_FILE: usize = 5;

// Compiled regexes for test title extraction
static JS_TEST_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?:^|\s)(?:it|test)\(\s*(?:'([^']*)'|"([^"]*)"|`([^`]*)`)"#).unwrap()
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

/// Language classification for test regex selection.
enum TestLang {
    JsTs,
    Rust,
    Python,
    Go,
}

/// Select the appropriate test language and regex for a file path.
fn detect_test_language(path: &str) -> Option<(TestLang, &'static Regex)> {
    let filename = Path::new(path)
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("");

    if filename.ends_with(".ts")
        || filename.ends_with(".tsx")
        || filename.ends_with(".js")
        || filename.ends_with(".jsx")
    {
        Some((TestLang::JsTs, &JS_TEST_RE))
    } else if filename.ends_with(".rs") || path.contains("/tests/") {
        Some((TestLang::Rust, &RUST_TEST_RE))
    } else if filename.ends_with(".py") {
        Some((TestLang::Python, &PYTHON_TEST_RE))
    } else if filename.ends_with(".go") {
        Some((TestLang::Go, &GO_TEST_RE))
    } else {
        None
    }
}

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

    // JS/TS: files inside a __tests__/ directory
    if path.contains("__tests__/")
        && (filename.ends_with(".ts")
            || filename.ends_with(".tsx")
            || filename.ends_with(".js")
            || filename.ends_with(".jsx"))
    {
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
    let Some((lang, re)) = detect_test_language(path) else {
        return Vec::new();
    };

    let mut intents: Vec<TestIntent> = Vec::new();

    for cap in re.captures_iter(content) {
        let title = match lang {
            // JS/TS uses three capture groups (single-quote, double-quote, backtick)
            TestLang::JsTs => cap.get(1).or_else(|| cap.get(2)).or_else(|| cap.get(3)).map(|m| m.as_str().to_string()),
            // All other languages use group 1 with humanized names
            _ => cap.get(1).map(|m| humanize(m.as_str())),
        };
        if let Some(t) = title {
            intents.push(TestIntent { title: t });
            if intents.len() >= MAX_INTENTS_PER_FILE {
                break;
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

/// Find test files for a source file by naming convention, independent of git coupling.
/// Checks candidate paths on disk and returns relative paths that exist.
pub fn find_test_files(repo_root: &Path, source_path: &str) -> Vec<String> {
    // Don't find tests for test files themselves
    if is_test_file(source_path) {
        return Vec::new();
    }

    let path = Path::new(source_path);
    let parent = path.parent().unwrap_or(Path::new(""));
    let Some(filename) = path.file_name().and_then(|f| f.to_str()) else {
        return Vec::new();
    };

    let mut candidates: Vec<String> = Vec::new();

    if let Some(stem) = filename.strip_suffix(".tsx")
        .or_else(|| filename.strip_suffix(".ts"))
        .or_else(|| filename.strip_suffix(".jsx"))
        .or_else(|| filename.strip_suffix(".js"))
    {
        let exts = ["tsx", "ts", "jsx", "js"];
        let tests_dir = parent.join("__tests__");
        for ext in &exts {
            candidates.push(parent.join(format!("{stem}.test.{ext}")).display().to_string());
            candidates.push(parent.join(format!("{stem}.spec.{ext}")).display().to_string());
            candidates.push(tests_dir.join(format!("{stem}.test.{ext}")).display().to_string());
            candidates.push(tests_dir.join(format!("{stem}.spec.{ext}")).display().to_string());
            candidates.push(tests_dir.join(format!("{stem}.{ext}")).display().to_string());
        }
    } else if let Some(stem) = filename.strip_suffix(".py") {
        candidates.push(parent.join(format!("test_{stem}.py")).display().to_string());
        candidates.push(parent.join(format!("{stem}_test.py")).display().to_string());
        candidates.push(parent.join("tests").join(format!("test_{stem}.py")).display().to_string());
        // Root-level tests/ directory mirroring src/ structure
        candidates.push(Path::new("tests").join(format!("test_{stem}.py")).display().to_string());
    } else if let Some(stem) = filename.strip_suffix(".go") {
        candidates.push(parent.join(format!("{stem}_test.go")).display().to_string());
    } else if let Some(stem) = filename.strip_suffix(".rs") {
        candidates.push(parent.join("tests").join(format!("{stem}.rs")).display().to_string());
        // Crate-level tests directory
        candidates.push(Path::new("tests").join(format!("{stem}.rs")).display().to_string());
    }

    // Deduplicate and check which candidates exist on disk
    let mut found: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for candidate in &candidates {
        if !seen.insert(candidate.clone()) {
            continue;
        }
        if repo_root.join(candidate).is_file() {
            found.push(candidate.clone());
        }
    }

    found
}

/// Count the total number of test cases in file content (no cap).
pub fn count_test_cases(content: &str, path: &str) -> u32 {
    detect_test_language(path)
        .map(|(_, re)| re.captures_iter(content).count() as u32)
        .unwrap_or(0)
}

/// Discover test files for a source file and build a TestInfo with coverage hint.
pub fn discover_test_info(repo_root: &Path, source_path: &str) -> Option<TestInfo> {
    let test_paths = find_test_files(repo_root, source_path);
    if test_paths.is_empty() {
        return None;
    }

    let mut test_files: Vec<DiscoveredTestFile> = Vec::new();
    let mut total_tests: u32 = 0;

    for test_path in &test_paths {
        let full_path = repo_root.join(test_path);
        let Ok(content) = std::fs::read_to_string(&full_path) else {
            continue;
        };

        let test_count = count_test_cases(&content, test_path);
        let intents = extract_test_intents(&content, test_path);
        total_tests += test_count;

        test_files.push(DiscoveredTestFile {
            path: test_path.clone(),
            test_intents: intents,
            test_count,
        });
    }

    if test_files.is_empty() {
        return None;
    }

    // Build coverage hint based on source file line count
    let source_full = repo_root.join(source_path);
    let coverage_hint = std::fs::read_to_string(&source_full)
        .ok()
        .map(|content| {
            let line_count = content.lines().count();
            format!(
                "{total_tests} test{} covering a {line_count}-line source file",
                if total_tests == 1 { "" } else { "s" },
            )
        });

    Some(TestInfo {
        test_files,
        coverage_hint,
    })
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

    // --- is_test_file __tests__/ tests ---

    #[test]
    fn test_detects_js_ts_in_dunder_tests_dir() {
        assert!(is_test_file("src/__tests__/Auth.tsx"));
        assert!(is_test_file("src/__tests__/Auth.ts"));
        assert!(is_test_file("src/__tests__/Auth.test.tsx"));
        assert!(is_test_file("src/components/__tests__/Button.jsx"));
    }

    #[test]
    fn test_rejects_non_js_in_dunder_tests_dir() {
        // A .py file inside __tests__/ should not match via JS rule
        // (it should only match via Python rules)
        assert!(!is_test_file("src/__tests__/readme.md"));
    }

    // --- find_test_files tests ---

    #[test]
    fn test_find_colocated_test_tsx() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("Auth.tsx"), "export class Auth {}").unwrap();
        fs::write(src.join("Auth.test.tsx"), "it('works', () => {})").unwrap();

        let found = find_test_files(tmp.path(), "src/Auth.tsx");
        assert_eq!(found, vec!["src/Auth.test.tsx"]);
    }

    #[test]
    fn test_find_dunder_tests_dir() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        let tests = tmp.path().join("src/__tests__");
        fs::create_dir_all(&src).unwrap();
        fs::create_dir_all(&tests).unwrap();
        fs::write(src.join("Auth.tsx"), "export class Auth {}").unwrap();
        fs::write(tests.join("Auth.test.tsx"), "it('works', () => {})").unwrap();

        let found = find_test_files(tmp.path(), "src/Auth.tsx");
        assert_eq!(found, vec!["src/__tests__/Auth.test.tsx"]);
    }

    #[test]
    fn test_find_spec_variant() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("Auth.tsx"), "export class Auth {}").unwrap();
        fs::write(src.join("Auth.spec.tsx"), "it('works', () => {})").unwrap();

        let found = find_test_files(tmp.path(), "src/Auth.tsx");
        assert_eq!(found, vec!["src/Auth.spec.tsx"]);
    }

    #[test]
    fn test_find_multiple_matches() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        let tests = tmp.path().join("src/__tests__");
        fs::create_dir_all(&src).unwrap();
        fs::create_dir_all(&tests).unwrap();
        fs::write(src.join("Auth.tsx"), "export class Auth {}").unwrap();
        fs::write(src.join("Auth.test.tsx"), "it('a', () => {})").unwrap();
        fs::write(tests.join("Auth.test.tsx"), "it('b', () => {})").unwrap();

        let found = find_test_files(tmp.path(), "src/Auth.tsx");
        assert_eq!(found.len(), 2);
        assert!(found.contains(&"src/Auth.test.tsx".to_string()));
        assert!(found.contains(&"src/__tests__/Auth.test.tsx".to_string()));
    }

    #[test]
    fn test_find_cross_extension() {
        // Source is .tsx but test is .test.ts
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("Auth.tsx"), "export class Auth {}").unwrap();
        fs::write(src.join("Auth.test.ts"), "it('works', () => {})").unwrap();

        let found = find_test_files(tmp.path(), "src/Auth.tsx");
        assert_eq!(found, vec!["src/Auth.test.ts"]);
    }

    #[test]
    fn test_find_python_prefix() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("auth.py"), "class Auth: pass").unwrap();
        fs::write(src.join("test_auth.py"), "def test_login(): pass").unwrap();

        let found = find_test_files(tmp.path(), "src/auth.py");
        assert_eq!(found, vec!["src/test_auth.py"]);
    }

    #[test]
    fn test_find_python_suffix() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("auth.py"), "class Auth: pass").unwrap();
        fs::write(src.join("auth_test.py"), "def test_login(): pass").unwrap();

        let found = find_test_files(tmp.path(), "src/auth.py");
        assert_eq!(found, vec!["src/auth_test.py"]);
    }

    #[test]
    fn test_find_python_tests_dir() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        let tests = tmp.path().join("src/tests");
        fs::create_dir_all(&src).unwrap();
        fs::create_dir_all(&tests).unwrap();
        fs::write(src.join("auth.py"), "class Auth: pass").unwrap();
        fs::write(tests.join("test_auth.py"), "def test_login(): pass").unwrap();

        let found = find_test_files(tmp.path(), "src/auth.py");
        assert_eq!(found, vec!["src/tests/test_auth.py"]);
    }

    #[test]
    fn test_find_python_root_tests_dir() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("app/logic");
        let tests = tmp.path().join("tests");
        fs::create_dir_all(&src).unwrap();
        fs::create_dir_all(&tests).unwrap();
        fs::write(src.join("auth.py"), "class Auth: pass").unwrap();
        fs::write(tests.join("test_auth.py"), "def test_login(): pass").unwrap();

        let found = find_test_files(tmp.path(), "app/logic/auth.py");
        assert_eq!(found, vec!["tests/test_auth.py"]);
    }

    #[test]
    fn test_find_go_test() {
        let tmp = TempDir::new().unwrap();
        let pkg = tmp.path().join("pkg/auth");
        fs::create_dir_all(&pkg).unwrap();
        fs::write(pkg.join("auth.go"), "package auth").unwrap();
        fs::write(pkg.join("auth_test.go"), "func TestLogin(t *testing.T) {}").unwrap();

        let found = find_test_files(tmp.path(), "pkg/auth/auth.go");
        assert_eq!(found, vec!["pkg/auth/auth_test.go"]);
    }

    #[test]
    fn test_find_rust_tests_dir() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        let tests = tmp.path().join("tests");
        fs::create_dir_all(&src).unwrap();
        fs::create_dir_all(&tests).unwrap();
        fs::write(src.join("auth.rs"), "pub fn login() {}").unwrap();
        fs::write(tests.join("auth.rs"), "#[test] fn test_login() {}").unwrap();

        let found = find_test_files(tmp.path(), "src/auth.rs");
        assert_eq!(found, vec!["tests/auth.rs"]);
    }

    #[test]
    fn test_find_no_matches() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("Auth.tsx"), "export class Auth {}").unwrap();

        let found = find_test_files(tmp.path(), "src/Auth.tsx");
        assert!(found.is_empty());
    }

    #[test]
    fn test_find_skips_test_of_test() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("Auth.test.tsx"), "it('works', () => {})").unwrap();

        let found = find_test_files(tmp.path(), "src/Auth.test.tsx");
        assert!(found.is_empty());
    }

    // --- count_test_cases tests ---

    #[test]
    fn test_count_uncapped() {
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
        let count = count_test_cases(content, "src/Auth.test.ts");
        assert_eq!(count, 7);
    }

    #[test]
    fn test_count_python() {
        let content = r#"
def test_a(): pass
def test_b(): pass
def test_c(): pass
def helper(): pass
"#;
        let count = count_test_cases(content, "test_foo.py");
        assert_eq!(count, 3);
    }

    #[test]
    fn test_count_go() {
        let content = r#"
func TestA(t *testing.T) {}
func TestB(t *testing.T) {}
func helper() {}
"#;
        let count = count_test_cases(content, "foo_test.go");
        assert_eq!(count, 2);
    }

    #[test]
    fn test_count_rust() {
        let content = r#"
#[test]
fn test_a() {}
#[test]
fn test_b() {}
fn helper() {}
"#;
        let count = count_test_cases(content, "tests/foo.rs");
        assert_eq!(count, 2);
    }

    // --- discover_test_info tests ---

    #[test]
    fn test_discover_test_info_full() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        fs::create_dir_all(&src).unwrap();

        // Source file: 10 lines
        let source = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\n";
        fs::write(src.join("Auth.tsx"), source).unwrap();

        let test_content = r#"
describe("Auth", () => {
  it('should login', () => {});
  it('should logout', () => {});
  it('should refresh', () => {});
});
"#;
        fs::write(src.join("Auth.test.tsx"), test_content).unwrap();

        let info = discover_test_info(tmp.path(), "src/Auth.tsx");
        assert!(info.is_some());
        let info = info.unwrap();

        assert_eq!(info.test_files.len(), 1);
        assert_eq!(info.test_files[0].path, "src/Auth.test.tsx");
        assert_eq!(info.test_files[0].test_count, 3);
        assert_eq!(info.test_files[0].test_intents.len(), 3);

        let hint = info.coverage_hint.unwrap();
        assert!(hint.contains("3 tests"));
        assert!(hint.contains("10-line source file"));
    }

    #[test]
    fn test_discover_test_info_none_when_no_tests() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("Auth.tsx"), "export class Auth {}").unwrap();

        let info = discover_test_info(tmp.path(), "src/Auth.tsx");
        assert!(info.is_none());
    }

    #[test]
    fn test_discover_test_info_singular() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("Auth.tsx"), "export class Auth {}").unwrap();

        let test_content = "it('should login', () => {});";
        fs::write(src.join("Auth.test.tsx"), test_content).unwrap();

        let info = discover_test_info(tmp.path(), "src/Auth.tsx").unwrap();
        let hint = info.coverage_hint.unwrap();
        assert!(hint.contains("1 test covering"));
    }

    #[test]
    fn test_count_unknown_extension() {
        let count = count_test_cases("some content", "README.md");
        assert_eq!(count, 0);
    }

    #[test]
    fn test_count_empty_content() {
        let count = count_test_cases("", "src/Auth.test.ts");
        assert_eq!(count, 0);
    }

    #[test]
    fn test_find_root_level_file() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("auth.py"), "class Auth: pass").unwrap();
        fs::write(tmp.path().join("test_auth.py"), "def test_login(): pass").unwrap();

        let found = find_test_files(tmp.path(), "auth.py");
        assert_eq!(found, vec!["test_auth.py"]);
    }

    #[test]
    fn test_extracts_js_backtick_test_names() {
        let content = r#"
describe("Auth", () => {
  it(`should handle template literal name`, () => {});
  test(`should also work with test()`, () => {});
});
"#;
        let intents = extract_test_intents(content, "src/Auth.test.ts");
        assert_eq!(intents.len(), 2);
        assert_eq!(intents[0].title, "should handle template literal name");
        assert_eq!(intents[1].title, "should also work with test()");
    }

    #[test]
    fn test_counts_backtick_test_names() {
        let content = r#"
describe("Mixed", () => {
  it('single quoted', () => {});
  it("double quoted", () => {});
  it(`backtick quoted`, () => {});
  test(`another backtick`, () => {});
});
"#;
        let count = count_test_cases(content, "src/Auth.test.ts");
        assert_eq!(count, 4);
    }

    #[test]
    fn test_mixed_quote_styles_in_extraction() {
        let content = r#"
describe("Suite", () => {
  it('single', () => {});
  it("double", () => {});
  it(`backtick`, () => {});
});
"#;
        let intents = extract_test_intents(content, "src/Auth.test.ts");
        assert_eq!(intents.len(), 3);
        assert_eq!(intents[0].title, "single");
        assert_eq!(intents[1].title, "double");
        assert_eq!(intents[2].title, "backtick");
    }

    #[test]
    fn test_discover_test_info_empty_test_file() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("Auth.tsx"), "export class Auth {}").unwrap();
        // Test file exists but contains no test cases
        fs::write(src.join("Auth.test.tsx"), "// TODO: add tests").unwrap();

        let info = discover_test_info(tmp.path(), "src/Auth.tsx");
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.test_files[0].test_count, 0);
        assert!(info.test_files[0].test_intents.is_empty());
        let hint = info.coverage_hint.unwrap();
        assert!(hint.contains("0 tests"));
    }
}

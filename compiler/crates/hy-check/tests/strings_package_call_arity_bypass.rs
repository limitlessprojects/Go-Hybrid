//! Regression tests for foreign package function calls (`strings.*`).
//! Enforces call arity constraints and non-callable member checks during `hyb check` (HYB-E1002, Row 13 BUILDABLE, fixes #4184).

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn strings_non_callable_types_rejected_e1002() {
    let cases = [
        "fn main() { strings.reader() }",
        "fn main() { strings.replacer() }",
    ];
    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject);
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for: {src}, got: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn strings_arity_mismatch_rejected_e1002() {
    let cases = [
        // 0-arg function called with args
        "fn main() { strings.builder(1) }",
        "fn main() { strings.builder(1, 2) }",
        // 1-arg functions called with 0 or 2 args
        "fn main() { strings.clone() }",
        "fn main() { strings.clone(\"a\", \"b\") }",
        "fn main() { strings.fields() }",
        "fn main() { strings.fields(\"a\", \"b\") }",
        "fn main() { strings.to_lower() }",
        "fn main() { strings.to_lower(\"a\", \"b\") }",
        "fn main() { strings.to_upper() }",
        "fn main() { strings.to_upper(\"a\", \"b\") }",
        "fn main() { strings.trim_space() }",
        "fn main() { strings.trim_space(\"a\", \"b\") }",
        "fn main() { strings.new_reader() }",
        "fn main() { strings.new_reader(\"a\", \"b\") }",
        // 2-arg functions called with 1 or 3 args
        "fn main() { strings.cut(\"a\") }",
        "fn main() { strings.cut(\"a\", \"b\", \"c\") }",
        "fn main() { strings.contains(\"a\") }",
        "fn main() { strings.contains(\"a\", \"b\", \"c\") }",
        "fn main() { strings.contains_any(\"a\") }",
        "fn main() { strings.contains_any(\"a\", \"b\", \"c\") }",
        "fn main() { strings.has_prefix(\"a\") }",
        "fn main() { strings.has_prefix(\"a\", \"b\", \"c\") }",
        "fn main() { strings.has_suffix(\"a\") }",
        "fn main() { strings.has_suffix(\"a\", \"b\", \"c\") }",
        "fn main() { strings.split(\"a\") }",
        "fn main() { strings.split(\"a\", \"b\", \"c\") }",
        "fn main() { strings.join([\"a\"]) }",
        "fn main() { strings.join([\"a\"], \",\", \"extra\") }",
        "fn main() { strings.count(\"a\") }",
        "fn main() { strings.count(\"a\", \"b\", \"c\") }",
        "fn main() { strings.index(\"a\") }",
        "fn main() { strings.index(\"a\", \"b\", \"c\") }",
        "fn main() { strings.repeat(\"a\") }",
        "fn main() { strings.repeat(\"a\", 2, 3) }",
        "fn main() { strings.trim_prefix(\"a\") }",
        "fn main() { strings.trim_prefix(\"a\", \"b\", \"c\") }",
        "fn main() { strings.trim_suffix(\"a\") }",
        "fn main() { strings.trim_suffix(\"a\", \"b\", \"c\") }",
        // 3-arg functions called with 2 or 4 args
        "fn main() { strings.split_n(\"a\", \"b\") }",
        "fn main() { strings.split_n(\"a\", \"b\", 2, 3) }",
        "fn main() { strings.replace_all(\"a\", \"b\") }",
        "fn main() { strings.replace_all(\"a\", \"b\", \"c\", \"d\") }",
        // 4-arg function called with 3 or 5 args
        "fn main() { strings.replace(\"a\", \"b\", \"c\") }",
        "fn main() { strings.replace(\"a\", \"b\", \"c\", 1, 2) }",
    ];
    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject);
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for: {src}, got: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn strings_non_variadic_spread_rejected_e1002() {
    let cases = [
        "fn main() {\n    var xs = [\"a\"]\n    strings.clone(xs...)\n}",
        "fn main() {\n    var xs = [\"a\", \"b\"]\n    strings.split(xs...)\n}",
        "fn main() {\n    var xs = [\"a\", \"b\", \"c\"]\n    strings.replace_all(xs...)\n}",
    ];
    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject);
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for: {src}, got: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn strings_valid_calls_accepted() {
    let src = r#"
fn test_strings() {
    _ = strings.builder()
    _ = strings.clone("hello")
    _ = strings.to_lower("HELLO")
    _ = strings.to_upper("hello")
    _ = strings.trim_space("  hello  ")
    _ = strings.contains("hello world", "world")
    _ = strings.has_prefix("hello world", "hello")
    _ = strings.has_suffix("hello world", "world")
    _ = strings.split("a,b,c", ",")
    _ = strings.join(["a", "b"], ",")
    _ = strings.replace_all("banana", "a", "o")
    _ = strings.replace("banana", "a", "o", 1)
    _ = strings.new_reader("data")
}
"#;
    let r = check(src);
    assert_eq!(error_codes(&r), Vec::<String>::new());
    assert_eq!(r.verdict, Verdict::Accept);
}

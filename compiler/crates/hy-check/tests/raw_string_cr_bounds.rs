//! Constant bounds must use the same raw-string bytes as emitted Go.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("raw-cr.hy", src).expect("valid Hybrid source");
    check_file(&ast)
}

#[test]
fn raw_cr_constant_indices_and_slices_reject_after_value_normalization() {
    for (src, byte_len) in [
        ("fn main() { fmt.println(`a\rb`[2]) }", 2),
        ("fn main() { const s = `a\rb` fmt.println(s[2]) }", 2),
        ("fn main() { fmt.println(`a\rb`[0:3]) }", 2),
        ("fn main() { fmt.println(`\r`[0]) }", 0),
        ("fn main() { fmt.println(`a\r\nb`[3]) }", 3),
        ("fn main() { const s = `a\r` + `b` fmt.println(s[2]) }", 2),
    ] {
        let result = check(src);
        assert_eq!(result.verdict, Verdict::Reject, "source: {src:?}");
        let length = format!("{byte_len}-byte string");
        assert!(
            result.diagnostics.iter().any(|diagnostic| {
                diagnostic.severity == Severity::Error
                    && diagnostic.code == "HYB-E1002"
                    && diagnostic.message.contains(&length)
            }),
            "expected string bounds error for {length}; source: {src:?}; diagnostics: {:?}",
            result.diagnostics
        );
    }
}

#[test]
fn raw_cr_bounds_controls_and_interpreted_escape_stay_accepted() {
    for src in [
        "fn main() { fmt.println(`a\rb`[1]) }",
        "fn main() { fmt.println(`a\r\nb`[2]) }",
        "fn main() { fmt.println(`a\nb`[2]) }",
        "fn main() { fmt.println(\"a\\rb\"[2]) }",
        "fn main() { fmt.println((`a\rb` + \"\\r\")[2]) }",
        "fn main() { fmt.println(`a\\rb`[3]) }",
        "fn main() { fmt.println(`a\rb`[0:2]) }",
        "fn main() { fmt.println(`\r`[0:0]) }",
        "fn main() { fmt.println(`ab`[1]) }",
    ] {
        let result = check(src);
        assert_eq!(
            result.verdict,
            Verdict::Accept,
            "source: {src:?}; diagnostics: {:?}",
            result.diagnostics
        );
    }
}

#[test]
fn raw_cr_normalization_exposes_duplicate_map_keys() {
    let src = "fn main() { m := map[string]int{`a\rb`: 1, \"ab\": 2} fmt.println(len(m)) }";
    let result = check(src);
    assert_eq!(result.verdict, Verdict::Reject);
    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic.code == "HYB-E1002"
            && diagnostic.message.contains("duplicate key")
    }));
}

#[test]
fn raw_and_interpreted_carriage_returns_remain_distinct_keys() {
    let src = "fn main() { m := map[string]int{`a\rb`: 1, \"a\\rb\": 2} fmt.println(len(m)) }";
    let result = check(src);
    assert_eq!(result.verdict, Verdict::Accept, "{:?}", result.diagnostics);
}

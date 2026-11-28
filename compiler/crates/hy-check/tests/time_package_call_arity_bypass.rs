//! Regression tests for foreign package function calls (`time.*`).
//! Enforces call arity constraints and non-callable member checks during `hyb check` (HYB-E1002, Row 13 BUILDABLE, fixes #4182).

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
fn time_non_callable_members_rejected_e1002() {
    let cases = [
        "fn main() { time.nanosecond() }",
        "fn main() { time.microsecond() }",
        "fn main() { time.millisecond() }",
        "fn main() { time.second() }",
        "fn main() { time.minute() }",
        "fn main() { time.hour() }",
        "fn main() { time.layout() }",
        "fn main() { time.ansic() }",
        "fn main() { time.rfc3339() }",
        "fn main() { time.time() }",
        "fn main() { time.duration() }",
        "fn main() { time.timer() }",
        "fn main() { time.ticker() }",
        "fn main() { time.utc() }",
        "fn main() { time.local() }",
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
fn time_arity_mismatch_rejected_e1002() {
    let cases = [
        // 0-arg function called with args
        "fn main() { time.now(1) }",
        "fn main() { time.now(1, 2) }",
        // 1-arg functions called with 0 or 2 args
        "fn main() { time.sleep() }",
        "fn main() { time.sleep(1, 2) }",
        "fn main() { time.since() }",
        "fn main() { time.since(1, 2) }",
        "fn main() { time.until() }",
        "fn main() { time.until(1, 2) }",
        "fn main() { time.after() }",
        "fn main() { time.after(1, 2) }",
        "fn main() { time.tick() }",
        "fn main() { time.tick(1, 2) }",
        "fn main() { time.new_ticker() }",
        "fn main() { time.new_ticker(1, 2) }",
        "fn main() { time.new_timer() }",
        "fn main() { time.new_timer(1, 2) }",
        "fn main() { time.parse_duration() }",
        "fn main() { time.parse_duration(\"1s\", \"extra\") }",
        // 2-arg functions called with 1 or 3 args
        "fn main() { time.unix(1) }",
        "fn main() { time.unix(1, 2, 3) }",
        "fn main() { time.parse(\"2006-01-02\") }",
        "fn main() { time.parse(\"2006-01-02\", \"2020-01-01\", \"extra\") }",
        // 3-arg function called with 2 or 4 args
        "fn main() { time.parse_in_location(\"layout\", \"val\") }",
        "fn main() { time.parse_in_location(\"layout\", \"val\", 1, 2) }",
        // 8-arg function called with 7 or 9 args
        "fn main() { time.date(2020, 1, 1, 0, 0, 0, 0) }",
        "fn main() { time.date(2020, 1, 1, 0, 0, 0, 0, 1, 2) }",
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
fn time_non_variadic_spread_rejected_e1002() {
    let cases = [
        "fn main() {\n    var xs = [1]\n    time.sleep(xs...)\n}",
        "fn main() {\n    var xs = [\"1s\"]\n    time.parse_duration(xs...)\n}",
        "fn main() {\n    var xs = [1, 2]\n    time.unix(xs...)\n}",
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
fn time_valid_calls_accepted() {
    let src = r#"
fn test_time() {
    t := time.now()
    _ = time.since(t)
    _ = time.until(t)
    _ = time.seconds(10)
    _ = time.milliseconds(500)
    _ = time.unix(100, 0)
    _, _ = time.parse_duration("10s")
}
"#;
    let r = check(src);
    assert_eq!(error_codes(&r), Vec::<String>::new());
    assert_eq!(r.verdict, Verdict::Accept);
}

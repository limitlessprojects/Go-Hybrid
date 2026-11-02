//! option_operand_kind_2823.rs — issue #2823 (Row 13 BUILDABLE).
//!
//! `TypeExpr::Option` resolved to `OpKind::Unknown`, which is fail-open in every
//! operand guard, and `some(v)` / `unwrap()` / `or(d)` had no result kind. So
//! `op + 1`, `takes_int(op)`, `var s string = op.unwrap()` and
//! `var b option[string] = some(1)` passed `hyb check` and failed `go build`.
//! `OpKind::Option(Box<OpKind>)` now carries the payload kind.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn option_operands_and_payload_mismatches_reject() {
    // The issue's own reproducer.
    let r = check(
        "fn mk() option[int] {\n    return some(1)\n}\nfn takes_int(x int) {\n    fmt.println(\"{x}\")\n}\nfn main() {\n    op := mk()\n    z := op + 1\n    fmt.println(\"{z}\")\n    takes_int(op)\n    var s string = op.unwrap()\n    fmt.println(s)\n    var b option[string] = some(1)\n    _ = b\n}\n",
    );
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().filter(|c| *c == "HYB-E1002").count() >= 4,
        "expected at least four HYB-E1002, got {codes:?}"
    );

    for (name, src) in [
        (
            "option as int operand",
            "fn mk() option[int] {\n    return some(1)\n}\nfn main() {\n    op := mk()\n    _ = op + 1\n}\n",
        ),
        (
            "option as int argument",
            "fn takes_int(x int) {\n    fmt.println(\"{x}\")\n}\nfn main() {\n    var op option[int] = some(1)\n    takes_int(op)\n}\n",
        ),
        (
            "unwrapped payload into string",
            "fn mk() option[int] {\n    return some(1)\n}\nfn main() {\n    op := mk()\n    var s string = op.unwrap()\n    _ = s\n}\n",
        ),
        (
            "some(1) into option[string]",
            "fn main() {\n    var b option[string] = some(1)\n    _ = b\n}\n",
        ),
        (
            "option[int] assigned to option[string]",
            "fn main() {\n    var a option[int] = some(1)\n    var b option[string] = some(\"x\")\n    b = a\n    _ = b\n}\n",
        ),
        (
            "option argument for an option[string] parameter",
            "fn f(o option[string]) {\n    _ = o\n}\nfn main() {\n    f(some(1))\n}\n",
        ),
        (
            "option value as int",
            "fn main() {\n    var op option[int] = some(1)\n    var k int = op\n    _ = k\n}\n",
        ),
        (
            "or() payload does not add to a string",
            "fn mk() option[int] {\n    return some(1)\n}\nfn main() {\n    t := mk().or(2) + \"x\"\n    _ = t\n}\n",
        ),
        (
            "is_some() is bool, not int",
            "fn main() {\n    var op option[int] = some(1)\n    _ = op.is_some() + 1\n}\n",
        ),
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "{name}");
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "{name}: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn correct_option_use_stays_accepted() {
    for (name, src) in [
        (
            "payload used at its own type",
            "fn mk() option[int] {\n    return some(1)\n}\nfn main() {\n    op := mk()\n    var n int = op.unwrap()\n    _ = n + 1\n    _ = op.or(2)\n    if op.is_some() {\n        _ = 1\n    }\n}\n",
        ),
        (
            "some into the matching option type",
            "fn main() {\n    var x option[int] = some(1)\n    var y option[string] = some(\"s\")\n    _ = x\n    _ = y\n}\n",
        ),
        (
            "option parameter and match",
            "fn f(o option[int]) int {\n    match o {\n        some(v): {\n            return v\n        }\n        some(_): {\n            return 0\n        }\n        none: {\n            return 0\n        }\n    }\n}\nfn main() {\n    _ = f(some(1))\n}\n",
        ),
        (
            "option comparison against none",
            "fn main() {\n    var op option[int] = some(1)\n    if op.is_none() {\n        _ = 1\n    }\n}\n",
        ),
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Accept, "{name}: {:?}", r.diagnostics);
        assert!(error_codes(&r).is_empty(), "{name}: {:?}", r.diagnostics);
    }
}

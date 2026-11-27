//! range_counter_scalar_4661.rs — regression for issue #4661:
//! `for i in lo..hi` lowers to `for i := lo`, so the counter carries lo's
//! Go type. The Range arm bound the counter's *kind* (Int) but never its
//! scalar, so `take(i)` / `var y int64 = i` / `uint8(1) + i` passed
//! `hyb check` and failed `go build`.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn untyped_counter_rejects_int64_argument() {
    // Issue shape 1: counter is `int`, take wants int64.
    let r = check(
        "import \"fmt\"\n\nfn take(a int64) int64 { return a }\n\nfn main() {\n    for i in 0..3 {\n        fmt.println(take(i))\n    }\n}\n",
    );
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for take(i) on int counter, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn untyped_counter_rejects_mixed_width_add() {
    // Issue shape 2: `uint8(1) + i` mismatches int counter.
    let r = check(
        "import \"fmt\"\n\nfn main() {\n    for i in 0..3 {\n        y := uint8(1) + i\n        fmt.println(y)\n    }\n}\n",
    );
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for uint8(1) + i, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn untyped_counter_rejects_int64_decl() {
    let r = check(
        "import \"fmt\"\n\nfn main() {\n    for i in 0..3 {\n        var y int64 = i\n        fmt.println(y)\n    }\n}\n",
    );
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for `var y int64 = i`, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn inclusive_range_counter_rejects_int64_decl() {
    // `0..=3` shares the Range arm.
    let r = check(
        "import \"fmt\"\n\nfn main() {\n    for i in 0..=3 {\n        var y int64 = i\n        fmt.println(y)\n    }\n}\n",
    );
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for inclusive `var y int64 = i`, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn typed_lo_counter_keeps_declared_width() {
    // `n := int64(3)` makes the counter int64: take(i) must accept.
    let r = check(
        "import \"fmt\"\n\nfn take(a int64) int64 { return a }\n\nfn main() {\n    n := int64(3)\n    for i in n..n+2 {\n        fmt.println(take(i))\n    }\n}\n",
    );
    assert!(
        error_codes(&r).is_empty(),
        "expected typed int64 counter to accept take(i), got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn untyped_counter_into_matching_int_accepts() {
    // Over-reach control: counter into its own default type stays legal.
    let r = check(
        "import \"fmt\"\n\nfn main() {\n    for i in 0..3 {\n        var y int = i\n        fmt.println(y)\n    }\n}\n",
    );
    assert!(
        error_codes(&r).is_empty(),
        "expected `var y int = i` to accept, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn plain_counter_read_accepts() {
    let r = check(
        "import \"fmt\"\n\nfn main() {\n    for i in 0..3 {\n        fmt.println(i)\n    }\n}\n",
    );
    assert!(
        error_codes(&r).is_empty(),
        "expected plain counter loop to accept, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

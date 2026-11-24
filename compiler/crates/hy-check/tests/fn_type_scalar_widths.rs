//! fn_type_scalar_widths.rs: issue #4751 (Row 13 BUILDABLE).
//!
//! `OpKind::Fn` compatibility previously compared parameter and result kinds
//! (`Int`, `Float`, ...) without carrying canonical scalar widths, allowing
//! `fn(int64) int64` to satisfy `fn(int) int` in `hyb check` while failing
//! `go build`.

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
fn rejects_mismatched_scalar_widths_in_fn_signatures() {
    let cases = [
        (
            "fn(int64) int64 assigned to fn(int) int",
            "fn g(x int64) int64 {\n    return x\n}\n\nfn main() {\n    var f fn(int) int = g\n    _ = f\n}\n",
        ),
        (
            "fn(float64) float64 assigned to fn(float32) float32",
            "fn g(x float64) float64 {\n    return x\n}\n\nfn main() {\n    var f fn(float32) float32 = g\n    _ = f\n}\n",
        ),
        (
            "fn() int64 assigned to fn() int8",
            "fn g() int64 {\n    return 1\n}\n\nfn main() {\n    var f fn() int8 = g\n    _ = f\n}\n",
        ),
        (
            "closure fn(x int32) int32 passed to fn(fn(int64) int64)",
            "fn apply(cb fn(int64) int64) int64 {\n    return cb(1)\n}\n\nfn main() {\n    let cb = fn(x int32) int32 { return x }\n    _ = apply(cb)\n}\n",
        ),
        (
            "nested higher-order fn() fn(uint16) uint16 vs fn() fn(uint64) uint64",
            "fn maker() fn(uint16) uint16 {\n    return fn(x uint16) uint16 { return x }\n}\n\nfn main() {\n    var f fn() fn(uint64) uint64 = maker\n    _ = f\n}\n",
        ),
    ];

    for (label, src) in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "expected Reject for {label}, got Accept"
        );
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for {label}, got {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn accepts_matching_scalar_widths_and_go_aliases_in_fn_signatures() {
    let cases = [
        (
            "exact same width fn(int64) int64",
            "fn g(x int64) int64 {\n    return x\n}\n\nfn main() {\n    var f fn(int64) int64 = g\n    _ = f(1)\n}\n",
        ),
        (
            "Hybrid i64 alias matches Go int64",
            "fn g(x i64) i64 {\n    return x\n}\n\nfn main() {\n    var f fn(int64) int64 = g\n    _ = f(1)\n}\n",
        ),
        (
            "byte matches uint8 and rune matches int32",
            "fn g(b byte, r rune) uint8 {\n    _ = r\n    return b\n}\n\nfn main() {\n    var f fn(uint8, int32) byte = g\n    _ = f\n}\n",
        ),
    ];

    for (label, src) in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "expected Accept for {label}, got {:?}",
            r.diagnostics
        );
    }
}

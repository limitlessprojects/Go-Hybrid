//! Regression tests for issue #827: `for i in lo..hi` / `for i in lo..=hi`
//! must reject nil/bool/pointer/const-float bounds, mismatched integer bound
//! types, and overflowing constant integer bounds with HYB-E2009.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn rejects_e2009(src: &str) {
    let r = check(src);
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error && d.code == "HYB-E2009"),
        "expected HYB-E2009, got {:?}\nsrc:\n{}",
        r.diagnostics,
        src
    );
    assert_eq!(r.verdict, Verdict::Reject, "src:\n{}", src);
}

fn accepts(src: &str) {
    let r = check(src);
    assert!(
        r.verdict == Verdict::Accept,
        "expected Accept, got {:?}\nsrc:\n{}",
        r.diagnostics,
        src
    );
}

#[test]
fn nil_bool_pointer_and_const_float_range_bounds_reject_e2009() {
    rejects_e2009("fn main() { for i in 0..nil { _ = i } }");
    rejects_e2009("fn main() { for i in 0..(1 < 2) { _ = i } }");
    rejects_e2009("fn main() { b := false\n for i in (!b)..5 { _ = i } }");
    rejects_e2009("const f = 2.5\nfn main() { for i in 0..f { _ = i } }");
    rejects_e2009("fn main() { const f = 2.5\n for i in 0..f { _ = i } }");
}

#[test]
fn mismatched_integer_types_and_overflow_bounds_reject_e2009() {
    rejects_e2009("fn main() { var n u32 = 5\n for i in 0..n { _ = i } }");
    rejects_e2009("fn main() { var lo i32 = 0\n var hi i64 = 5\n for i in lo..hi { _ = i } }");
    rejects_e2009("fn main() { var lo i64 = 0\n for i in lo..=10 { _ = i } }");
    rejects_e2009("fn main() { for i in 0..(1 << 63) { _ = i } }");
    rejects_e2009("fn main() { var lo i8 = 0\n for i in lo..300 { _ = i } }");
}

#[test]
fn matching_integer_range_bounds_and_shadowed_consts_accept() {
    accepts("fn main() { var lo i64 = 0\n for i in lo..10 { _ = i } }");
    accepts("fn main() { var n u32 = 5\n for i in u32(0)..n { _ = i } }");
    accepts("const f = 2.5\nfn main() { f := 5\n for i in 0..f { _ = i } }");
}

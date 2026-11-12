//! addr_of_call_projections.rs — regression tests verifying that field and
//! array-index projections on value-returning struct conversions, associated
//! function calls, and indirect function-value calls are rejected under `&`
//! with HYB-E1002.

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
fn rejects_addr_of_struct_conversion_field() {
    let src = "\
struct box {
    v int
}

struct other_box {
    v int
}

fn main() {
    o := other_box{v: 1}
    p := &(box(o).v)
    _ = p
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn rejects_addr_of_associated_fn_and_indirect_call_projections() {
    let src = "\
struct box {
    v int
}

fn box.make() box {
    return box{v: 1}
}

fn mk() box {
    return box{v: 2}
}

fn main() {
    p1 := &(box.make().v)
    fns := []fn() box{mk}
    p2 := &(fns[0]().v)
    _ = p1
    _ = p2
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert_eq!(
        codes.iter().filter(|c| c.as_str() == "HYB-E1002").count(),
        2,
        "expected two HYB-E1002 diagnostics, got {:?}",
        r.diagnostics
    );
}

#[test]
fn accepts_addr_of_slice_returning_associated_fn_index() {
    let src = "\
struct box {
    v int
}

fn box.make_slice() []int {
    return []int{1, 2}
}

fn main() {
    p := &(box.make_slice()[0])
    _ = p
}
";
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "unexpected diagnostics: {:?}",
        r.diagnostics
    );
}

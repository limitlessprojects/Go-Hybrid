//! assign_float_int_soundness.rs — regression tests for variable declarations,
//! struct literals, assignments, and parallel assignments:
//! verifies that assigning or initializing float64 targets with typed int variables
//! is rejected with HYB-E1002, while untyped integer constants remain cleanly accepted (fixes #2674).

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
fn typed_int_var_in_var_decl_rejected_with_e1002() {
    let src = "\
fn main() {
    var i int = 42
    var f float64 = i
    _ = f
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for var decl with typed int, got {:?}",
        r.diagnostics
    );
}

#[test]
fn typed_int_var_in_struct_literal_rejected_with_e1002() {
    let src = "\
struct item {
    f float64
}

fn main() {
    var i int = 42
    s := item{f: i}
    _ = s
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for struct literal with typed int field, got {:?}",
        r.diagnostics
    );
}

#[test]
fn typed_int_var_in_assignment_and_field_assign_rejected_with_e1002() {
    // 1. Single target assignment
    let src1 = "\
fn main() {
    var f float64
    var i int = 42
    f = i
    _ = f
}
";
    let r1 = check(src1);
    assert_eq!(r1.verdict, Verdict::Reject);
    assert!(
        error_codes(&r1).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for assign with typed int, got {:?}",
        r1.diagnostics
    );

    // 2. Struct field assignment
    let src2 = "\
struct item {
    f float64
}

fn main() {
    var s item
    var i int = 42
    s.f = i
    _ = s
}
";
    let r2 = check(src2);
    assert_eq!(r2.verdict, Verdict::Reject);
    assert!(
        error_codes(&r2).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for field assign with typed int, got {:?}",
        r2.diagnostics
    );
}

#[test]
fn typed_int_var_in_parallel_assign_rejected_with_e1002() {
    let src = "\
fn main() {
    var f1 float64
    var f2 float64
    var i int = 42
    f1, f2 = 1.0, i
    _ = f1
    _ = f2
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for parallel assign with typed int, got {:?}",
        r.diagnostics
    );
}

#[test]
fn untyped_int_constants_and_float_assign_accepted() {
    let src = "\
const file_k = 10

struct item {
    f float64
}

fn main() {
    const local_k = 20

    // Constant initializers
    var f1 float64 = 1
    var f2 float64 = local_k
    var f3 float64 = file_k

    // Struct literal with constants
    s := item{f: 42}
    s2 := item{f: local_k}

    // Assignments with constants
    var f float64
    f = 100
    f = local_k
    f = 1 + 2

    // Field assignment with constants
    var it item
    it.f = 200
    it.f = file_k

    // Parallel assignment with constants
    f1, f2 = 1.5, 3

    _ = f1
    _ = f2
    _ = f3
    _ = s
    _ = s2
    _ = f
    _ = it
}
";
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected accept for untyped integer constants assigned to float, got diags: {:?}",
        r.diagnostics
    );
}

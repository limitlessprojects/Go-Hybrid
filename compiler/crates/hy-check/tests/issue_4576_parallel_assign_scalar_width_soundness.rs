//! issue_4576_parallel_assign_scalar_width_soundness.rs — regression for issue #4576.
//!
//! Parallel assignment from a multi-value call (`a, b = pair()`) and multi-value
//! method/closure calls must validate scalar widths (`int32` vs `int64`,
//! `float32` vs `float64`) against destination places and parameter signatures.
//! Previously, `AssignOp::Assign` tested only `kinds_conflict` (both `int32` and `int64`
//! map to `OpKind::Int`), and `call_result_scalars` returned `None` for method and
//! closure callees, allowing mismatched scalar assignments to pass `hyb check` and
//! fail downstream `go build` (Row 13 BUILDABLE, HYB-E1002).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_reject(src: &str) {
    let found = codes(src);
    assert!(
        found.iter().any(|code| code == "HYB-E1002"),
        "expected HYB-E1002, got {found:?} for:\n{src}"
    );
}

fn assert_accept(src: &str) {
    let found = codes(src);
    assert!(
        found.is_empty(),
        "expected acceptance, got {found:?} for:\n{src}"
    );
}

const COMMON_DEFS: &str = r#"fn pair_i32() (int32, int32) {
    return 1, 2
}

fn pair_i64() (int64, int64) {
    return 10, 20
}

fn pair_f32() (float32, float32) {
    return 1.0, 2.0
}

struct worker {}

fn (w worker) multi_i32() (int32, int32) {
    return 1, 2
}

fn (w worker) multi_i64() (int64, int64) {
    return 10, 20
}
"#;

#[test]
fn rejects_parallel_assign_top_level_fn_scalar_width_mismatch() {
    assert_reject(&format!(
        "{COMMON_DEFS}
fn main() {{
    var a int64 = 10
    var b int64 = 20
    a, b = pair_i32()
    _ = a
    _ = b
}}
"
    ));
}

#[test]
fn rejects_parallel_assign_method_scalar_width_mismatch() {
    assert_reject(&format!(
        "{COMMON_DEFS}
fn main() {{
    var w worker
    var a int64 = 10
    var b int64 = 20
    a, b = w.multi_i32()
    _ = a
    _ = b
}}
"
    ));
}

#[test]
fn rejects_parallel_assign_float_width_mismatch() {
    assert_reject(&format!(
        "{COMMON_DEFS}
fn main() {{
    var x float64 = 1.0
    var y float64 = 2.0
    x, y = pair_f32()
    _ = x
    _ = y
}}
"
    ));
}

#[test]
fn accepts_parallel_assign_matching_scalar_widths() {
    assert_accept(&format!(
        "{COMMON_DEFS}
fn main() {{
    var a int64 = 10
    var b int64 = 20
    a, b = pair_i64()
    _ = a
    _ = b
}}
"
    ));
}

#[test]
fn accepts_parallel_assign_method_matching_widths() {
    assert_accept(&format!(
        "{COMMON_DEFS}
fn main() {{
    var w worker
    var a int64 = 10
    var b int64 = 20
    a, b = w.multi_i64()
    _ = a
    _ = b
}}
"
    ));
}

#[test]
fn accepts_parallel_assign_same_type_int32() {
    assert_accept(&format!(
        "{COMMON_DEFS}
fn main() {{
    var a int32 = 1
    var b int32 = 2
    a, b = pair_i32()
    _ = a
    _ = b
}}
"
    ));
}

//! float_int_non_const_soundness.rs — regression tests for non-constant integer
//! expressions assigned or passed to float targets under Row 13 (BUILDABLE)
//! and HYB-E1002 (issue #2506).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_rejects_e1002(src: &str, what: &str) {
    let r = check(src);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for {}, got verdict {:?}, diags {:?}",
        what,
        r.verdict,
        codes
    );
    assert_eq!(r.verdict, Verdict::Reject, "expected rejection for {}", what);
}

fn assert_ok(src: &str, what: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected clean accept for {}, got errors: {:?}",
        what,
        error_codes(&r)
    );
}

#[test]
fn non_const_int_in_var_init_and_assign_rejected() {
    let src_var = "fn main() {\n    var x int = 1\n    var f float64 = x\n    fmt.println(\"{f}\")\n}\n";
    assert_rejects_e1002(src_var, "var f float64 = x");

    let src_assign = "fn main() {\n    var x int = 1\n    var f float64 = 0.0\n    f = x\n    fmt.println(\"{f}\")\n}\n";
    assert_rejects_e1002(src_assign, "f = x");
}

#[test]
fn non_const_int_in_fn_arg_and_return_rejected() {
    let src_arg = "fn take_float(f float64) {}\n\nfn main() {\n    var x int = 1\n    take_float(x)\n}\n";
    assert_rejects_e1002(src_arg, "take_float(x)");

    let src_ret = "fn ret_float() float64 {\n    var x int = 1\n    return x\n}\n\nfn main() {\n    _ = ret_float()\n}\n";
    assert_rejects_e1002(src_ret, "return x (in float64 fn)");
}

#[test]
fn non_const_int_in_struct_slice_map_rejected() {
    let src_struct = "struct s {\n    f float64\n}\n\nfn main() {\n    var x int = 1\n    rec := s{f: x}\n    fmt.println(\"{rec.f}\")\n}\n";
    assert_rejects_e1002(src_struct, "s{f: x}");

    let src_slice = "fn main() {\n    var x int = 1\n    xs := []float64{x}\n    fmt.println(\"{len(xs)}\")\n}\n";
    assert_rejects_e1002(src_slice, "[]float64{x}");

    let src_map_val = "fn main() {\n    var x int = 1\n    m := map[string]float64{\"k\": x}\n    fmt.println(\"{len(m)}\")\n}\n";
    assert_rejects_e1002(src_map_val, "map[string]float64{\"k\": x}");
}

#[test]
fn untyped_constants_to_float_targets_stay_accepted() {
    let src = r#"
fn take_float(f float64) {}

fn ret_float() float64 {
    return 1
}

struct s {
    f float64
}

fn main() {
    var f float64 = 1
    f = 42
    take_float(100)
    _ = ret_float()
    rec := s{f: 2}
    xs := []float64{1, 2, 3.5}
    m := map[string]float64{"a": 1}
    fmt.println("{f} {rec.f} {len(xs)} {len(m)}")
}
"#;
    assert_ok(src, "untyped int constants assigned to float targets");
}

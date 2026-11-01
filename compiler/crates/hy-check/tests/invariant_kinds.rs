//! invariant_kinds.rs — regression tests for #1985: the untyped-int-to-float
//! exemption is legal only at the top of an assignment, never inside a Go type
//! constructor (`[]T`, `[N]T`, `chan T`, `map[K]V`, `*T`) or `copy`.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn body(inner: &str) -> String {
    format!("fn main() {{\n{inner}\n}}\n")
}

fn rejects(src: &str, what: &str) {
    let r = check(src);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for {what}, got {cs:?}"
    );
    assert_eq!(r.verdict, Verdict::Reject, "{what}");
}

#[test]
fn slice_element_type_is_invariant() {
    rejects(
        &body("    var xs []float64 = []int{1}"),
        "[]int assigned to []float64",
    );
}

#[test]
fn array_element_type_is_invariant() {
    rejects(
        &body("    var xs [2]float64 = [2]int{1, 2}"),
        "[2]int assigned to [2]float64",
    );
}

#[test]
fn channel_element_type_is_invariant() {
    rejects(
        &body("    var c chan float64 = make(chan int)"),
        "chan int assigned to chan float64",
    );
}

#[test]
fn map_value_type_is_invariant() {
    rejects(
        &body("    var m map[string]float64 = map[string]int{}"),
        "map[string]int assigned to map[string]float64",
    );
}

#[test]
fn pointer_element_type_is_invariant() {
    rejects(
        &body("    i := 1\n    var p *float64 = &i"),
        "*int assigned to *float64",
    );
}

#[test]
fn pointer_param_auto_address_is_invariant() {
    rejects(
        "fn take(p *float64) {\n}\n\nfn main() {\n    i := 1\n    take(i)\n}\n",
        "an int variable auto-addressed at a *float64 parameter",
    );
}

#[test]
fn copy_element_types_are_invariant() {
    rejects(
        &body("    copy([]float64{}, []int{})"),
        "copy([]float64, []int)",
    );
}

#[test]
fn top_level_untyped_int_to_float_still_accepts() {
    // The exemption this change narrows must survive at the top level:
    // `var f float64 = 1` is legal Go and has its own oracle test.
    let r = check(&body("    var f float64 = 1\n    _ = f"));
    let cs = codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E1002"),
        "`var f float64 = 1` must stay accepted, got {cs:?}"
    );
}

#[test]
fn matching_element_types_still_accept() {
    let r = check(&body(
        "    var xs []float64 = []float64{1, 2}\n    var m map[string]float64 = map[string]float64{}\n    _ = xs\n    _ = m",
    ));
    let cs = codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E1002"),
        "identical element types must stay accepted, got {cs:?}"
    );
}

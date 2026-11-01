//! composite_element_width_2875.rs — issue #2875 (Row 13 BUILDABLE).
//!
//! `OpKind` collapses every integer width into `Int` and both float widths
//! into `Float`, so two COMPOSITE types that differ only in their element type
//! carry identical kinds and the kind checks accepted them:
//!
//! ```hy
//! var a []i64
//! var b []int = a        // Go: cannot use a (variable of type []int64) as []int value
//! ```
//!
//! The same gap covered `map[string]i64` -> `map[string]int`, `*i64` -> `*int`
//! (through `p := &v`), call arguments, `return`, plain assignment and
//! `min`/`max` with mixed typed operands. Go only allows composite assignment
//! between IDENTICAL types, so the declared element scalar decides now, while
//! `byte`/`uint8` and `rune`/`int32` stay convertible (they share a canonical
//! name) and untyped constants still adapt.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
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

fn rejects_with_e1002(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for {:?}, got {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {:?}", src);
}

fn accepts_without_e1002(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E1002"),
        "must NOT fire HYB-E1002 for {:?}, got {:?}",
        src,
        cs
    );
}

// ------------------------------------------------------------ reject (#2875):

#[test]
fn composite_declaration_with_a_different_element_width_rejects_e1002() {
    rejects_with_e1002(
        "fn main() {\n    var a []i64\n    var b []int = a\n    fmt.println(\"{b}\")\n}",
    );
    rejects_with_e1002(
        "fn main() {\n    var a map[string]i64\n    var b map[string]int = a\n    fmt.println(\"{b}\")\n}",
    );
    rejects_with_e1002(
        "fn main() {\n    var v i64 = 1\n    p := &v\n    var q *int = p\n    fmt.println(\"{q}\")\n}",
    );
}

#[test]
fn call_argument_with_a_different_element_width_rejects_e1002() {
    rejects_with_e1002(
        "fn take(xs []int) int {\n    return len(xs)\n}\n\nfn main() {\n    var a []i64\n    fmt.println(take(a))\n}",
    );
}

#[test]
fn return_with_a_different_element_width_rejects_e1002() {
    rejects_with_e1002(
        "fn mk() []int {\n    var a []i64\n    return a\n}\n\nfn main() {\n    fmt.println(mk())\n}",
    );
}

#[test]
fn assignment_with_a_different_element_width_rejects_e1002() {
    rejects_with_e1002(
        "fn main() {\n    var a []i64\n    var c []int\n    c = a\n    fmt.println(\"{c}\")\n}",
    );
}

#[test]
fn min_max_with_mixed_typed_widths_rejects_e1002() {
    rejects_with_e1002(
        "fn main() {\n    var a i64 = 1\n    var b int = 2\n    fmt.println(min(a, b))\n}",
    );
    rejects_with_e1002(
        "fn main() {\n    var a f32 = 1.0\n    var b f64 = 2.0\n    fmt.println(max(a, b))\n}",
    );
}

// ------------------------------------------------------------ accept:

#[test]
fn identical_composite_element_types_stay_accepted() {
    accepts_without_e1002(
        "fn main() {\n    var a []i64\n    var b []i64 = a\n    fmt.println(\"{b}\")\n}",
    );
    accepts_without_e1002(
        "fn main() {\n    var a map[string]i64\n    var b map[string]i64 = a\n    fmt.println(\"{b}\")\n}",
    );
    accepts_without_e1002(
        "fn main() {\n    var v i64 = 1\n    p := &v\n    var q *i64 = p\n    fmt.println(\"{q}\")\n}",
    );
    accepts_without_e1002(
        "fn main() {\n    var a [2]i64\n    var b [2]i64 = a\n    fmt.println(\"{b}\")\n}",
    );
}

#[test]
fn aliased_elements_and_untyped_constants_stay_accepted() {
    // `byte` IS `uint8`, so the two slices have identical element types.
    accepts_without_e1002(
        "fn main() {\n    var a []byte\n    var b []uint8 = a\n    fmt.println(\"{b}\")\n}",
    );
    // An untyped constant adapts to the target's element type.
    accepts_without_e1002(
        "fn main() {\n    var c []i64\n    c = []i64{1, 2}\n    fmt.println(\"{c}\")\n}",
    );
    // Mixed widths with an untyped constant operand are legal Go.
    accepts_without_e1002(
        "fn main() {\n    var a i64 = 1\n    fmt.println(min(a, 2))\n}",
    );
}

//! variadic_append_elem_2884.rs — issue #2884 (Row 13 BUILDABLE).
//!
//! `append(dst, src...)` must pass a slice whose element type is IDENTICAL to
//! `dst`'s (or a string into `[]byte`). The spread branch reads the two element
//! types from their DECLARED binding text, so a value bound to a CALL
//! (`src := mk64()`) had no text to compare: the kind model collapses every
//! integer width into `Int`, and `append(dst_int32, src_int64...)` was accepted
//! while `go build` refused it
//! (`cannot use src (variable of type []int64) as []int32 value in argument to append`).
//!
//! The callee's single slice result type is now recorded for such a binding, so
//! the element width survives and the spread comparison sees it. `byte`/`uint8`
//! and `rune`/`int32` share a canonical name and stay interchangeable, and
//! `string` is still appendable to `[]byte`.

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

const MK: &str = "fn mk32() []int32 {\n    return []int32{1}\n}\n\nfn mk64() []int64 {\n    return []int64{1}\n}\n\n";

// ------------------------------------------------------------ reject (#2884):

#[test]
fn spread_with_a_different_element_family_rejects_e1002() {
    // The issue's first reproducer.
    rejects_with_e1002(
        "fn main() {\n    var dst []int32\n    var src []string\n    dst = append(dst, src...)\n    fmt.println(\"{dst}\")\n}",
    );
}

#[test]
fn spread_with_a_different_element_width_rejects_e1002() {
    // The issue's second reproducer, and the other width pairs.
    rejects_with_e1002(
        "fn main() {\n    var dst []int32\n    var src []int64\n    dst = append(dst, src...)\n    fmt.println(\"{dst}\")\n}",
    );
    rejects_with_e1002(
        "fn main() {\n    var dst []float32\n    var src []float64\n    dst = append(dst, src...)\n    fmt.println(\"{dst}\")\n}",
    );
    rejects_with_e1002(
        "fn main() {\n    var dst []int32\n    var src []uint32\n    dst = append(dst, src...)\n    fmt.println(\"{dst}\")\n}",
    );
    rejects_with_e1002(
        "fn main() {\n    var dst []byte\n    var src []int32\n    dst = append(dst, src...)\n    fmt.println(\"{dst}\")\n}",
    );
}

#[test]
fn spread_of_a_call_result_with_a_different_element_width_rejects_e1002() {
    // `src := mk64()` — no declared element text on the binding itself.
    rejects_with_e1002(&format!(
        "{MK}fn main() {{\n    var dst []int32\n    src := mk64()\n    dst = append(dst, src...)\n    fmt.println(\"{{dst}}\")\n}}"
    ));
    rejects_with_e1002(&format!(
        "{MK}fn main() {{\n    dst := mk32()\n    var src []int64\n    dst = append(dst, src...)\n    fmt.println(\"{{dst}}\")\n}}"
    ));
}

// ------------------------------------------------------------ accept:

#[test]
fn identical_element_types_stay_accepted() {
    accepts_without_e1002(
        "fn main() {\n    var dst []int64\n    var src []int64\n    dst = append(dst, src...)\n    fmt.println(\"{dst}\")\n}",
    );
    accepts_without_e1002(&format!(
        "{MK}fn main() {{\n    var dst []int32\n    src := mk32()\n    dst = append(dst, src...)\n    fmt.println(\"{{dst}}\")\n}}"
    ));
}

#[test]
fn aliased_elements_and_string_into_bytes_stay_accepted() {
    // `byte` IS `uint8` and `rune` IS `int32`.
    accepts_without_e1002(
        "fn main() {\n    var dst []byte\n    var src []uint8\n    dst = append(dst, src...)\n    fmt.println(\"{dst}\")\n}",
    );
    accepts_without_e1002(
        "fn main() {\n    var dst []int32\n    var src []rune\n    dst = append(dst, src...)\n    fmt.println(\"{dst}\")\n}",
    );
    // A string spread is legal for `[]byte`.
    accepts_without_e1002(
        "fn main() {\n    var dst []byte\n    dst = append(dst, \"abc\"...)\n    fmt.println(\"{dst}\")\n}",
    );
}

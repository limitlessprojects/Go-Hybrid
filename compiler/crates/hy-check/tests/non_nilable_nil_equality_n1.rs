//! non_nilable_nil_equality_n1.rs — regression tests for SPEC §3.1 Rule N1 and Row 13 BUILDABLE.
//!
//! Non-nilable types (`int`, `float64`, `bool`, `string`, `struct`, `array`)
//! cannot be compared to untyped `nil`. Previously, `buildable.rs` skipped kind
//! checking when either side was a `nil` literal, allowing invalid comparisons like
//! `x == nil` (where `x` is `int`) to pass `hyb check` with 0 diagnostics and fail `go build`.

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

fn assert_reject_e1002(src: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "expected reject, got accept for:\n{src}"
    );
    let codes = error_codes(&r);
    assert!(
        codes.contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002, got {codes:?} for:\n{src}"
    );
}

fn assert_accept(src: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected accept, got diagnostics: {:?}",
        r.diagnostics
    );
}

#[test]
fn rejects_int_nil_equality() {
    assert_reject_e1002("fn main() {\n    x := 5\n    if x == nil {}\n}\n");
    assert_reject_e1002("fn main() {\n    x := 5\n    if x != nil {}\n}\n");
    assert_reject_e1002("fn main() {\n    x := 5\n    if nil == x {}\n}\n");
    assert_reject_e1002("fn main() {\n    if 100 == nil {}\n}\n");
}

#[test]
fn rejects_string_nil_equality() {
    assert_reject_e1002("fn main() {\n    s := \"hello\"\n    if s == nil {}\n}\n");
    assert_reject_e1002("fn main() {\n    s := \"hello\"\n    if s != nil {}\n}\n");
    assert_reject_e1002("fn main() {\n    if \"literal\" == nil {}\n}\n");
}

#[test]
fn rejects_bool_nil_equality() {
    assert_reject_e1002("fn main() {\n    b := true\n    if b == nil {}\n}\n");
    assert_reject_e1002("fn main() {\n    if true != nil {}\n}\n");
}

#[test]
fn rejects_float_nil_equality() {
    assert_reject_e1002("fn main() {\n    f := 3.14\n    if f == nil {}\n}\n");
    assert_reject_e1002("fn main() {\n    if 2.5 != nil {}\n}\n");
}

#[test]
fn rejects_struct_nil_equality() {
    let src = "struct point {\n    x int\n    y int\n}\nfn main() {\n    p := point{x: 1, y: 2}\n    if p == nil {}\n}\n";
    assert_reject_e1002(src);
}

#[test]
fn rejects_array_nil_equality() {
    assert_reject_e1002("fn main() {\n    arr := [2]int{1, 2}\n    if arr == nil {}\n}\n");
    assert_reject_e1002("fn main() {\n    arr := [2]int{1, 2}\n    if nil != arr {}\n}\n");
}

#[test]
fn rejects_nil_nil_equality() {
    assert_reject_e1002("fn main() {\n    if nil == nil {}\n}\n");
}

#[test]
fn accepts_nilable_types_with_nil() {
    // Pointer
    assert_accept("fn main() {\n    x := 5\n    p := &x\n    if p == nil {}\n    if nil != p {}\n}\n");
    // Slice
    assert_accept("fn main() {\n    s := []int{1, 2}\n    if s == nil {}\n    if nil != s {}\n}\n");
    // Map
    assert_accept("fn main() {\n    m := map[string]int{}\n    if m == nil {}\n    if nil != m {}\n}\n");
    // Channel
    assert_accept("fn main() {\n    ch := make(chan int, 1)\n    if ch == nil {}\n    if nil != ch {}\n}\n");
}

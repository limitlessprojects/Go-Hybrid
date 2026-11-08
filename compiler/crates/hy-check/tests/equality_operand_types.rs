//! Regression test: equality (`==`, `!=`) operand type checking.
//!
//! Go disallows comparing mismatched types (e.g. `int == string`, `int == bool`).
//! Previously, `buildable.rs` left `BinOp::Eq | BinOp::Ne => {}` unhandled under
//! the assumption that `emit_guard.rs` (HYB-E2006) checked them. However,
//! HYB-E2006 only checks incomparable shapes (slice, map, func). Mismatched
//! scalar and pointer types passed `hyb check` with 0 diagnostics and failed `go build`.
//!
//! Rule: `BUILDABLE` (GUARANTEES.md Row 13). Diagnostic: `HYB-E1002`.

use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("test.hy", src).expect("parses");
    check_file(&ast)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn assert_e1002(src: &str) {
    let cs = codes(src);
    assert!(
        cs.contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 in {cs:?} for:\n{src}"
    );
}

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(
        cs.is_empty(),
        "expected 0 diagnostics, got {cs:?} for:\n{src}"
    );
}

#[test]
fn rejects_mismatched_equality_operands() {
    assert_e1002("fn main() {\n    x := 5\n    s := \"a\"\n    if x == s {}\n}\n");
    assert_e1002("fn main() {\n    x := 5\n    s := \"a\"\n    if x != s {}\n}\n");
    assert_e1002("fn main() {\n    x := 5\n    b := true\n    if x == b {}\n}\n");
    assert_e1002("fn main() {\n    s := \"a\"\n    b := true\n    if s == b {}\n}\n");
    assert_e1002("fn main() {\n    if 1 == \"hello\" {}\n}\n");
    assert_e1002("fn main() {\n    if true == 0 {}\n}\n");
    assert_e1002("fn main() {\n    x := 5\n    p := &x\n    if p == x {}\n}\n");
}

#[test]
fn accepts_matching_and_nil_equality() {
    assert_ok("fn main() {\n    x := 5\n    y := 10\n    if x == y {}\n}\n");
    assert_ok("fn main() {\n    s1 := \"a\"\n    s2 := \"b\"\n    if s1 == s2 {}\n}\n");
    assert_ok("fn main() {\n    b1 := true\n    b2 := false\n    if b1 == b2 {}\n}\n");
    assert_ok("fn main() {\n    x := 5\n    p := &x\n    if p == nil {}\n}\n");
    assert_ok("fn main() {\n    x := 5\n    p := &x\n    if nil == p {}\n}\n");
    assert_ok("fn main() {\n    if 1 == 1.0 {}\n}\n");
    assert_ok("fn main() {\n    if 1.0 == 1 {}\n}\n");
    assert_ok("fn main() {\n    f := 1.5\n    if f == 1 {}\n}\n");
}

#[test]
fn rejects_non_nilable_types_compared_to_nil_issue_2471() {
    // int
    assert_e1002("fn main() {\n    x := 5\n    if x == nil {}\n}\n");
    assert_e1002("fn main() {\n    x := 5\n    if nil == x {}\n}\n");
    assert_e1002("fn main() {\n    if 100 == nil {}\n}\n");
    assert_e1002("fn main() {\n    if nil != 100 {}\n}\n");

    // string
    assert_e1002("fn main() {\n    s := \"hello\"\n    if s != nil {}\n}\n");
    assert_e1002("fn main() {\n    if \"raw\" == nil {}\n}\n");

    // bool
    assert_e1002("fn main() {\n    b := true\n    if b == nil {}\n}\n");

    // float
    assert_e1002("fn main() {\n    f := 3.14\n    if f != nil {}\n}\n");

    // struct
    assert_e1002("struct point {\n    x int\n    y int\n}\nfn main() {\n    pt := point{x: 1, y: 2}\n    if pt == nil {}\n}\n");

    // array
    assert_e1002("fn main() {\n    arr := [2]int{1, 2}\n    if arr != nil {}\n}\n");

    // nil == nil
    assert_e1002("fn main() {\n    if nil == nil {}\n}\n");
}

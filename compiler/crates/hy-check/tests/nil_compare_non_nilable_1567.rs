//! nil_compare_non_nilable_1567.rs — `== nil` / `!= nil` against an operand
//! that has no nil value in Go (issue #1567).
//!
//! `check_equality` used to return early on ANY nil literal, on the comment
//! that "E4001/N1 own its validity". E4001/N1 own nil validity at
//! declarations, assignments, returns and spawn arguments — none of those
//! sites see a comparison — so `x == nil` for a primitive/string/struct/array
//! passed `hyb check` and then failed `go build` with
//! `invalid operation: x == nil (mismatched types int and untyped nil)`
//! (GUARANTEES row 13, BUILDABLE).
//!
//! The fix resolves the NON-nil operand through its DECLARED type (`var a any
//! = 1` is an interface value even though its initializer's shape is
//! primitive) and rejects only shapes that provably have no nil:
//! primitive, string, struct, array. Pointer / chan / func / map / slice /
//! interface / unknown shapes stay accepted — fail-open, same policy as
//! `incomparable_noun`.

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

fn assert_rejects(src: &str, code: &str, what: &str) {
    let r = check(src);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == code),
        "expected {code} for {what}, got {cs:?}"
    );
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {what}");
}

fn assert_accepts(src: &str, what: &str) {
    let r = check(src);
    let cs = codes(&r);
    assert!(cs.is_empty(), "expected ACCEPT for {what}, got {cs:?}");
    assert_eq!(r.verdict, Verdict::Accept, "verdict for {what}");
}

// ---------------------------------------------------------------------------
// The reject shapes: value types with no nil.
// ---------------------------------------------------------------------------

#[test]
fn int_compare_nil_rejects_e4001() {
    assert_rejects(
        "fn main() {\n    x := 1\n    if x == nil {\n        fmt.println(\"nil\")\n    }\n}\n",
        "HYB-E4001",
        "`x == nil` with x int",
    );
}

#[test]
fn string_compare_nil_rejects_e4001() {
    assert_rejects(
        "fn main() {\n    s := \"a\"\n    if s == nil {\n        fmt.println(\"nil\")\n    }\n}\n",
        "HYB-E4001",
        "`s == nil` with s string",
    );
}

#[test]
fn bool_not_nil_rejects_e4001() {
    assert_rejects(
        "fn main() {\n    b := true\n    if b != nil {\n        fmt.println(\"b\")\n    }\n}\n",
        "HYB-E4001",
        "`b != nil` with b bool",
    );
}

#[test]
fn struct_compare_nil_rejects_e4001() {
    assert_rejects(
        "struct s {\n    a int\n}\n\nfn main() {\n    var v s\n    if v == nil {\n        fmt.println(\"nil\")\n    }\n}\n",
        "HYB-E4001",
        "`v == nil` with v a struct",
    );
}

#[test]
fn array_compare_nil_rejects_e4001() {
    assert_rejects(
        "fn main() {\n    var a [2]int\n    if a == nil {\n        fmt.println(\"nil\")\n    }\n}\n",
        "HYB-E4001",
        "`a == nil` with a an array",
    );
}

#[test]
fn reversed_nil_operand_rejects_e4001() {
    assert_rejects(
        "fn main() {\n    x := 5\n    if nil == x {\n        fmt.println(\"nil\")\n    }\n}\n",
        "HYB-E4001",
        "`nil == x` with x int",
    );
}

#[test]
fn primitive_param_compare_nil_rejects_e4001() {
    assert_rejects(
        "fn f(n int) {\n    if n == nil {\n        fmt.println(\"nil\")\n    }\n}\n\nfn main() {\n    f(1)\n}\n",
        "HYB-E4001",
        "`n == nil` on an int parameter",
    );
}

#[test]
fn nil_eq_nil_rejects_e4003() {
    assert_rejects(
        "fn main() {\n    if nil == nil {\n        fmt.println(\"both\")\n    }\n}\n",
        "HYB-E4003",
        "`nil == nil`",
    );
}

// ---------------------------------------------------------------------------
// Controls: nilable shapes, and interfaces whose initializer shape lies.
// ---------------------------------------------------------------------------

#[test]
fn slice_map_chan_func_pointer_nil_accept() {
    let src = r#"fn main() {
    var xs []int
    if xs == nil {
        fmt.println("slice")
    }
    var m map[string]int
    if m == nil {
        fmt.println("map")
    }
    var ch chan int
    if ch == nil {
        fmt.println("chan")
    }
    f := fn() {}
    if f == nil {
        fmt.println("func")
    }
    var p *int
    if p == nil {
        fmt.println("ptr")
    }
}
"#;
    assert_accepts(src, "nil comparisons on slice/map/chan/func/pointer");
}

#[test]
fn any_and_error_nil_accept() {
    let src = r#"fn main() {
    var a any = 1
    if a == nil {
        fmt.println("any")
    }
    var e error = nil
    if e == nil {
        fmt.println("error")
    }
}
"#;
    assert_accepts(src, "nil comparisons on any and error");
}

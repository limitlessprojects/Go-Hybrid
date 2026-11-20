//! Regression tests: the #2474 element-type check on `set[T]` methods
//! (`insert` / `contains` / `remove`, and `delete(s, v)`) must also see sets
//! reached through a struct field (`x.seen`, `p.seen`, generic `b.v`), the
//! builtin `new(set[T])`, and an address-of binding (`p := &s`).
//!
//! Before, `set_elem_of_expr` resolved only plain set bindings, set literals
//! and user fn results, so `x.seen.insert("a")` on a `set[int]` field passed
//! `hyb check` with 0 diagnostics and failed `go build` ("cannot use "a"
//! (untyped string constant) as int value in argument to x.seen.insert")
//! (Row 13 BUILDABLE: reject fail-closed, HYB-E1002 / HYB-E2005).
//! Valid calls through the same receivers must keep passing.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
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

fn rejects(src: &str, code: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == code),
        "expected {} for {:?}, got {:?}",
        code,
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {:?}", src);
}

fn accepts(src: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "verdict for {:?}: {:?}", src, r.diagnostics);
}

#[test]
fn struct_field_set_insert_wrong_element_rejects() {
    rejects(
        r#"
struct g { seen set[int] }
fn main() {
    x := g{seen: set[int]{}}
    x.seen.insert("a")
    fmt.println(x)
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn pointer_struct_field_set_contains_wrong_element_rejects() {
    rejects(
        r#"
struct g { seen set[int] }
fn main() {
    x := &g{seen: set[int]{}}
    fmt.println(x.seen.contains("a"))
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn generic_struct_field_set_insert_wrong_element_rejects() {
    rejects(
        r#"
struct box[t any] { v t }
fn main() {
    b := box[set[int]]{v: set[int]{}}
    fmt.println(b.v.insert("a"))
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn struct_field_set_delete_wrong_element_rejects() {
    rejects(
        r#"
struct g { seen set[int] }
fn main() {
    x := g{seen: set[int]{}}
    delete(x.seen, "a")
}
"#,
        "HYB-E2005",
    );
}

#[test]
fn new_set_contains_wrong_element_rejects() {
    rejects(
        r#"
fn main() {
    s := new(set[int])
    fmt.println(s.contains("a"))
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn addr_of_set_insert_wrong_element_rejects() {
    rejects(
        r#"
fn main() {
    s := set[int]{}
    p := &s
    p.insert("a")
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn pointer_to_set_field_insert_wrong_element_rejects() {
    rejects(
        r#"
struct h { s *set[int] }
fn main() {
    x := h{s: new(set[int])}
    x.s.insert("a")
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn valid_calls_through_fields_new_and_pointers_accept() {
    accepts(
        r#"
struct g { seen set[int] }
fn main() {
    x := g{seen: set[int]{}}
    fmt.println(x.seen.insert(1), x.seen.contains(1))
    delete(x.seen, 1)
    p := &x
    fmt.println(p.seen.insert(2))
    s := new(set[string])
    fmt.println(s.insert("a"), s.contains("a"))
    t := set[int]{}
    q := &t
    fmt.println(q.insert(3), q.contains(3))
}
"#,
    );
}

#[test]
fn valid_generic_and_float_field_calls_accept() {
    accepts(
        r#"
struct box[t any] { v t }
struct f { seen set[float64] }
fn main() {
    b := box[set[int]]{v: set[int]{}}
    fmt.println(b.v.insert(1))
    x := f{seen: set[float64]{}}
    fmt.println(x.seen.insert(1))
    x.seen.remove(2)
}
"#,
    );
}

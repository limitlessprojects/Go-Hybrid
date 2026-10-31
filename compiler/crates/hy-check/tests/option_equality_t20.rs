//! Regression test: SPEC §2.5 T20 option equality guards (HYB-E2006).
//!
//! SPEC §2.5 T20:
//! "Constructors some(v) / none; o.unwrap() (panics with HY-PANIC/unwrap-none if none);
//! o.or(default) returns the payload or the default; o.is_some() bool.
//! No == on option in MVP (T16's rule applied to the builtin)."
//!
//! Rule: BUILDABLE (GUARANTEES.md Row 13). Diagnostic: HYB-E2006.

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

fn assert_e2006(src: &str) {
    let cs = codes(src);
    assert!(
        cs.contains(&"HYB-E2006".to_string()),
        "expected HYB-E2006 in {cs:?} for:\n{src}"
    );
}

#[test]
fn rejects_option_equality_scalar() {
    let src = "
fn main() {
    var o1 option[int]
    var o2 option[int]
    if o1 == o2 {
        fmt.println(\"eq\")
    }
}
";
    assert_e2006(src);
}

#[test]
fn rejects_option_inequality_scalar() {
    let src = "
fn main() {
    var o1 option[int]
    var o2 option[int]
    if o1 != o2 {
        fmt.println(\"neq\")
    }
}
";
    assert_e2006(src);
}

#[test]
fn rejects_generic_option_equality() {
    let src = "
fn cmp[t](a option[t], b option[t]) bool {
    return a == b
}
";
    assert_e2006(src);
}

#[test]
fn rejects_some_constructor_equality() {
    let src = "
fn main() {
    a := some(1)
    b := some(2)
    if a == b {
        fmt.println(\"eq\")
    }
}
";
    assert_e2006(src);
}

#[test]
fn rejects_none_equality() {
    let src = "
fn main() {
    var o option[int]
    if o == none {
        fmt.println(\"none\")
    }
}
";
    assert_e2006(src);
}

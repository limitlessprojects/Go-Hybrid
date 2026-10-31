//! Regression test: SPEC §2.5 T16 enum equality guards (HYB-E2006).
//!
//! SPEC §2.5 T16:
//! "== / != are defined only for unit-only enums (all variants payload-free).
//! Data-carrying enums compare via match.
//! Why: payload equality is deep-equality policy (reference vs structural)
//! that MVP does not need and must not guess."
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

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(
        cs.is_empty(),
        "expected 0 diagnostics, got {cs:?} for:\n{src}"
    );
}

#[test]
fn data_carrying_enum_equality_is_rejected() {
    let src = "
enum shape {
    circle(float64),
    square(float64),
}

fn main() {
    var a shape
    var b shape
    if a == b {
        fmt.println(\"eq\")
    }
}
";
    assert_e2006(src);
}

#[test]
fn data_carrying_enum_inequality_is_rejected() {
    let src = "
enum shape {
    circle(float64),
    square(float64),
}

fn main() {
    var a shape
    var b shape
    if a != b {
        fmt.println(\"neq\")
    }
}
";
    assert_e2006(src);
}

#[test]
fn unit_only_enum_equality_stays_accepted() {
    let src = "
enum color {
    red,
    green,
    blue,
}

fn main() {
    var c1 color
    var c2 color
    if c1 == c2 {
        fmt.println(\"eq\")
    }
    if c1 != c2 {
        fmt.println(\"neq\")
    }
}
";
    assert_ok(src);
}

#[test]
fn struct_with_data_carrying_enum_field_equality_is_rejected() {
    let src = "
enum shape {
    circle(float64),
    square(float64),
}

struct item {
    s shape
}

fn main() {
    var i1 item
    var i2 item
    if i1 == i2 {
        fmt.println(\"eq\")
    }
}
";
    assert_e2006(src);
}

#[test]
fn struct_with_unit_only_enum_field_equality_stays_accepted() {
    let src = "
enum color {
    red,
    green,
    blue,
}

struct badge {
    c color
}

fn main() {
    var b1 badge
    var b2 badge
    if b1 == b2 {
        fmt.println(\"eq\")
    }
}
";
    assert_ok(src);
}

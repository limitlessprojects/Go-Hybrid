//! #778 — reject type names used where a value is required.
//!
//! A bare type name (struct, generic struct, enum, type alias, primitive type,
//! or function type parameter) used in an expression/value position (assignment RHS,
//! call argument, binary arithmetic operand, return value, or assignment target)
//! previously passed `hyb check` with 0 diagnostics. When emitted to Go, `go build`
//! rejected with `<T> (type) is not an expression`, violating GUARANTEES Row 13 (BUILDABLE).
//!
//! Controls ensure valid positions remain accepted:
//! - Type conversions (`int(3.0)`, `u8(200)`, `holder(1)`)
//! - Allocation first arguments (`new(holder)`, `make(chan int, 1)`, `make([]int, 4)`)
//! - Qualified member access (`color.red` enum variants, `greeting.build(...)` method expressions)
//! - Shadowed bindings where a local variable or parameter shares the type's name.

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

fn assert_has_code(src: &str, want: &str) {
    let cs = codes(src);
    assert!(
        cs.contains(&want.to_string()),
        "expected diagnostic {want}, got {cs:?} for:\n{src}"
    );
}

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(cs.is_empty(), "expected 0 diagnostics, got {cs:?} for:\n{src}");
}

// ---------------------------------------------------------------------------
// Issue #778 reproducers: Four core value positions
// ---------------------------------------------------------------------------

#[test]
fn reject_type_name_in_assignment_rhs() {
    let src = r#"
struct holder {
    n int
}

fn main() {
    x := holder
    fmt.println("{x}")
}
"#;
    assert_has_code(src, "HYB-E1002");
}

#[test]
fn reject_type_name_in_call_argument() {
    let src = r#"
struct holder {
    n int
}

fn take(h holder) int {
    return h.n
}

fn main() {
    v := take(holder)
    fmt.println("{v}")
}
"#;
    assert_has_code(src, "HYB-E1002");
}

#[test]
fn reject_type_name_in_arithmetic_operand() {
    let src = r#"
struct holder {
    n int
}

fn main() {
    y := holder + 1
    fmt.println("{y}")
}
"#;
    assert_has_code(src, "HYB-E1002");
}

#[test]
fn reject_type_name_in_return_statement() {
    let src = r#"
struct holder {
    n int
}

fn make_it() holder {
    return holder
}

fn main() {
    h := make_it()
    fmt.println("{h.n}")
}
"#;
    assert_has_code(src, "HYB-E1002");
}

// ---------------------------------------------------------------------------
// Assignment LHS (target place)
// ---------------------------------------------------------------------------

#[test]
fn reject_type_name_in_assignment_target() {
    let src = r#"
struct holder {
    n int
}

fn main() {
    holder = 5
}
"#;
    assert_has_code(src, "HYB-E1002");
}

#[test]
fn reject_type_name_in_compound_assignment_target() {
    let src = r#"
struct holder {
    n int
}

fn main() {
    holder += 1
}
"#;
    assert_has_code(src, "HYB-E1002");
}

// ---------------------------------------------------------------------------
// Built-in types, type aliases, and enums in value positions
// ---------------------------------------------------------------------------

#[test]
fn reject_primitive_type_as_value() {
    let src = r#"
fn main() {
    x := int
    fmt.println("{x}")
}
"#;
    assert_has_code(src, "HYB-E1002");
}

#[test]
fn reject_string_primitive_as_value() {
    let src = r#"
fn main() {
    s := string
    fmt.println("{s}")
}
"#;
    assert_has_code(src, "HYB-E1002");
}

#[test]
fn reject_type_alias_as_value() {
    let src = r#"
type tally = map[string]int

fn main() {
    t := tally
    fmt.println("{t}")
}
"#;
    assert_has_code(src, "HYB-E1002");
}

#[test]
fn reject_enum_type_as_value() {
    let src = r#"
enum color {
    red,
    green,
}

fn main() {
    c := color
    fmt.println("{c}")
}
"#;
    assert_has_code(src, "HYB-E1002");
}

#[test]
fn reject_generic_type_parameter_as_value() {
    let src = r#"
fn f[t any](x t) int {
    y := t
    fmt.println("{y}")
    return 0
}

fn main() {
    f[int](1)
}
"#;
    assert_has_code(src, "HYB-E1002");
}

// ---------------------------------------------------------------------------
// Composite contexts: index, channel send, struct literal, slice literal
// ---------------------------------------------------------------------------

#[test]
fn reject_type_name_in_channel_send() {
    let src = r#"
struct holder {
    n int
}

fn main() {
    ch := make(chan int, 1)
    ch <- holder
}
"#;
    assert_has_code(src, "HYB-E1002");
}

#[test]
fn reject_type_name_in_slice_index() {
    let src = r#"
struct holder {
    n int
}

fn main() {
    xs := [1, 2, 3]
    v := xs[holder]
    fmt.println("{v}")
}
"#;
    assert_has_code(src, "HYB-E1002");
}

#[test]
fn reject_type_name_in_struct_field_value() {
    let src = r#"
struct holder {
    n int
}

struct wrapper {
    val int
}

fn main() {
    w := wrapper{val: holder}
    fmt.println("{w.val}")
}
"#;
    assert_has_code(src, "HYB-E1002");
}

// ---------------------------------------------------------------------------
// Controls: valid positions that MUST remain accepted
// ---------------------------------------------------------------------------

#[test]
fn accept_type_conversions() {
    let src = r#"
struct holder {
    n int
}

fn main() {
    i := int(3.0)
    b := u8(200)
    h := holder(1)
    fmt.println("{i} {b} {h.n}")
}
"#;
    assert_ok(src);
}

#[test]
fn accept_new_and_make_type_arguments() {
    let src = r#"
struct holder {
    n int
}

type tally = map[string]int

fn main() {
    p := new(holder)
    p.n = 7
    c := make(chan int, 1)
    c <- 1
    s := make([]int, 4)
    t := make(tally)
    fmt.println("{p.n} {<-c} {len(s)} {len(t)}")
}
"#;
    assert_ok(src);
}

#[test]
fn accept_enum_variant_qualifier() {
    let src = r#"
enum color {
    red,
    green,
}

fn main() {
    c := color.red
    fmt.println("{c}")
}
"#;
    assert_ok(src);
}

#[test]
fn accept_shadowed_local_variable() {
    let src = r#"
struct holder {
    n int
}

fn main() {
    holder := 42
    x := holder
    fmt.println("{x}")
}
"#;
    assert_ok(src);
}

#[test]
fn accept_shadowed_parameter() {
    let src = r#"
struct holder {
    n int
}

fn take(holder int) int {
    return holder
}

fn main() {
    v := take(99)
    fmt.println("{v}")
}
"#;
    assert_ok(src);
}

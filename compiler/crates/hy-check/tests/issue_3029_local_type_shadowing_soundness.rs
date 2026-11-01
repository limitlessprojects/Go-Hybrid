//! #3029 — [buildable] local value named like a type shadows type uses in same scope.
//!
//! A local value binding (`:=`, parameter, or `for` binding) whose name is a
//! predeclared type (`int`, `string`, `bool`, `byte`, `any`) or a user struct
//! name is accepted by `hyb check`. If the same scope then uses that name as a
//! type (var declaration, composite literal, conversion, `[]T` / `map[K]V`),
//! Go resolves it to the local variable and `go build` fails with:
//! `'X (local variable) is not a type'`.
//!
//! That breaks GUARANTEES Row 13 (BUILDABLE).

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
        cs.iter().any(|c| c == want),
        "expected diagnostic [{want}], got {cs:?} for:\n{src}"
    );
}

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(cs.is_empty(), "expected 0 diagnostics, got {cs:?} for:\n{src}");
}

// ---------------------------------------------------------------------------
// 9 Repro cases from #3029
// ---------------------------------------------------------------------------

#[test]
fn test_case_1_local_var_int_shadows_var_decl_type() {
    let src = r#"
fn main() {
    int := 1
    var y int = 2
    fmt.println(int, y)
}
"#;
    assert_has_code(src, "HYB-E1002");
}

#[test]
fn test_case_2_local_var_string_shadows_var_decl_type() {
    let src = r#"
fn main() {
    string := "a"
    var t string = "b"
    fmt.println(string, t)
}
"#;
    assert_has_code(src, "HYB-E1002");
}

#[test]
fn test_case_3_local_var_any_shadows_var_decl_type() {
    let src = r#"
fn main() {
    any := 1
    var x any = 2
    fmt.println(any, x)
}
"#;
    assert_has_code(src, "HYB-E1002");
}

#[test]
fn test_case_4_param_int_shadows_var_decl_type() {
    let src = r#"
fn f(int int) int {
    var y int = int
    return y
}
"#;
    assert_has_code(src, "HYB-E1002");
}

#[test]
fn test_case_5_loop_binding_string_shadows_var_decl_type() {
    let src = r#"
fn main() {
    xs := ["hello"]
    for string in xs {
        var t string = "a"
        fmt.println(t)
    }
}
"#;
    assert_has_code(src, "HYB-E1002");
}

#[test]
fn test_case_6_local_var_bool_shadows_slice_lit_type() {
    let src = r#"
fn main() {
    bool := 1
    xs := []bool{true}
    fmt.println(bool, xs)
}
"#;
    assert_has_code(src, "HYB-E1002");
}

#[test]
fn test_case_7_local_var_int_shadows_map_lit_val_type() {
    let src = r#"
fn main() {
    int := 1
    m := map[string]int{"a": 2}
    fmt.println(int, m)
}
"#;
    assert_has_code(src, "HYB-E1002");
}

#[test]
fn test_case_8_local_var_byte_shadows_slice_type() {
    let src = r#"
fn main() {
    byte := 1
    var bs []byte
    fmt.println(byte, bs)
}
"#;
    assert_has_code(src, "HYB-E1002");
}

#[test]
fn test_case_9_local_var_struct_shadows_struct_lit_type() {
    let src = r#"
struct p {
    x int
}

fn main() {
    p := p{x: 1}
    q := p{x: 2}
    fmt.println(p.x, q.x)
}
"#;
    assert_has_code(src, "HYB-E1002");
}

// ---------------------------------------------------------------------------
// Controls (must remain accepted)
// ---------------------------------------------------------------------------

#[test]
fn test_control_shadow_alone_is_accepted() {
    let src = r#"
fn main() {
    int := 1
    fmt.println(int)
}
"#;
    assert_ok(src);
}

#[test]
fn test_control_shadow_in_inner_block_before_outer_type_is_accepted() {
    let src = r#"
fn main() {
    if true {
        int := 1
        fmt.println(int)
    }
    var y int = 2
    fmt.println(y)
}
"#;
    assert_ok(src);
}

#[test]
fn test_control_shadow_in_sibling_function_is_accepted() {
    let src = r#"
fn f1() {
    int := 1
    fmt.println(int)
}

fn f2() {
    var y int = 2
    fmt.println(y)
}
"#;
    assert_ok(src);
}

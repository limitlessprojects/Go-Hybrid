//! Go-standard `make(map[K]V)` / `make(map[K]V, n)` / `new(map[K]V)` used
//! to fail in the parser with HYB-E0002 (`expected '{', found ')'` or `,`)
//! because `parse_map_lit` always required a composite-literal `{`.
//! `make(chan T, n)` and `make([]T, n)` already parsed as type-as-value.
//!
//! The checker and emitter already treated an empty MapLit as a map type
//! argument (`classify_make_target`, `emit_make`, `emit_new`), which is why
//! the `make(map[K]V{})` workaround compiled. These tests pin the
//! brace-free Go spelling through check (0 diagnostics).

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

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(
        cs.is_empty(),
        "expected 0 diagnostics, got {cs:?} for:\n{src}"
    );
}

#[test]
fn make_map_without_braces_is_accepted() {
    assert_ok(
        r#"
fn main() {
    m := make(map[string]int)
    m["a"] = 1
}
"#,
    );
}

#[test]
fn make_map_with_size_hint_is_accepted() {
    assert_ok(
        r#"
fn main() {
    m := make(map[string]int, 8)
    m["a"] = 1
}
"#,
    );
}

#[test]
fn new_map_without_braces_is_accepted() {
    assert_ok(
        r#"
fn main() {
    p := new(map[string]int)
    _ = p
}
"#,
    );
}

#[test]
fn make_nested_map_with_size_is_accepted() {
    assert_ok(
        r#"
fn main() {
    m := make(map[string]map[int]int, 4)
    m["a"] = map[int]int{1: 2}
}
"#,
    );
}

#[test]
fn braced_make_map_workaround_still_accepted() {
    assert_ok(
        r#"
fn main() {
    m := make(map[string]int{}, 5)
    m["a"] = 1
}
"#,
    );
}

#[test]
fn map_composite_literal_still_accepted() {
    assert_ok(
        r#"
fn main() {
    m := map[string]int{"a": 1}
    _ = m["a"]
}
"#,
    );
}

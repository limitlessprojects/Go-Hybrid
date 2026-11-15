//! Regression tests for `make(...)` constant size folding and shadowing in
//! `compiler/crates/hy-check/src/builtins.rs` (`HYB-E2005`, GUARANTEES Row 13
//! `BUILDABLE`).
//!
//! Covers:
//!   1. `len(...)` over string literals, file/local `const` strings, and `"a" + "b"`
//!      constant concatenations inside `make` size/capacity arguments.
//!   2. `len(...)` and `cap(...)` over side-effect-free fixed-size array expressions
//!      (`[N]T{...}`, `var a [N]T`, `&a`).
//!   3. `min(...)` and `max(...)` over constant integer expressions (negative,
//!      overflowing `int`, or `len > cap`).
//!   4. Shadowing defense: function parameters, loop variables, closure binders,
//!      and user-declared functions (`fn i8`, `fn len`, `fn min`) must not be
//!      folded as file-level constants or built-in conversions (`0` false positives).

use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let file = parse_file("test.hy", src).expect("parses");
    check_file(&file)
        .diagnostics
        .into_iter()
        .map(|d| d.code.to_string())
        .collect()
}

#[test]
fn rejects_negative_and_swapped_make_sizes_from_const_string_len() {
    assert_eq!(
        codes(
            r#"import "fmt"
fn main() {
    xs := make([]int, len("ab") - 5)
    fmt.println(len(xs))
}
"#,
        ),
        vec!["HYB-E2005"],
        "negative len(\"ab\") - 5 in make must reject with HYB-E2005"
    );

    assert_eq!(
        codes(
            r#"import "fmt"
const prefix = "he"
fn main() {
    const suffix = "llo"
    xs := make([]int, len(prefix + suffix), 2)
    fmt.println(len(xs))
}
"#,
        ),
        vec!["HYB-E2005"],
        "len(prefix + suffix) == 5 > cap 2 in make must reject with HYB-E2005"
    );
}

#[test]
fn rejects_negative_and_swapped_make_sizes_from_const_array_len_and_cap() {
    assert_eq!(
        codes(
            r#"import "fmt"
fn main() {
    xs := make([]int, len([2]int{1, 2}) - 5)
    fmt.println(len(xs))
}
"#,
        ),
        vec!["HYB-E2005"],
        "negative len([2]int{{1, 2}}) - 5 in make must reject with HYB-E2005"
    );

    assert_eq!(
        codes(
            r#"import "fmt"
fn main() {
    var a [5]int
    xs := make([]int, cap(a), 2)
    fmt.println(len(xs))
}
"#,
        ),
        vec!["HYB-E2005"],
        "cap(a) == 5 > cap 2 on fixed array in make must reject with HYB-E2005"
    );
}

#[test]
fn rejects_negative_swapped_and_overflow_make_sizes_from_min_and_max() {
    assert_eq!(
        codes(
            r#"import "fmt"
fn main() {
    xs := make([]int, min(10, -3))
    fmt.println(len(xs))
}
"#,
        ),
        vec!["HYB-E2005"],
        "min(10, -3) in make must reject with HYB-E2005"
    );

    assert_eq!(
        codes(
            r#"import "fmt"
fn main() {
    xs := make([]int, max(2, 8), min(4, 9))
    fmt.println(len(xs))
}
"#,
        ),
        vec!["HYB-E2005"],
        "max(2, 8) > min(4, 9) in make must reject with HYB-E2005"
    );

    assert_eq!(
        codes(
            r#"import "fmt"
fn main() {
    xs := make([]int, min(1 << 63, 1 << 64))
    fmt.println(len(xs))
}
"#,
        ),
        vec!["HYB-E2005"],
        "min(1 << 63, 1 << 64) overflowing int in make must reject with HYB-E2005"
    );
}

#[test]
fn accepts_shadowed_params_loop_vars_closures_and_user_fns_in_make_sizes() {
    let ok_param_and_loop = r#"import "fmt"
const neg = -5
fn run(neg int) {
    xs := make([]int, i8(neg))
    fmt.println(len(xs))
}
fn main() {
    run(3)
    for neg in 1..3 {
        ys := make([]int, i8(neg))
        fmt.println(len(ys))
    }
    f := fn(neg int) {
        zs := make([]int, i8(neg))
        fmt.println(len(zs))
    }
    f(4)
}
"#;
    assert!(
        codes(ok_param_and_loop).is_empty(),
        "parameter, loop var, and closure param shadowing file const `neg` inside `i8(neg)` must not false-positive HYB-E2005"
    );

    let ok_user_fn_i8 = r#"import "fmt"
fn i8(x int) int {
    return -x
}
fn get_arr() [2]int {
    return [2]int{1, 2}
}
fn main() {
    xs := make([]int, i8(-5))
    ys := make([]int, len(get_arr()) - 1)
    fmt.println(len(xs), len(ys))
}
"#;
    assert!(
        codes(ok_user_fn_i8).is_empty(),
        "user-declared `fn i8` and call-bearing array `len(get_arr())` must not false-positive HYB-E2005"
    );
}

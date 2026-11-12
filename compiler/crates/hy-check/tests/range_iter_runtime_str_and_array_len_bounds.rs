//! Regression tests for `range_iter.rs` (`typed_int_bound`) classifying
//! runtime `len(...)` / `cap(...)` bounds as typed `int` (`HYB-E2009`,
//! GUARANTEES Row 13 `BUILDABLE`) on:
//!   1. Runtime string slices (`len(s[0:3])`), runtime string concatenations
//!      (`len("abc" + s)`), and string interpolations (`len("abc{s}")`).
//!   2. Call-bearing array expressions (`len(get_arr())`, `cap(get_arr())`,
//!      `len([2]int{one(), 2})`), which Go evaluates at runtime as `int`.

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
fn rejects_mismatched_fixed_width_range_bounds_with_runtime_string_len() {
    assert_eq!(
        codes(
            r#"import "fmt"
fn main() {
    var lo i64 = 0
    s := "abcdef"
    for i in lo..len(s[0:3]) {
        fmt.println(i)
    }
    for j in lo..len("abc" + s) {
        fmt.println(j)
    }
    for k in lo..len("abc{s}") {
        fmt.println(k)
    }
}
"#,
        ),
        vec!["HYB-E2009", "HYB-E2009", "HYB-E2009"],
        "runtime `len(s[0:3])`, `len(\"abc\" + s)`, and `len(\"abc{{s}}\")` with `i64` lo bound must reject HYB-E2009"
    );
}

#[test]
fn rejects_mismatched_fixed_width_range_bounds_with_call_bearing_array_len_and_cap() {
    assert_eq!(
        codes(
            r#"import "fmt"
fn get_arr() [2]int {
    return [2]int{1, 2}
}
fn one() int {
    return 1
}
fn main() {
    var lo u8 = 0
    for i in lo..len(get_arr()) {
        fmt.println(i)
    }
    for j in lo..cap(get_arr()) {
        fmt.println(j)
    }
    for k in lo..len([2]int{one(), 2}) {
        fmt.println(k)
    }
}
"#,
        ),
        vec!["HYB-E2009", "HYB-E2009", "HYB-E2009"],
        "call-bearing array `len`/`cap` bounds with `u8` lo bound must reject HYB-E2009"
    );
}

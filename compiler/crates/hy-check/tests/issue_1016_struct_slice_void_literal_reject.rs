//! Issue #1016 — void calls and zero-value builtins inside struct and slice
//! literals must reject with HYB-E1002.
//!
//! `kinds_conflict` treats `OpKind::Void` as non-conflicting because a count
//! check owns that case. Struct fields and slice/array elements never called
//! `check_single_value_position`, so `hy-check` accepted programs Go refuses
//! with `(no value) used as value`.

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

fn assert_code(src: &str, want: &str) {
    let cs = codes(src);
    assert!(
        !cs.is_empty() && cs.iter().all(|c| c == want),
        "expected only [{want}], got {cs:?} for:\n{src}"
    );
}

#[test]
fn void_call_in_struct_field_rejects() {
    assert_code(
        "struct box { x int }\n\nfn noop() {}\n\nfn main() {\n    b := box{x: noop()}\n    _ = b\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn void_call_in_slice_element_rejects() {
    assert_code(
        "fn noop() {}\n\nfn main() {\n    xs := []int{noop()}\n    _ = xs\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn close_builtin_in_slice_element_rejects() {
    assert_code(
        "fn main() {\n    ch := make(chan int, 1)\n    xs := []int{close(ch)}\n    _ = xs\n}\n",
        "HYB-E1002",
    );
}

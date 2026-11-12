//! Three-index (full) slices `a[lo:hi:max]` must parse and emit.
//!
//! `Expr::Slice` had no cap field and the parser stopped at the second `:`, so
//! `a[1:3:4]` failed at the `:` before the max. The cap is now carried through
//! and emitted, and an omitted middle index is filled because Go requires it.

use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit_src(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("parses");
    emit_go(&ast).expect("emits")
}

#[test]
fn full_slice_emits_all_three_indices() {
    let go = emit_src(
        "\
fn f(a []int) []int {
    return a[1:3:4]
}
",
    );
    assert!(go.contains("a[1:3:4]"), "expected a three-index slice, got:\n{go}");
}

#[test]
fn plain_slice_is_unchanged() {
    let go = emit_src(
        "\
fn f(a []int) []int {
    return a[2:4]
}
",
    );
    assert!(go.contains("a[2:4]"), "expected a two-index slice, got:\n{go}");
    assert!(!go.contains("a[2:4:4]"), "two-index slice grew a max:\n{go}");
}

#[test]
fn omitted_middle_index_is_filled_for_a_full_slice() {
    let go = emit_src(
        "\
fn f(a []int) []int {
    return a[1::4]
}
",
    );
    assert!(go.contains("cap(a)"), "expected a cap clamp, got:\n{go}");
    assert!(go.contains("len(a)"), "expected a len fill, got:\n{go}");
}

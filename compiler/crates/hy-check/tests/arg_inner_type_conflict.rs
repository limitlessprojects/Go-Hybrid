//! #3416 / #4620 (argument half) — a function or method call whose ARGUMENT
//! carries a container (or pointer) whose INNER type differs from the declared
//! parameter's.
//!
//! `OpKind` collapses container inners: `Map(Str, Int)` fits
//! `map[string]int64` as well as `map[string]int32`, so `take(m)` with
//! `take(m map[string]int64)` passed `hyb check` with 0 diagnostics and then
//! failed `go build` (`cannot use m (variable of type map[string]int32) as
//! map[string]int64 value in argument to take`). The argument walk now reuses
//! the #3412 spelling comparison (`deep_composite_conflict`), and an inferred
//! `:=` local of a composite literal keeps its spelling so the walk has
//! something to compare.
//!
//! Controls pin the fail-open edges: the same spelling accepts, an untyped
//! constant accepts, an addressable `&x` still matches a pointer parameter,
//! and a bare alias spelling keeps failing open on purpose.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_has(r: &hy_check::CheckResult, code: &str) {
    let cs = error_codes(r);
    assert!(cs.iter().any(|c| c == code), "expected {}, got {:?}", code, cs);
    assert_eq!(r.verdict, Verdict::Reject);
}

fn assert_clean(r: &hy_check::CheckResult) {
    let cs = error_codes(r);
    assert!(cs.is_empty(), "expected clean accept, got {:?}", cs);
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn fn_arg_declared_map_value_width_rejects_e1002() {
    assert_has(
        &check(
            r#"package main

fn take(m map[string]int64) int { return len(m) }

fn main() {
    var m map[string]int32
    take(m)
}
"#,
        ),
        "HYB-E1002",
    );
}

#[test]
fn fn_arg_inferred_map_literal_width_rejects_e1002() {
    // `m := map[string]int32{...}` is a TYPED local: the literal spelling is
    // `map[string]int32`, so the argument walk has a spelling to compare.
    assert_has(
        &check(
            r#"package main

fn take(m map[string]int64) int { return len(m) }

fn main() {
    m := map[string]int32{"a": 1}
    take(m)
}
"#,
        ),
        "HYB-E1002",
    );
}

#[test]
fn fn_arg_inline_map_literal_width_rejects_e1002() {
    assert_has(
        &check(
            r#"package main

fn take(m map[string]int64) int { return len(m) }

fn main() {
    take(map[string]int32{"a": 1})
}
"#,
        ),
        "HYB-E1002",
    );
}

#[test]
fn fn_arg_map_key_width_rejects_e1002() {
    assert_has(
        &check(
            r#"package main

fn take(m map[int64]string) int { return len(m) }

fn main() {
    var m map[int32]string
    take(m)
}
"#,
        ),
        "HYB-E1002",
    );
}

#[test]
fn fn_arg_slice_element_width_rejects_e1002() {
    assert_has(
        &check(
            r#"package main

fn take(xs []int64) int { return len(xs) }

fn main() {
    var xs []int32
    take(xs)
}
"#,
        ),
        "HYB-E1002",
    );
}

#[test]
fn fn_arg_nested_slice_width_rejects_e1002() {
    assert_has(
        &check(
            r#"package main

fn take(xs [][]int64) int { return len(xs) }

fn main() {
    var xs [][]int32
    take(xs)
}
"#,
        ),
        "HYB-E1002",
    );
}

#[test]
fn fn_arg_inferred_make_chan_element_width_rejects_e1002() {
    assert_has(
        &check(
            r#"package main

fn take(ch chan int64) int { return 1 }

fn main() {
    ch := make(chan int32, 1)
    take(ch)
}
"#,
        ),
        "HYB-E1002",
    );
}

#[test]
fn method_arg_map_value_width_rejects_e1002() {
    assert_has(
        &check(
            r#"package main

struct box {
    n int
}

fn (b box) take(m map[string]int64) int { return len(m) }

fn main() {
    b := box{n: 1}
    m := map[string]int32{"a": 1}
    b.take(m)
}
"#,
        ),
        "HYB-E1002",
    );
}

#[test]
fn matching_inner_spelling_accepts() {
    assert_clean(&check(
        r#"package main

fn take(m map[string]int32) int { return len(m) }

fn main() {
    m := map[string]int32{"a": 1}
    var n map[string]int32 = m
    take(m)
    take(n)
}
"#,
    ));
}

#[test]
fn same_width_other_container_forms_accept() {
    assert_clean(&check(
        r#"package main

fn take(xs []int64) int { return len(xs) }

fn main() {
    var xs []int64
    take(xs)
    take([]int64{1, 2})
}
"#,
    ));
}

#[test]
fn untyped_element_literal_against_wider_slice_accepts() {
    // A slice literal whose element type is spelled by the *parameter* still
    // matches; the walk must not invent a conflict from the element size.
    assert_clean(&check(
        r#"package main

fn take(xs []int64) int { return len(xs) }

fn main() {
    take([]int64{1})
    take([]int64{nil_len()})
}

fn nil_len() int64 { return 0 }
"#,
    ));
}

#[test]
fn addressable_pointer_argument_still_accepts() {
    assert_clean(&check(
        r#"package main

fn take(p *int32) int { return 1 }

fn main() {
    var x int32 = 1
    take(&x)
}
"#,
    ));
}

#[test]
fn alias_spelling_matching_target_fails_open() {
    // `m32` renders as a bare word: the deep comparison has no family to
    // split, so the alias form keeps failing open on purpose (Go accepts it
    // because it IS `map[string]int64`).
    assert_clean(&check(
        r#"package main

type m32 = map[string]int64

fn take(m map[string]int64) int { return len(m) }

fn main() {
    var m m32
    take(m)
}
"#,
    ));
}

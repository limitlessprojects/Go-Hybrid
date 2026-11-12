//! Regression tests for `rangebind.rs` preserving outer `for`, `match`, and
//! `select` bindings when a same-named inner `for <var> in lo..hi` reads `<var>`
//! through an index, field, or slice projection (`0..a[0]`, `0..it.n`) in `hi`
//! (GUARANTEES Row 13 `BUILDABLE`).

use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit(src: &str) -> String {
    let file = parse_file("test.hy", src).expect("parses");
    emit_go(&file).expect("emits")
}

#[test]
fn preserves_outer_iterate_binding_projected_in_same_name_range_hi() {
    let go_idx = emit(
        r#"package main

import "fmt"

fn main() {
    xs := [][]int{[]int{3}, []int{4}}
    for a in xs {
        for a in 0..a[0] {
            fmt.println(a)
        }
    }
}
"#,
    );
    assert!(
        go_idx.contains("for _, a := range xs {")
            && go_idx.contains("hyHi0 := a[0]")
            && go_idx.contains("for a := 0; a < hyHi0; a++ {"),
        "expected outer slice binding `a` to be preserved and `hyHi0 := a[0]` hoisted:\n{go_idx}"
    );

    let go_field = emit(
        r#"package main

import "fmt"

struct item {
    n int
}

fn main() {
    xs := []item{item{n: 3}}
    for it in xs {
        for it in 0..it.n {
            fmt.println(it)
        }
    }
}
"#,
    );
    assert!(
        go_field.contains("for _, it := range xs {")
            && go_field.contains("hyHi0 := it.n")
            && go_field.contains("for it := 0; it < hyHi0; it++ {"),
        "expected outer struct binding `it` to be preserved and `hyHi0 := it.n` hoisted:\n{go_field}"
    );
}

#[test]
fn preserves_outer_match_and_select_bindings_projected_in_same_name_range_hi() {
    let go_match = emit(
        r#"package main

import "fmt"

fn main() {
    var x option[[]int] = some([]int{3})
    match x {
        some(a): {
            for a in 0..a[0] {
                fmt.println(a)
            }
        }
        none: fmt.println(0)
    }
}
"#,
    );
    assert!(
        go_match.contains("a := hyMatch0.val")
            && go_match.contains("hyHi1 := a[0]")
            && go_match.contains("for a := 0; a < hyHi1; a++ {"),
        "expected `some(a)` binding to be injected and `hyHi1 := a[0]` hoisted:\n{go_match}"
    );

    let go_select = emit(
        r#"package main

import "fmt"

struct item {
    n int
}

fn main() {
    ch := make(chan item, 1)
    ch <- item{n: 3}
    select {
        it := <-ch: {
            for it in 0..it.n {
                fmt.println(it)
            }
        }
        default: fmt.println(0)
    }
}
"#,
    );
    assert!(
        go_select.contains("case it := <-ch:")
            && go_select.contains("hyHi0 := it.n")
            && go_select.contains("for it := 0; it < hyHi0; it++ {"),
        "expected `case it := <-ch:` binding to be preserved and `hyHi0 := it.n` hoisted:\n{go_select}"
    );
}

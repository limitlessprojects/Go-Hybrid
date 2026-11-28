//! Regression tests for SPEC N5 map auto-allocation on trailing elements of
//! partial elided array literals (`[...]`) in composite slots (`[][n]bag{[bag{}]}`,
//! `[][0x2]bag{[bag{}]}`, `[][(2)]bag{[bag{}]}`, `map[string][n]bag{"a": [bag{}]}`)
//! (`compiler/crates/hy-emit/src/expr.rs`).

use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit(src: &str) -> String {
    let file = parse_file("test.hy", src).expect("parses");
    emit_go(&file).expect("emits")
}

#[test]
fn fills_trailing_n5_elements_on_elided_partial_arrays_with_hex_and_paren_lengths() {
    let go_hex = emit(
        r#"package main

import "fmt"

struct bag {
    counts map[string]int
}

fn main() {
    xs := [][0x2]bag{[bag{}]}
    xs[0][1].counts["a"] = 1
    fmt.println(xs[0][1].counts["a"])
}
"#,
    );
    assert!(
        go_hex.contains("[0x2]bag{bag{counts: map[string]int{}}, bag{counts: map[string]int{}}}"),
        "expected hex-length `[0x2]bag` elided partial array to fill trailing N5 slot:\n{go_hex}"
    );

    let go_paren = emit(
        r#"package main

import "fmt"

struct bag {
    counts map[string]int
}

fn main() {
    xs := [][(2)]bag{[bag{}]}
    xs[0][1].counts["a"] = 1
    fmt.println(xs[0][1].counts["a"])
}
"#,
    );
    assert!(
        go_paren.contains("[(2)]bag{bag{counts: map[string]int{}}, bag{counts: map[string]int{}}}"),
        "expected parenthesized-length `[(2)]bag` elided partial array to fill trailing N5 slot:\n{go_paren}"
    );
}

#[test]
fn emits_tail_fill_closure_for_elided_partial_arrays_with_const_or_expr_lengths() {
    let go_const = emit(
        r#"package main

import "fmt"

struct bag {
    counts map[string]int
}

const n = 2

fn main() {
    xs := [][n]bag{[bag{}]}
    m := map[string][n]bag{"a": [bag{}]}
    xs[0][1].counts["a"] = 1
    fmt.println(xs[0][1].counts["a"], len(m))
}
"#,
    );
    assert_eq!(
        go_const
            .matches("for hyIdx := 1; hyIdx < len(hyArr); hyIdx++ { hyArr[hyIdx] = bag{counts: map[string]int{}} }")
            .count(),
        2,
        "expected both slice and map elided partial `[n]bag` slots to emit N5 tail-fill closures:\n{go_const}"
    );
}

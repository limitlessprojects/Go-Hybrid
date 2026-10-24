use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn rangebind_preserves_index_var_used_in_indexed_callee() {
    let src = r#"package main

import "fmt"

fn step_a(x int) int {
    return x + 1
}

fn step_b(x int) int {
    return x + 2
}

fn main() {
    fns := []fn(int) int{step_a, step_b}
    for i, _ in fns {
        fmt.println(fns[i](10))
    }
}
"#;
    let file = parse_file("test.hy", src).expect("parse succeeds");
    let go = emit_go(&file).expect("emit succeeds");
    assert!(
        go.contains("for i, _ := range fns {"),
        "expected index `i` read in `fns[i](10)` to remain bound, got:\n{go}"
    );
    assert!(
        !go.contains("for range fns {"),
        "must not elide `i` when `fns[i](10)` reads it:\n{go}"
    );
}

#[test]
fn rangebind_preserves_value_var_used_as_indexed_callee_base() {
    let src = r#"package main

import "fmt"

fn step_a() int {
    return 1
}

fn main() {
    groups := [][]fn() int{[]fn() int{step_a}}
    idx := 0
    for i, fns in groups {
        fmt.println(i)
        fmt.println(fns[idx]())
    }
}
"#;
    let file = parse_file("test.hy", src).expect("parse succeeds");
    let go = emit_go(&file).expect("emit succeeds");
    assert!(
        go.contains("for i, fns := range groups {"),
        "expected `fns` read as base of `fns[idx]()` to remain bound, got:\n{go}"
    );
}

#[test]
fn rangebind_handles_selector_indexed_callee_without_confusing_field_name() {
    let src = r#"package main

import "fmt"

struct runner {
    idx int
    fns []fn(int) int
}

fn run_all(r runner, items []int) {
    for i, fns in items {
        fmt.println(r.fns[i](1))
    }
    for i, v in items {
        fmt.println(v)
        fmt.println(r.fns[r.idx](1))
    }
}
"#;
    let file = parse_file("test.hy", src).expect("parse succeeds");
    let go = emit_go(&file).expect("emit succeeds");
    assert!(
        go.contains("for i, _ := range items {"),
        "expected `i` in `r.fns[i](1)` to stay bound while field `.fns` does not count as reading range var `fns`, got:\n{go}"
    );
    assert!(
        go.contains("for _, v := range items {"),
        "expected field `.idx` in `r.fns[r.idx](1)` not to count as reading `i`, got:\n{go}"
    );
}

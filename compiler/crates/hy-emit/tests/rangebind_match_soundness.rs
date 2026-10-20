use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn rangebind_ignores_dropped_match_arms_after_unguarded_some_or_redundant_wildcard() {
    let src = r#"package main

import "fmt"

fn main() {
    xs := []int{10, 20}
    opt := some(1)
    for i, v in xs {
        fmt.println(i)
        match opt {
            some(x): { fmt.println(x) }
            some(y) if v > 0: { fmt.println(y) }
            none: { fmt.println("none") }
            _: { fmt.println(v) }
        }
    }
}
"#;
    let file = parse_file("test.hy", src).expect("parse succeeds");
    let go = emit_go(&file).expect("emit succeeds");
    assert!(
        go.contains("for i, _ := range xs {"),
        "expected unread `v` (referenced only in dropped match arms) to be blanked, got:\n{go}"
    );
    assert!(
        !go.contains("for i, v := range xs"),
        "must not emit unused `v` when only dead match arms reference it:\n{go}"
    );
}

#[test]
fn rangebind_does_not_emit_unused_outer_some_or_select_binding_read_only_in_dropped_inner_match_arm() {
    let src = r#"package main

import "fmt"

fn check_nested(ch chan int, opt1 option[int], opt2 option[int]) {
    select {
        v := <-ch: {
            match opt1 {
                some(p): {
                    match opt2 {
                        some(x): { fmt.println(x) }
                        some(y) if p > 0 && v > 0: { fmt.println(y) }
                        none: { fmt.println("none") }
                    }
                }
                none: {}
            }
        }
        default: {}
    }
}
"#;
    let file = parse_file("test.hy", src).expect("parse succeeds");
    let go = emit_go(&file).expect("emit succeeds");
    assert!(
        go.contains("case <-ch:"),
        "expected select receive `v` read only in dropped inner match arm to lower to `case <-ch:`, got:\n{go}"
    );
    assert!(
        !go.contains("p := hyMatch"),
        "expected outer match binding `p` read only in dropped inner match arm to be omitted, got:\n{go}"
    );
}

#[test]
fn rangebind_does_not_treat_none_binding_as_shadowing_outer_range_var() {
    let src = r#"package main

import "fmt"

fn main() {
    xs := []int{10, 20}
    opt := some(1)
    for i, v in xs {
        fmt.println(i)
        match opt {
            some(x): { fmt.println(x) }
            none(v): { fmt.println(v) }
        }
    }
}
"#;
    let file = parse_file("test.hy", src).expect("parse succeeds");
    let go = emit_go(&file).expect("emit succeeds");
    assert!(
        go.contains("for i, v := range xs {"),
        "expected `v` read in `none(v)` body (which emit_match does not bind) to keep `v` bound, got:\n{go}"
    );
}

#[test]
fn rangebind_handles_same_name_inner_numeric_range_bound_shadowing() {
    let src_exclusive = r#"package main

import "fmt"

fn main() {
    xs := []int{10, 20}
    for i, v in xs {
        fmt.println(i)
        for v in 0..v {
            fmt.println(v)
        }
    }
}
"#;
    let file_ex = parse_file("test.hy", src_exclusive).expect("parse exclusive succeeds");
    let go_ex = emit_go(&file_ex).expect("emit exclusive succeeds");
    assert!(
        go_ex.contains("for i, _ := range xs {"),
        "exclusive `for v in 0..v` lowers to `for v := 0; v < v; v++` where `v` in `v < v` is the inner `v`, so outer `v` must be blanked:\n{go_ex}"
    );

    let src_inclusive = r#"package main

import "fmt"

fn main() {
    xs := []int{10, 20}
    for i, v in xs {
        fmt.println(i)
        for v in 0..=v {
            fmt.println(v)
        }
    }
}
"#;
    let file_inc = parse_file("test.hy", src_inclusive).expect("parse inclusive succeeds");
    let go_inc = emit_go(&file_inc).expect("emit inclusive succeeds");
    assert!(
        go_inc.contains("for i, v := range xs {"),
        "inclusive `for v in 0..=v` hoists `hyHi0 := v` before inner `v` is declared, so outer `v` is read:\n{go_inc}"
    );
}

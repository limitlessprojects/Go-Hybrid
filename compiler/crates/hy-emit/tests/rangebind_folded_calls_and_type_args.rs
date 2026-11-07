//! Regression tests for `rangebind.rs`:
//! 1. Folded bracketed call expressions (`fns[i]()`, `s.fns[i]()`, `fns[a[i + 1]]()`)
//!    and generic type array lengths (`holder[[n]int]{}`) must mark both the
//!    container and index/length identifiers as read across `for`, `match`,
//!    and `select` bindings.
//! 2. `make(T, ...)` and `new(T)` type-argument identifiers must not falsely
//!    count as value reads of an unread binding of the same name (`t`).
//! 3. `match` statements with no `some(v)` arms on the ordered-arms path
//!    (`none if cond:` / duplicate `none:`) emit no arm bodies in `emit_match`,
//!    so `rangebind` must not report bindings inside those dropped arms as read.

use hy_syntax::parse_file;

fn emit(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("parse ok");
    hy_emit::emit_file(&ast).expect("emit ok")
}

#[test]
fn rangebind_keeps_loop_var_for_folded_bracket_callee_and_index() {
    let src = r#"package main
import "fmt"

fn main() {
    batches := [][]fn(){}[0:0]
    for idx, fns in batches {
        fmt.println(idx)
        fns[0]()
    }

    handlers := []fn(){}
    ks := []int{0}
    for pos, k in ks {
        fmt.println(pos)
        handlers[k]()
    }
}
"#;
    let go = emit(src);
    assert!(
        go.contains("for idx, fns := range batches {"),
        "folded bracket callee `fns[0]()` must keep `fns` bound:\n{go}"
    );
    assert!(
        go.contains("for pos, k := range ks {"),
        "folded bracket index `handlers[k]()` must keep `k` bound:\n{go}"
    );
}

#[test]
fn rangebind_keeps_field_bracket_index_in_for_match_and_select() {
    let src = r#"package main

struct runner {
    fns []fn() bool
}

fn run(r runner, xs []int, opt option[int], ch chan int) {
    for _, idx in xs {
        _ = r.fns[idx]()
    }
    match opt {
        some(m) if r.fns[m](): {
        }
        some(n): {
            _ = r.fns[n]()
        }
        none: {}
    }
    select {
        s := <-ch: {
            _ = r.fns[s]()
        }
        default: {}
    }
}
"#;
    let go = emit(src);
    assert!(
        go.contains("for _, idx := range xs {"),
        "field bracket call `r.fns[idx]()` must keep `idx` bound:\n{go}"
    );
    assert!(
        go.contains("m := hyMatch0.val"),
        "match guard `r.fns[m]()` must prebind `m := hyMatch0.val`:\n{go}"
    );
    assert!(
        go.contains("n := hyMatch0.val"),
        "match arm body `r.fns[n]()` must bind `n := hyMatch0.val`:\n{go}"
    );
    assert!(
        go.contains("case s := <-ch:"),
        "select arm body `r.fns[s]()` must keep `case s := <-ch:`:\n{go}"
    );
}

#[test]
fn rangebind_does_not_count_field_selector_inside_folded_brackets_as_local_read() {
    let src = r#"package main
import "fmt"

struct state {
    idx int
}

fn run(fns []fn(), st state, xs []int) {
    for i, idx in xs {
        fmt.println(i)
        fns[st.idx]()
    }
}
"#;
    let go = emit(src);
    assert!(
        go.contains("for i, _ := range xs {"),
        "field selector `.idx` inside `fns[st.idx]()` must not count as reading local `idx`:\n{go}"
    );
}

#[test]
fn rangebind_blanks_unread_var_shadowing_make_or_new_type_arg() {
    let src = r#"package main
import "fmt"

struct item {
    v int
}

type int_list = []int

fn run(xs []int) {
    for i, item in xs {
        fmt.println(i)
        p := new(item)
        fmt.println(p)
    }
    for j, int_list in xs {
        fmt.println(j)
        s := make(int_list, 4)
        fmt.println(len(s))
    }
    for k, n in xs {
        fmt.println(k)
        s2 := make([]int, n)
        fmt.println(len(s2))
    }
}
"#;
    let go = emit(src);
    assert!(
        go.contains("for i, _ := range xs {"),
        "`new(item)` type argument must not count as reading loop var `item`:\n{go}"
    );
    assert!(
        go.contains("for j, _ := range xs {"),
        "`make(int_list, 4)` type argument must not count as reading loop var `int_list`:\n{go}"
    );
    assert!(
        go.contains("for k, n := range xs {"),
        "`make([]int, n)` size argument must still count as reading loop var `n`:\n{go}"
    );
}

#[test]
fn rangebind_ignores_ordered_match_without_some_arms() {
    let src = r#"package main
import "fmt"

fn run(xs []int, opt option[int], flag bool) {
    for i, v in xs {
        fmt.println(i)
        match opt {
            none if flag: {
                fmt.println(v)
            }
            _: {
                fmt.println(v)
            }
        }
    }
}
"#;
    let go = emit(src);
    assert!(
        go.contains("for i, _ := range xs {"),
        "ordered match with no `some(...)` arms emits no arm bodies and must blank `v`:\n{go}"
    );
    assert!(
        !go.contains("fmt.Println(v)"),
        "emit_match must not emit arm bodies when `some_arms` is empty:\n{go}"
    );
}

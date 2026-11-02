//! Issue #1614: `for` range-bind blanking must respect reads inside hoisted
//! `?` subexpressions of the bound's own `hi`. `Emitter::emit_for` hoists
//! `hyT0, hyErr0 := limit(x)` BEFORE `for x := 0; x < …; x++` — at that point
//! the inner `x` is not yet in scope, so the hoisted statement reads the
//! OUTER binding. `rangebind::body_reads` used to blanket the outer `x`
//! (`for i, _ := range xs`) because `var == target && !inclusive`, producing
//! invalid Go (`undefined: x`).

use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn rangebind_preserves_outer_var_read_by_hoisted_try_in_own_hi() {
    let src = r#"package main

import "fmt"

fn limit(n int) (int, error) {
    return n + 1, nil
}

fn run(xs []int) error {
    for i, x in xs {
        fmt.println(i)
        for x in 0..limit(x)? {
            fmt.println(x)
        }
    }
    return nil
}
"#;
    let file = parse_file("test.hy", src).expect("parse succeeds");
    let go = emit_go(&file).expect("emit succeeds");
    assert!(
        go.contains("for i, x := range xs {"),
        "outer `x` is read by the hoisted `limit(x)` (issue #1614) — must not be blanked, got:\n{go}"
    );
    assert!(
        !go.contains("for i, _ := range xs {"),
        "blanking the outer `x` makes the hoisted `limit(x)` read an undefined name:\n{go}"
    );
    // The hoisted try statement must land inside the outer loop, before the
    // inner range head, so it reads the still-in-scope outer `x`.
    let outer = go.find("for i, x := range xs {").expect("outer head");
    let hoisted = go.find("limit(x)").expect("hoisted limit(x)");
    let inner = go.find("for x := 0; x < ").expect("inner range head");
    assert!(
        outer < hoisted && hoisted < inner,
        "expected `limit(x)` hoisted between the outer head and the inner range head:\n{go}"
    );
}

#[test]
fn rangebind_still_blanks_outer_var_when_own_hi_shadowed_without_try() {
    // The #139 lowering must keep working: a try-free `hi` is re-evaluated
    // inside the loop head where the inner `v` shadows the outer one, so the
    // outer `v` is genuinely unread and must stay blanked.
    let src = r#"package main

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
    let file = parse_file("test.hy", src).expect("parse succeeds");
    let go = emit_go(&file).expect("emit succeeds");
    assert!(
        go.contains("for i, _ := range xs {"),
        "try-free same-name `hi` evaluates in the loop head where inner `v` shadows — outer `v` must stay blanked, got:\n{go}"
    );
}

#[test]
fn rangebind_preserves_outer_var_for_hoisted_side_effect_sibling_in_hi() {
    // `hoist_order_before_try` hoists the side-effecting sibling `g(x)`
    // before the hoisted try inside `hi`, also above the loop head.
    let src = r#"package main

import "fmt"

fn g(n int) int {
    return n
}

fn f() (int, error) {
    return 1, nil
}

fn main() {
    xs := []int{1, 2}
    for i, x in xs {
        fmt.println(i)
        for x in 0..g(x) + f()? {
            fmt.println(x)
        }
    }
}
"#;
    let file = parse_file("test.hy", src).expect("parse succeeds");
    let go = emit_go(&file).expect("emit succeeds");
    assert!(
        go.contains("for i, x := range xs {"),
        "outer `x` is read by the hoisted `g(x)` sibling in a try-carrying `hi` (issue #1614) — must not be blanked, got:\n{go}"
    );
    assert!(
        !go.contains("for i, _ := range xs {"),
        "blanking the outer `x` makes the hoisted `g(x)` read an undefined name:\n{go}"
    );
}

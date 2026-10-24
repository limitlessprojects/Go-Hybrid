use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn match_guard_with_indexed_call_binds_pattern_var_before_guard() {
    let src = r#"package main

import "fmt"

fn check_zero() bool {
    return true
}

fn run_match(opt option[int]) {
    preds := []fn() bool{check_zero}
    match opt {
        some(i) if preds[i](): {
            fmt.println("matched")
        }
        none: {}
    }
}
"#;
    let file = parse_file("test.hy", src).expect("parse succeeds");
    let go = emit_go(&file).expect("emit succeeds");
    assert!(
        go.contains("i := hyMatch0.val") && go.contains("if preds[i]() {"),
        "expected `some(i)` guard `preds[i]()` to prebind `i := hyMatch0.val`, got:\n{go}"
    );
}

#[test]
fn match_guard_shadow_does_not_hide_outer_range_var_read_in_later_arm_indexed_call() {
    let src = r#"package main

import "fmt"

fn step(x int) int {
    return x + 1
}

fn run_all(opts []option[int]) {
    fns := []fn(int) int{step}
    for i, opt in opts {
        match opt {
            some(i) if i > 0: {
                fmt.println(i)
            }
            some(x): {
                fmt.println(fns[i](x))
            }
            none: {}
        }
    }
}
"#;
    let file = parse_file("test.hy", src).expect("parse succeeds");
    let go = emit_go(&file).expect("emit succeeds");
    assert!(
        go.contains("for i, opt := range opts {"),
        "expected outer `i` read in `some(x)` arm via `fns[i](x)` to stay bound, got:\n{go}"
    );
    assert!(
        go.contains("hyMatched1 := false"),
        "expected guarded `some(i)` followed by `some(x)` reading outer `i` in `fns[i](x)` to isolate scopes via `hyMatched`, got:\n{go}"
    );
}

#[test]
fn match_some_shadowing_outer_range_var_in_indexed_call_blanks_unread_outer_var() {
    let src = r#"package main

import "fmt"

fn step(x int) int {
    return x + 1
}

fn run_all(opts []option[int]) {
    fns := []fn(int) int{step}
    for i, opt in opts {
        match opt {
            some(i): {
                fmt.println(fns[i](1))
            }
            none: {}
        }
    }
}
"#;
    let file = parse_file("test.hy", src).expect("parse succeeds");
    let go = emit_go(&file).expect("emit succeeds");
    assert!(
        go.contains("for _, opt := range opts {"),
        "expected outer `i` shadowed by `some(i)` in `fns[i](1)` to be blanked, got:\n{go}"
    );
    assert!(
        go.contains("i := hyMatch0.val"),
        "expected inner `some(i)` binding read by `fns[i](1)` to be emitted, got:\n{go}"
    );
}

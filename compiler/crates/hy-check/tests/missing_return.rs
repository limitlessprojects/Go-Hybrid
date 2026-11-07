//! BUILDABLE (GUARANTEES Row 13) — missing return. `hyb check` returned 0
//! diagnostics for a function with results whose body can fall off its end
//! (`fn f() int { }`, `if` without `else`, `for` with a condition, `for {}`
//! with a `break`, named results, methods, closures), and the emitted Go
//! failed `go build` with `missing return`.
//!
//! Every reject case was first reproduced as a real `go build` failure on
//! the emitted Go (go1.26.9), and every accept case builds. The accept cases
//! matter as much as the rejects: a rule that rejects a buildable program is
//! a regression, not a fix.

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
    assert_eq!(cs, vec![want.to_string()], "for:\n{src}");
}

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(
        cs.is_empty(),
        "expected 0 diagnostics, got {cs:?} for:\n{src}"
    );
}

// ---------------------------------------------------------------------------
// missing return
// ---------------------------------------------------------------------------

#[test]
fn empty_body_with_a_result_is_rejected() {
    assert_code(
        "fn f() int {\n}\n\nfn main() {\n    fmt.println(\"{f()}\")\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn if_without_else_is_not_terminating() {
    assert_code(
        "fn f(n int) int {\n    if n > 1 {\n        return 1\n    }\n}\n\nfn main() {\n    fmt.println(\"{f(1)}\")\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn conditional_and_range_loops_are_not_terminating() {
    assert_code(
        "fn f(n int) int {\n    for n > 0 {\n        return n\n    }\n}\n\nfn main() {\n    fmt.println(\"{f(1)}\")\n}\n",
        "HYB-E1002",
    );
    assert_code(
        "fn f(n int) int {\n    for i in 0..n {\n        return i\n    }\n}\n\nfn main() {\n    fmt.println(\"{f(1)}\")\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn infinite_loop_with_a_break_is_not_terminating() {
    assert_code(
        "fn f(n int) int {\n    for {\n        if n > 3 {\n            break\n        }\n        return 1\n    }\n}\n\nfn main() {\n    fmt.println(\"{f(1)}\")\n}\n",
        "HYB-E1002",
    );
    // `break loop` targets the innermost loop, i.e. this one.
    assert_code(
        "fn f(c chan int) int {\n    loop {\n        select {\n            v, ok := <-c => {\n                if !ok { break loop }\n                fmt.println(\"{v}\")\n            }\n        }\n    }\n}\n\nfn main() {\n    fmt.println(\"x\")\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn named_results_still_need_a_return() {
    assert_code(
        "fn f(n int) (code int) {\n    code = n\n}\n\nfn main() {\n    fmt.println(\"{f(1)}\")\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn methods_and_function_literals_are_checked() {
    assert_code(
        "struct counter {\n    n int\n}\n\nfn (c *counter) get() int {\n    c.n += 1\n}\n\nfn main() {\n    c := counter{n: 1}\n    fmt.println(\"{c.get()}\")\n}\n",
        "HYB-E1002",
    );
    assert_code(
        "fn main() {\n    g := fn() int {\n        fmt.println(\"x\")\n    }\n    fmt.println(\"{g()}\")\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn terminating_bodies_stay_accepted() {
    // if/else-if/else chain where every branch returns.
    assert_ok("fn f(n int) int {\n    if n > 1 {\n        return 1\n    } else if n > 0 {\n        return 2\n    } else {\n        return 3\n    }\n}\n\nfn main() {\n    fmt.println(\"{f(1)}\")\n}\n");
    // panic(...) terminates.
    assert_ok(
        "fn f(n int) int {\n    panic(\"no\")\n}\n\nfn main() {\n    fmt.println(\"{f(1)}\")\n}\n",
    );
    // A bare block ending in return.
    assert_ok("fn f(n int) int {\n    {\n        return n\n    }\n}\n\nfn main() {\n    fmt.println(\"{f(1)}\")\n}\n");
    // `for {}` with no break, and one whose only break belongs to an inner loop.
    assert_ok("fn f(n int) int {\n    for {\n    }\n}\n\nfn main() {\n    fmt.println(\"x\")\n}\n");
    assert_ok("fn f(n int) int {\n    for {\n        for {\n            break\n        }\n        return 2\n    }\n}\n\nfn main() {\n    fmt.println(\"{f(1)}\")\n}\n");
    // An unlabeled break inside a select leaves the select, not the loop.
    assert_ok("fn f(c chan int) int {\n    for {\n        select {\n            v := <-c => {\n                if v > 0 { break }\n                return v\n            }\n        }\n    }\n}\n\nfn main() {\n    fmt.println(\"x\")\n}\n");
    // A select whose every arm returns.
    assert_ok("fn f(c chan int) int {\n    select {\n        v := <-c => { return v }\n    }\n}\n\nfn main() {\n    fmt.println(\"x\")\n}\n");
    // An option match whose arms all return.
    assert_ok("fn f(o option[int]) int {\n    match o {\n        some(v) => { return v }\n        none => { return 0 }\n    }\n}\n\nfn main() {\n    fmt.println(\"{f(some(1))}\")\n}\n");
    // `fn main() (code int)` is the exit-code surface: it may fall off the end.
    assert_ok("fn main() (code int) {\n    code = 0\n    fmt.println(\"x\")\n}\n");
    // No results: nothing to return.
    assert_ok("fn f(n int) {\n    fmt.println(\"{n}\")\n}\n\nfn main() {\n    f(1)\n}\n");
}

//! Regression tests for issue #490:
//! A closure returned from a helper (single-value or multi-value), copied into
//! another closure variable, or carried inside a composite container shares
//! mutable GC storage with any reference-bearing argument/capture it closes
//! over. Moving that argument (or a peer) at a concurrency boundary freezes
//! the closure binding (`HYB-E3003`), invoking the closure inside a live map
//! slot-loan window triggers `HYB-E3102`, and passing a `const` composite to a
//! helper that returns a mutating closure rejects with `HYB-E3006`.

use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test_490.hy", src).expect("test source must parse");
    check_file(&ast)
}

#[test]
fn helper_returned_closure_freezes_when_captured_arg_moves() {
    let r = check(
        r#"
package main

import "fmt"

fn make_fn(m map[string]int) fn() {
    return fn() {
        m["closure"] = 1
    }
}

fn worker(m map[string]int, done chan int) {
    m["worker"] = 2
    done <- 1
}

fn main() {
    m := map[string]int{"init": 0}
    done := make(chan int, 1)
    f := make_fn(m)
    task worker(move m, done)
    f()
    <-done
    fmt.println("done")
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject, got: {:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 on post-move call of `f()`, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn copied_and_rebound_closure_freezes_when_captured_binding_moves() {
    let r_copy = check(
        r#"
package main

fn make_fn(m map[string]int) fn() {
    return fn() {
        m["closure"] = 1
    }
}

fn worker(m map[string]int, done chan int) {
    m["worker"] = 2
    done <- 1
}

fn main() {
    m := map[string]int{"init": 0}
    done := make(chan int, 1)
    f := make_fn(m)
    f2 := f
    _ = f
    task worker(move m, done)
    f2()
    <-done
}
"#,
    );
    assert_eq!(r_copy.verdict, Verdict::Reject, "{:?}", r_copy.diagnostics);
    assert!(
        r_copy.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 on copied closure `f2()`, got: {:?}",
        r_copy.diagnostics
    );

    let r_inline_copy = check(
        r#"
package main

fn worker(m map[string]int, done chan int) {
    m["worker"] = 2
    done <- 1
}

fn main() {
    m := map[string]int{"init": 0}
    done := make(chan int, 1)
    f := fn() {
        m["closure"] = 1
    }
    var f2 fn() = fn() {}
    f2 = f
    _ = f
    task worker(move m, done)
    f2()
    <-done
}
"#,
    );
    assert_eq!(
        r_inline_copy.verdict,
        Verdict::Reject,
        "{:?}",
        r_inline_copy.diagnostics
    );
    assert!(
        r_inline_copy.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 on rebound closure `f2()`, got: {:?}",
        r_inline_copy.diagnostics
    );
}

#[test]
fn multi_value_returned_closure_freezes_when_captured_arg_moves() {
    let r_define = check(
        r#"
package main

fn make_fn_pair(m map[string]int) (fn(), bool) {
    return fn() {
        m["closure"] = 1
    }, true
}

fn worker(m map[string]int, done chan int) {
    m["worker"] = 2
    done <- 1
}

fn main() {
    m := map[string]int{"init": 0}
    done := make(chan int, 1)
    f, ok := make_fn_pair(m)
    _ = ok
    task worker(move m, done)
    f()
    <-done
}
"#,
    );
    assert_eq!(r_define.verdict, Verdict::Reject, "{:?}", r_define.diagnostics);
    assert!(
        r_define.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 on multi-return `f()`, got: {:?}",
        r_define.diagnostics
    );

    let r_assign = check(
        r#"
package main

fn make_fn_pair(m map[string]int) (fn(), bool) {
    return fn() {
        m["closure"] = 1
    }, true
}

fn worker(m map[string]int, done chan int) {
    m["worker"] = 2
    done <- 1
}

fn main() {
    m := map[string]int{"init": 0}
    done := make(chan int, 1)
    var f fn() = fn() {}
    ok := false
    f, ok = make_fn_pair(m)
    _ = ok
    task worker(move m, done)
    f()
    <-done
}
"#,
    );
    assert_eq!(r_assign.verdict, Verdict::Reject, "{:?}", r_assign.diagnostics);
    assert!(
        r_assign.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 on multi-return assign `f()`, got: {:?}",
        r_assign.diagnostics
    );
}

#[test]
fn composite_containers_holding_closures_freeze_when_captured_arg_moves() {
    let r_struct_helper = check(
        r#"
package main

struct runner {
    f fn()
}

fn make_runner(m map[string]int) runner {
    return runner{
        f: fn() {
            m["closure"] = 1
        },
    }
}

fn worker(m map[string]int, done chan int) {
    m["worker"] = 2
    done <- 1
}

fn main() {
    m := map[string]int{"init": 0}
    done := make(chan int, 1)
    r := make_runner(m)
    task worker(move m, done)
    r.f()
    <-done
}
"#,
    );
    assert_eq!(
        r_struct_helper.verdict,
        Verdict::Reject,
        "{:?}",
        r_struct_helper.diagnostics
    );
    assert!(
        r_struct_helper.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 on `r.f()`, got: {:?}",
        r_struct_helper.diagnostics
    );

    let r_inline_lit = check(
        r#"
package main

struct runner {
    f fn()
}

fn wrap_fn(cb fn()) fn() {
    return cb
}

fn worker(m map[string]int, done chan int) {
    m["worker"] = 2
    done <- 1
}

fn main() {
    m := map[string]int{"init": 0}
    done := make(chan int, 1)
    r := runner{
        f: wrap_fn(fn() {
            m["closure"] = 1
        }),
    }
    task worker(move m, done)
    r.f()
    <-done
}
"#,
    );
    assert_eq!(
        r_inline_lit.verdict,
        Verdict::Reject,
        "{:?}",
        r_inline_lit.diagnostics
    );
    assert!(
        r_inline_lit.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 on inline literal `r.f()`, got: {:?}",
        r_inline_lit.diagnostics
    );
}

#[test]
fn helper_returned_and_copied_closures_invalidate_map_slot_loans() {
    let r = check(
        r#"
package main

import "fmt"

struct item {
    n int
}

fn make_mutator(m map[string]item) fn() {
    return fn() {
        m["b"] = item{n: 2}
    }
}

fn main() {
    m := map[string]item{"a": item{n: 1}}
    f := make_mutator(m)
    f2 := f
    _ = f
    slot := &m["a"]
    f2()
    fmt.println(slot.n)
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3102"),
        "expected HYB-E3102 on slot-loan invalidation via `f2()`, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn helper_returning_closure_that_mutates_const_param_rejects_e3006() {
    let r_mut = check(
        r#"
package main

const shared = map[string]int{"init": 0}

fn make_writer(m map[string]int) fn() {
    return fn() {
        m["k"] = 1
    }
}

fn main() {
    f := make_writer(shared)
    f()
}
"#,
    );
    assert_eq!(r_mut.verdict, Verdict::Reject, "{:?}", r_mut.diagnostics);
    assert!(
        r_mut.diagnostics.iter().any(|d| d.code == "HYB-E3006"),
        "expected HYB-E3006 when passing const map to helper returning mutating closure, got: {:?}",
        r_mut.diagnostics
    );

    let r_read = check(
        r#"
package main

import "fmt"

const shared = map[string]int{"init": 0}

fn make_reader(m map[string]int) fn() int {
    return fn() int {
        return m["init"]
    }
}

fn main() {
    f := make_reader(shared)
    fmt.println(f())
}
"#,
    );
    assert_eq!(
        r_read.verdict,
        Verdict::Accept,
        "read-only closure over const map must accept, got: {:?}",
        r_read.diagnostics
    );
}

#[test]
fn primitive_only_helper_returned_closure_positive_control_accepts() {
    let r = check(
        r#"
package main

import "fmt"

fn make_adder(base int) fn() int {
    return fn() int {
        return base + 1
    }
}

fn worker(m map[string]int, done chan int) {
    m["worker"] = 2
    done <- 1
}

fn main() {
    m := map[string]int{"init": 0}
    done := make(chan int, 1)
    base := 41
    f := make_adder(base)
    task worker(move m, done)
    fmt.println(f())
    <-done
}
"#,
    );
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected Accept for primitive-only closure helper, got: {:?}",
        r.diagnostics
    );
    assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
}

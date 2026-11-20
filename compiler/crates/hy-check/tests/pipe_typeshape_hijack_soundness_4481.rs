//! Regression tests for #4481: `multi_value_shapes`, `multi_value_tys`, and
//! `ChannelLivenessChecker::is_pipe_call` must not hijack user-defined
//! `pipe` / `new_pipe` callees or shadowed `hy.pipe` / `hy.new_pipe` receivers
//! as `OwnHandle("hy.send_cap")` / `OwnHandle("hy.recv_cap")`.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).expect("must parse");
    check_file(&ast)
}

fn error_codes(src: &str) -> Vec<String> {
    let r = check(src);
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_has(code: &str, src: &str) {
    let cs = error_codes(src);
    assert!(
        cs.iter().any(|c| c == code),
        "expected {code}, got {cs:?} for:\n{src}"
    );
}

fn assert_accept(src: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected Accept, got {:?} for:\n{src}",
        r.diagnostics
    );
    assert!(
        r.diagnostics.is_empty(),
        "expected 0 diagnostics, got {:?} for:\n{src}",
        r.diagnostics
    );
}

#[test]
fn user_fn_pipe_returning_closures_rejected_at_spawn_e3001() {
    let src = r#"
fn pipe() (fn(), fn()) {
    m := map[string]int{"k": 0}
    return fn() { m["k"] = 1 }, fn() { m["k"] = 2 }
}

fn worker(f fn(), done chan<- int) {
    f()
    done <- 1
}

fn main() {
    a, b := pipe()
    done := make(chan int, 2)
    task worker(move a, done)
    task worker(move b, done)
    <-done
    <-done
}
"#;
    let cs = error_codes(src);
    assert_eq!(
        cs.iter().filter(|c| c.as_str() == "HYB-E3001").count(),
        2,
        "both closure bindings unpacked from user `pipe()` must be rejected at spawn, got {cs:?}"
    );
}

#[test]
fn user_fn_new_pipe_returning_mutex_rejected_at_move_spawn_e3001() {
    let src = r#"
import "sync"

fn new_pipe() (*sync.mutex, []int) {
    return new(sync.mutex), []int{0}
}

fn worker(mu *sync.mutex, done chan<- int) {
    mu.lock()
    mu.unlock()
    done <- 1
}

fn main() {
    a, b := new_pipe()
    _ = b
    done := make(chan int, 1)
    task worker(move a, done)
    <-done
}
"#;
    assert_has("HYB-E3001", src);
}

#[test]
fn local_closure_named_pipe_returning_closures_rejected_at_spawn_e3001() {
    let src = r#"
fn worker(f fn(), done chan<- int) {
    f()
    done <- 1
}

fn main() {
    n := 0
    pipe := fn() (fn(), fn()) {
        return fn() { n = 1 }, fn() { n = 2 }
    }
    a, b := pipe()
    done := make(chan int, 2)
    task worker(move a, done)
    task worker(move b, done)
    <-done
    <-done
}
"#;
    assert_has("HYB-E3001", src);
}

#[test]
fn generic_user_fn_pipe_returning_unproven_type_param_rejected_e3005() {
    let src = r#"
fn pipe[t](x t) (t, int) {
    return x, 0
}

fn sink[t](v t) {
    _ = v
}

fn launch[t](x t) {
    a, b := pipe[t](x)
    _ = b
    task sink[t](move a)
}

fn main() {}
"#;
    assert_has("HYB-E3005", src);
}

#[test]
fn shadowed_local_hy_method_pipe_rejected_at_spawn_e3001() {
    let src = r#"
struct router {}

fn (r router) pipe() (fn(), fn()) {
    m := map[string]int{"k": 0}
    return fn() { m["k"] = 1 }, fn() { m["k"] = 2 }
}

fn worker(f fn(), done chan<- int) {
    f()
    done <- 1
}

fn main() {
    hy := router{}
    a, b := hy.pipe()
    done := make(chan int, 2)
    task worker(move a, done)
    task worker(move b, done)
    <-done
    <-done
}
"#;
    let cs = error_codes(src);
    assert_eq!(
        cs.iter().filter(|c| c.as_str() == "HYB-E3001").count(),
        2,
        "shadowed `hy.pipe()` returning closures must reject both task moves, got {cs:?}"
    );
}

#[test]
fn shadowed_param_and_closure_param_hy_pipe_rejected_at_spawn_e3001() {
    let src_param = r#"
struct router {}

fn (r router) new_pipe() (fn(), fn()) {
    m := map[string]int{"k": 0}
    return fn() { m["k"] = 1 }, fn() { m["k"] = 2 }
}

fn worker(f fn()) {
    f()
}

fn run(hy router) {
    a, b := hy.new_pipe()
    task worker(move a)
    task worker(move b)
}

fn main() {}
"#;
    assert_has("HYB-E3001", src_param);

    let src_closure = r#"
struct router {}

fn (r router) pipe() (fn(), fn()) {
    m := map[string]int{"k": 0}
    return fn() { m["k"] = 1 }, fn() { m["k"] = 2 }
}

fn worker(f fn()) {
    f()
}

fn main() {
    f := fn(hy router) {
        a, b := hy.pipe()
        task worker(move a)
        task worker(move b)
    }
    f(router{})
}
"#;
    assert_has("HYB-E3001", src_closure);
}

#[test]
fn struct_hy_associated_fn_pipe_rejected_at_spawn_e3001() {
    let src = r#"
struct hy {}

fn hy.pipe() (fn(), fn()) {
    m := map[string]int{"k": 0}
    return fn() { m["k"] = 1 }, fn() { m["k"] = 2 }
}

fn worker(f fn()) {
    f()
}

fn main() {
    a, b := hy.pipe()
    task worker(move a)
    task worker(move b)
}
"#;
    assert_has("HYB-E3001", src);
}

#[test]
fn multi_value_assign_from_user_pipe_preserves_shapes_and_rejects_e3001() {
    let src = r#"
fn pipe() (fn(), fn()) {
    m := map[string]int{"k": 0}
    return fn() { m["k"] = 1 }, fn() { m["k"] = 2 }
}

fn worker(f fn()) {
    f()
}

fn main() {
    var a fn()
    var b fn()
    a, b = pipe()
    task worker(move a)
    task worker(move b)
}
"#;
    assert_has("HYB-E3001", src);
}

#[test]
fn pointer_loan_through_user_new_pipe_rejected_e3104() {
    let src = r#"
fn new_pipe(p *int) (*int, *int) {
    return p, p
}

fn leak() *int {
    x := 42
    a, b := new_pipe(&x)
    _ = b
    return a
}

fn main() {
    _ = leak()
}
"#;
    assert_has("HYB-E3104", src);
}

#[test]
fn innocent_user_pipe_and_shadowed_hy_pipe_accept_without_spurious_liveness_errors() {
    assert_accept(
        r#"
fn pipe() (int, int) {
    return 1, 2
}

fn new_pipe() (string, int) {
    return "ok", 7
}

fn main() {
    a, b := pipe()
    s, n := new_pipe()
    _ = a + b + n
    _ = s
}
"#,
    );

    assert_accept(
        r#"
struct router {}

fn (r router) pipe() (int, int) {
    return 3, 4
}

fn main() {
    hy := router{}
    a, b := hy.pipe()
    _ = a + b
}
"#,
    );
}

#[test]
fn builtin_hy_pipe_before_later_hy_shadow_retains_capability_tracking() {
    // Before `hy := router{}`, `hy.pipe[int]()` is the built-in linear pipe and
    // must still enforce HYB-E4004 when `tx` is left unclosed; after `hy := router{}`,
    // `hy.pipe()` is a regular method call returning `(int, int)`.
    let reject_src = r#"
struct router {}

fn (r router) pipe() (int, int) {
    return 10, 20
}

fn main() {
    tx, rx := hy.pipe[int]()
    _ = tx
    rx.drain()
    hy := router{}
    a, b := hy.pipe()
    _ = a + b
}
"#;
    assert_has("HYB-E4004", reject_src);

    let accept_src = r#"
struct router {}

fn (r router) pipe() (int, int) {
    return 10, 20
}

fn main() {
    tx, rx := hy.pipe[int]()
    tx.close()
    rx.drain()
    hy := router{}
    a, b := hy.pipe()
    _ = a + b
}
"#;
    assert_accept(accept_src);
}

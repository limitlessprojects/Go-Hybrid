//! mixed_results_slot_indices_2323.rs — regression tests for #2323.
//!
//! Validates that mixed named/unnamed result lists preserve full slot positions
//! so that bare return origin analysis accurately maps multi-value return pairs
//! to their call-site binding positions, rejecting aliased concurrent writes
//! with HYB-E3003.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("repro.hy", src).expect("snippet parses");
    check_file(&ast)
}

#[test]
fn mixed_results_fnlit_bare_return_rejects_e3003() {
    let src = r#"
type dict = map[string]int

fn worker(m dict, done chan<- int) {
    m["k"] = 1
    done <- 1
}

fn main() {
    _, x, y, _ := (fn() (int, a dict, b dict, error) {
        m := map[string]int{"k": 1}
        a = m
        b = m
        return
    })()
    done := make(chan int, 1)
    task worker(move x, done)
    y["k"] = 2
    <-done
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 on mutating aliased map y after moving x, got diagnostics: {:?}",
        r.diagnostics
    );
}

#[test]
fn mixed_results_top_level_fn_bare_return_rejects_e3003() {
    let src = r#"
type dict = map[string]int

fn dup() (int, a dict, b dict, error) {
    m := map[string]int{"k": 1}
    a = m
    b = m
    return
}

fn worker(m dict, done chan<- int) {
    m["k"] = 1
    done <- 1
}

fn main() {
    _, x, y, _ := dup()
    done := make(chan int, 1)
    task worker(move x, done)
    y["k"] = 2
    <-done
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 on top-level fn bare return, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn mixed_results_method_bare_return_rejects_e3003() {
    let src = r#"
type dict = map[string]int

struct service {}

fn (s *service) dup() (int, a dict, b dict, error) {
    m := map[string]int{"k": 1}
    a = m
    b = m
    return
}

fn worker(m dict, done chan<- int) {
    m["k"] = 1
    done <- 1
}

fn main() {
    svc := service{}
    _, x, y, _ := svc.dup()
    done := make(chan int, 1)
    task worker(move x, done)
    y["k"] = 2
    <-done
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 on method bare return, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn mixed_results_try_unwrapped_bare_return_rejects_e3003() {
    let src = r#"
type dict = map[string]int

fn dup() (int, a dict, b dict, error) {
    m := map[string]int{"k": 1}
    a = m
    b = m
    return
}

fn worker(m dict, done chan<- int) {
    m["k"] = 1
    done <- 1
}

fn test() error {
    _, x, y := dup()?
    done := make(chan int, 1)
    task worker(move x, done)
    y["k"] = 2
    <-done
    return nil
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 on try unwrapped call, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn mixed_results_independent_maps_accepted() {
    let src = r#"
type dict = map[string]int

fn separate() (int, a dict, b dict, error) {
    m1 := map[string]int{"k": 1}
    m2 := map[string]int{"k": 2}
    a = m1
    b = m2
    return
}

fn worker(m dict, done chan<- int) {
    m["k"] = 1
    done <- 1
}

fn main() {
    _, x, y, _ := separate()
    done := make(chan int, 1)
    task worker(move x, done)
    y["k"] = 2
    <-done
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "independent maps must stay accepted, got: {:?}", r.diagnostics);
}

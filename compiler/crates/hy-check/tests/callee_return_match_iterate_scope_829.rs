//! callee_return_match_iterate_scope_829.rs — regression tests for issue #829:
//! `Checker::collect_callee_const_return_roots` and `Checker::callee_shared_return_pairs`
//! dropped match/for-in bindings, lexical scopes, conditional rebinds, and copy/out-param stores.
//!
//! Without tracking match bindings and iterate loops, callee return provenance was lost:
//! - File-level `const` returned through match arms or loop iterators could be written to
//!   (bypassing HYB-E3006) or moved to tasks (bypassing HYB-E3004).
//! - Callee return pairs sharing origins across match arms or loop iterators were missed,
//!   bypassing use-after-move (HYB-E3003) and causing data races under `go run -race`.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

// ---------------------------------------------------------------------------
// 1. Match arm binding returning file-level const -> reject mutation (HYB-E3006)
// ---------------------------------------------------------------------------

#[test]
fn match_arm_const_return_rejects_mutation_e3006() {
    let src = r#"
package main

const global_map = map[string]int{"k": 1}

fn pick(opt option[map[string]int]) map[string]int {
    match opt {
        some(m): {
            return m
        }
        none: {
            return map[string]int{}
        }
    }
}

fn main() {
    c := pick(some(global_map))
    c["k"] = 99
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected reject, got: {:?}", r.diagnostics);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 in codes: {:?}",
        codes
    );
}

// ---------------------------------------------------------------------------
// 2. Match arm binding returning file-level const -> reject move (HYB-E3004)
// ---------------------------------------------------------------------------

#[test]
fn match_arm_const_return_rejects_task_move_e3004() {
    let src = r#"
package main

const global_map = map[string]int{"k": 1}

fn pick(opt option[map[string]int]) map[string]int {
    match opt {
        some(m): {
            return m
        }
        none: {
            return map[string]int{}
        }
    }
}

fn worker(c map[string]int, done chan int) {
    _ = c["k"]
    done <- 1
}

fn main() {
    c := pick(some(global_map))
    done := make(chan int, 1)
    task worker(move c, done)
    <-done
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected reject, got: {:?}", r.diagnostics);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E3004"),
        "expected HYB-E3004 in codes: {:?}",
        codes
    );
}

// ---------------------------------------------------------------------------
// 3. For-in loop variable returning file-level const -> reject mutation (HYB-E3006)
// ---------------------------------------------------------------------------

#[test]
fn iterate_loop_const_return_rejects_mutation_e3006() {
    let src = r#"
package main

const global_list = []map[string]int{map[string]int{"k": 1}}

fn first_elem() map[string]int {
    for elem in global_list {
        return elem
    }
    return map[string]int{}
}

fn main() {
    c := first_elem()
    c["k"] = 99
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected reject, got: {:?}", r.diagnostics);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 in codes: {:?}",
        codes
    );
}

// ---------------------------------------------------------------------------
// 4. Match arm shared return pair -> reject use-after-move (HYB-E3003)
// ---------------------------------------------------------------------------

#[test]
fn match_arm_shared_return_rejects_use_after_move_e3003() {
    let src = r#"
package main

fn split_match(opt option[map[string]int]) (map[string]int, map[string]int) {
    match opt {
        some(src): {
            a := src
            b := a
            return a, b
        }
        none: {
            dummy := map[string]int{}
            return dummy, dummy
        }
    }
}

fn worker(m map[string]int, done chan int) {
    _ = m["k"]
    done <- 1
}

fn main() {
    orig := map[string]int{"k": 1}
    a, b := split_match(some(orig))
    done := make(chan int, 1)
    task worker(move a, done)
    b["k"] = 99
    <-done
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected reject, got: {:?}", r.diagnostics);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 in codes: {:?}",
        codes
    );
}

// ---------------------------------------------------------------------------
// 5. For-in loop shared return pair -> reject use-after-move (HYB-E3003)
// ---------------------------------------------------------------------------

#[test]
fn iterate_loop_shared_return_rejects_use_after_move_e3003() {
    let src = r#"
package main

fn loop_pair(src []map[string]int) (map[string]int, map[string]int) {
    for elem in src {
        a := elem
        b := a
        return a, b
    }
    dummy := map[string]int{}
    return dummy, dummy
}

fn worker(m map[string]int, done chan int) {
    _ = m["k"]
    done <- 1
}

fn main() {
    slice := []map[string]int{map[string]int{"k": 1}}
    a, b := loop_pair(slice)
    done := make(chan int, 1)
    task worker(move a, done)
    b["k"] = 99
    <-done
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected reject, got: {:?}", r.diagnostics);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 in codes: {:?}",
        codes
    );
}

// ---------------------------------------------------------------------------
// 6. Negative control: independent values through match arms remain accepted
// ---------------------------------------------------------------------------

#[test]
fn independent_match_returns_accepted() {
    let src = r#"
package main

fn make_distinct(opt option[int]) (map[string]int, map[string]int) {
    match opt {
        some(n): {
            _ = n
            a := map[string]int{"a": 1}
            b := map[string]int{"b": 2}
            return a, b
        }
        none: {
            return map[string]int{}, map[string]int{}
        }
    }
}

fn worker(m map[string]int, done chan int) {
    _ = m["a"]
    done <- 1
}

fn main() {
    a, b := make_distinct(some(1))
    done := make(chan int, 1)
    task worker(move a, done)
    b["b"] = 42
    <-done
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected accept, got diagnostics: {:?}",
        r.diagnostics
    );
}

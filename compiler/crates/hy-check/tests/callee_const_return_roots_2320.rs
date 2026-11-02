//! callee_const_return_roots_2320.rs — regression tests for issue #2320:
//! `Checker::collect_callee_const_return_roots` failed OPEN at `depth > 4` and
//! on fn-value callees, so a file-level `const` map returned through five
//! wrappers (or through `cb := leaf; m := cb()`) reached a `task` and raced:
//! `hyb check --deny-managed` said OK (0 diagnostics) while `go run -race`
//! reported a data race in 10 of 10 runs.
//!
//! Verdict pinned here: the interprocedural walk fails CLOSED (SPEC O4.9, rule
//! FC). Past its depth bound, and for a callee whose body this file cannot see,
//! every may-share file-level `const` is reported as a possible return root
//! instead of nothing — so the existing O5.1/O5.4 (`HYB-E3004`) and T22
//! (`HYB-E3006`) checks fire exactly as they do for the direct call, and a
//! chain deeper than the bound is conservative rather than silent.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

/// `main` walks a chain of `wrappers` functions whose result is `ret_ty`, then
/// moves the result into a task. Used by the shape-gate controls: only a
/// may-share result can hand a file-level `const` back, so a primitive-typed
/// chain must keep the pre-#2320 verdict.
fn typed_chain_program(wrappers: usize, ret_ty: &str, leaf_body: &str) -> String {
    let mut lines = vec![
        "package main".to_string(),
        String::new(),
        "const shared = map[string]int{\"a\": 1}".to_string(),
        String::new(),
        format!("fn leaf() {} {{ return {} }}", ret_ty, leaf_body),
    ];
    for i in 1..=wrappers {
        let callee = if i == 1 {
            "leaf()".to_string()
        } else {
            format!("h{}()", i - 1)
        };
        lines.push(format!("fn h{}() {} {{ return {} }}", i, ret_ty, callee));
    }
    lines.extend([
        String::new(),
        format!("fn worker(x {ret_ty}, done chan int) {{"),
        "    _ = x".to_string(),
        "    done <- 1".to_string(),
        "}".to_string(),
        String::new(),
        "fn main() {".to_string(),
        format!("    x := h{}()", wrappers),
        "    done := make(chan int, 1)".to_string(),
        "    task worker(move x, done)".to_string(),
        "    <-done".to_string(),
        "}".to_string(),
    ]);
    lines.join("\n") + "\n"
}
/// `main` calls `h<wrappers>()` twice — the first result is moved into a task,
/// the second is written to. `leaf` returns `leaf_body`; `h1` wraps `leaf` and
/// each further wrapper wraps the previous one.
fn wrapper_program(wrappers: usize, leaf_body: &str) -> String {
    let mut lines = vec![
        "package main".to_string(),
        String::new(),
        "const shared = map[string]int{\"a\": 1}".to_string(),
        String::new(),
        format!("fn leaf() map[string]int {{ return {} }}", leaf_body),
    ];
    for i in 1..=wrappers {
        let callee = if i == 1 {
            "leaf()".to_string()
        } else {
            format!("h{}()", i - 1)
        };
        lines.push(format!("fn h{}() map[string]int {{ return {} }}", i, callee));
    }
    lines.extend([
        String::new(),
        "fn worker(m map[string]int, done chan int) {".to_string(),
        "    _ = m[\"a\"]".to_string(),
        "    done <- 1".to_string(),
        "}".to_string(),
        String::new(),
        "fn main() {".to_string(),
        format!("    m := h{}()", wrappers),
        "    done := make(chan int, 1)".to_string(),
        "    task worker(move m, done)".to_string(),
        format!("    m2 := h{}()", wrappers),
        "    m2[\"a\"] = 99".to_string(),
        "    <-done".to_string(),
        "}".to_string(),
    ]);
    lines.join("\n") + "\n"
}

// ---------------------------------------------------------------------------
// Rejects — the two exits of the walk (depth bound, unseen callee)
// ---------------------------------------------------------------------------

#[test]
fn five_wrapper_const_return_chain_is_e3004_and_e3006_issue_2320() {
    // The issue's program A: `main` calls `h5()`, which reaches `leaf()` at
    // depth 5 — past the old bound of 4, where the walk returned with no root
    // recorded and both checks stayed silent.
    let src = wrapper_program(5, "shared");
    let r = check(&src);
    let cs = codes(&r);
    assert_eq!(r.verdict, Verdict::Reject, "src:\n{src}\ndiags: {:?}", r.diagnostics);
    assert!(
        cs.iter().any(|c| c == "HYB-E3004"),
        "expected HYB-E3004 for the move of a value returned through five wrappers, got {:?}",
        cs
    );
    assert!(
        cs.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for the element write through the same const, got {:?}",
        cs
    );
}

#[test]
fn const_return_chain_sweep_one_to_eight_wrappers_rejects() {
    // The issue's sweep: 0-4 wrappers already rejected, 5-8 were accepted and
    // every accepted case raced. The whole sweep must reject now.
    for wrappers in 1..=8 {
        let src = wrapper_program(wrappers, "shared");
        let r = check(&src);
        let cs = codes(&r);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "{wrappers} wrappers must reject; src:\n{src}\ndiags: {:?}",
            r.diagnostics
        );
        assert!(
            cs.iter().any(|c| c == "HYB-E3004") && cs.iter().any(|c| c == "HYB-E3006"),
            "{wrappers} wrappers: expected HYB-E3004 and HYB-E3006, got {:?}",
            cs
        );
    }
}

#[test]
fn direct_leaf_call_still_rejects_control() {
    // Control for the sweep: the direct-call shape was already rejected on
    // main and must stay rejected.
    let src = "\
package main

const shared = map[string]int{\"a\": 1}

fn leaf() map[string]int { return shared }

fn worker(m map[string]int, done chan int) {
    _ = m[\"a\"]
    done <- 1
}

fn main() {
    m := leaf()
    done := make(chan int, 1)
    task worker(move m, done)
    m2 := leaf()
    m2[\"a\"] = 99
    <-done
}
";
    let r = check(src);
    let cs = codes(&r);
    assert_eq!(r.verdict, Verdict::Reject, "diags: {:?}", r.diagnostics);
    assert!(
        cs.iter().any(|c| c == "HYB-E3004") && cs.iter().any(|c| c == "HYB-E3006"),
        "{:?}",
        cs
    );
}

#[test]
fn fn_value_callee_const_return_is_e3004_and_e3006_issue_2320() {
    // The issue's program B: `cb := leaf` — `resolve_call_expr_decl_or_fnlit`
    // returns None for a local binding, and the old code returned with no root
    // recorded. With `m := leaf()` instead of `m := cb()` the same program is
    // rejected, which is exactly the asymmetry the issue reports.
    let src = "\
package main

const shared = map[string]int{\"a\": 1}

fn leaf() map[string]int { return shared }

fn worker(m map[string]int, done chan int) {
    _ = m[\"a\"]
    done <- 1
}

fn main() {
    cb := leaf
    m := cb()
    done := make(chan int, 1)
    task worker(move m, done)
    m2 := cb()
    m2[\"a\"] = 99
    <-done
}
";
    let r = check(src);
    let cs = codes(&r);
    assert_eq!(r.verdict, Verdict::Reject, "diags: {:?}", r.diagnostics);
    assert!(
        cs.iter().any(|c| c == "HYB-E3004"),
        "expected HYB-E3004 for the move of a const returned through a fn value, got {:?}",
        cs
    );
    assert!(
        cs.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for the element write through the same const, got {:?}",
        cs
    );
}

#[test]
fn repeated_callee_visit_keeps_the_const_origin() {
    // Two calls to the same callee in one body: the second visit replays the
    // roots the first one collected instead of tracking it again. `return b`
    // must still carry the const, otherwise the walk's visited-set would have
    // turned the fix into a new false negative.
    let src = "\
package main

const shared = map[string]int{\"a\": 1}

fn q() map[string]int { return shared }

fn w() map[string]int {
    a := q()
    b := q()
    _ = a
    return b
}

fn worker(m map[string]int, done chan int) {
    _ = m[\"a\"]
    done <- 1
}

fn main() {
    m := w()
    done := make(chan int, 1)
    task worker(move m, done)
    m2 := w()
    m2[\"a\"] = 99
    <-done
}
";
    let r = check(src);
    let cs = codes(&r);
    assert_eq!(r.verdict, Verdict::Reject, "diags: {:?}", r.diagnostics);
    assert!(
        cs.iter().any(|c| c == "HYB-E3004") && cs.iter().any(|c| c == "HYB-E3006"),
        "{:?}",
        cs
    );
}

#[test]
fn const_return_walk_past_its_depth_bound_rejects() {
    // 40 wrappers: deeper than the walk bound. Past the bound the walk fails
    // closed, so the chain is rejected by the fail-closed path instead of
    // being silently assumed fresh.
    let src = wrapper_program(40, "shared");
    let r = check(&src);
    assert_eq!(r.verdict, Verdict::Reject, "diags: {:?}", r.diagnostics);
    assert!(codes(&r).iter().any(|c| c == "HYB-E3004"), "{:?}", codes(&r));
}



// ---------------------------------------------------------------------------
// Controls — fail closed, not fail everything
// ---------------------------------------------------------------------------

#[test]
fn fresh_map_through_the_same_five_wrapper_chain_stays_accepted() {
    // The accepted twin of the issue's program A: no file-level `const` is in
    // play, so the walk collects nothing and the program is unchanged.
    let src = wrapper_program(5, "map[string]int{\"a\": 1}");
    let r = check(&src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "src:\n{src}\ndiags: {:?}",
        r.diagnostics
    );
}

#[test]
fn builtin_and_package_callees_are_not_fail_closed() {
    // The fail-closed callee set is fn values and computed callees — not every
    // callee the checker cannot resolve. `make`/`append`/a package call return
    // fresh values, and `const`s are file-level, so a call into another file
    // cannot hand one of ours back: the fresh map here stays movable.
    let src = "\
package main

import \"strings\"

const shared = map[string]int{\"a\": 1}

fn worker(m map[string]int, done chan int) {
    _ = m[\"a\"]
    done <- 1
}

fn main() {
    buf := strings.builder()
    buf.write_string(\"x\")
    m := map[string]int{\"a\": 1}
    done := make(chan int, 1)
    task worker(move m, done)
    words := make([]string, 0, 4)
    words = append(words, buf.string())
    _ = words
    _ = shared
    <-done
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "diags: {:?}", r.diagnostics);
}

#[test]
fn const_return_walk_past_the_bound_is_conservative_not_silent() {
    // Documents the price of O4.9 past the bound: this chain is 40 wrappers
    // deep and returns a FRESH map, so the same code with a shallower chain is
    // accepted — but past the bound the walk cannot see the leaf, so it
    // reports every may-share `const` and the move is rejected. Conservative
    // by design: silence here is what issue #2320 was.
    let src = wrapper_program(40, "map[string]int{\"a\": 1}");
    let r = check(&src);
    assert_eq!(r.verdict, Verdict::Reject, "diags: {:?}", r.diagnostics);

    let shallow = check(&wrapper_program(5, "map[string]int{\"a\": 1}"));
    assert_eq!(
        shallow.verdict,
        Verdict::Accept,
        "diags: {:?}",
        shallow.diagnostics
    );
}


// ---------------------------------------------------------------------------
// Shape gate — the fail-closed report is about a share, not about the name
// ---------------------------------------------------------------------------

#[test]
fn deep_chain_returning_a_primitive_keeps_the_pre_fix_verdict() {
    // 40 wrappers deep — past the walk bound — but the result type is `int`,
    // which cannot alias a `const` map, so the walk reports nothing and the
    // program keeps the verdict it had before #2320 (control for the gate).
    let src = typed_chain_program(40, "int", "1");
    let r = check(&src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "src:\n{src}\ndiags: {:?}",
        r.diagnostics
    );
}

#[test]
fn fn_value_returning_a_primitive_keeps_the_pre_fix_verdict() {
    // The same gate on the unresolved-callee exit: `cb := num` hides the body,
    // but `num` is declared `int`, so no file-level `const` can come back
    // through the call and the move stays legal.
    let src = "\
package main

const shared = map[string]int{\"a\": 1}

fn num() int { return 1 }

fn worker(n int, done chan int) {
    _ = n
    done <- 1
}

fn main() {
    cb := num
    n := cb()
    done := make(chan int, 1)
    task worker(move n, done)
    <-done
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "diags: {:?}", r.diagnostics);
}

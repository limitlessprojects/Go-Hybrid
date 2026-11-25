//! loop_label_scope.rs — HYB-E3010: `break loop` / `continue loop` with no
//! enclosing loop (issue: loop-label scope never validated).
//!
//! SPEC-LANGUAGE LX14: the `loop` label binds to the nearest enclosing
//! `for`/`loop`. Outside any loop the label has no target — the emitter
//! lowered it to `break hyLoop` / `continue hyLoop` against an undefined Go
//! label, so accepted programs failed `go vet`/`go build` with
//! `invalid break label hyLoop` (the same accepted-should-emit-buildable-Go
//! contract break as #77/#89/#94). The label also does not cross select or
//! match: a select/match arm inside a loop still targets that loop.
//!
//! Controls: labeled break/continue inside a real loop (plain, if-nested,
//! for-range, match-arm) must stay accepted — the emitter labels the owning
//! loop (`hyLoop:`), which is separately pinned in hy-emit's tests.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn first_code(r: &hy_check::CheckResult) -> Option<String> {
    r.diagnostics
        .iter()
        .find(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code.clone())
}

#[test]
fn top_level_break_loop_rejects_e3010() {
    let r = check("fn main() {\n    break loop\n}\n");
    assert_eq!(first_code(&r), Some("HYB-E3010".to_string()));
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn top_level_continue_loop_rejects_e3010() {
    let r = check("fn main() {\n    continue loop\n}\n");
    assert_eq!(first_code(&r), Some("HYB-E3010".to_string()));
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn break_loop_in_select_outside_loop_rejects_e3010() {
    // The select is NOT a loop: `break loop` inside a select arm at fn top
    // level still has no target (LX14 binds to for/loop, not select).
    let src = "\
fn main() {
    ch := make(chan int)
    select {
        <-ch:
            break loop
    }
}
";
    let r = check(src);
    assert_eq!(first_code(&r), Some("HYB-E3010".to_string()));
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn break_loop_in_nested_block_outside_loop_rejects_e3010() {
    let src = "\
fn main() {
    {
        break loop
    }
}
";
    let r = check(src);
    assert_eq!(first_code(&r), Some("HYB-E3010".to_string()));
}

#[test]
fn break_loop_inside_loop_accepted() {
    let r = check("fn main() {\n    loop {\n        break loop\n    }\n}\n");
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(first_code(&r).is_none(), "no diagnostics: {:?}", r.diagnostics);
}

#[test]
fn break_loop_in_if_inside_while_for_outside_loop_rejects_e3010() {
    // `for cond { }` is a ForStmt, not a LoopStmt; break loop outside any `loop` statement has no target (LX14).
    let src = "\
fn main() {
    x := 1
    for x > 0 {
        if x > 0 {
            break loop
        }
    }
}
";
    let r = check(src);
    assert_eq!(first_code(&r), Some("HYB-E3010".to_string()));
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn continue_loop_inside_for_range_outside_loop_rejects_e3010() {
    let src = "\
fn main() {
    for v in 0..3 {
        continue loop
    }
}
";
    let r = check(src);
    assert_eq!(first_code(&r), Some("HYB-E3010".to_string()));
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn break_loop_and_continue_loop_in_for_inside_loop_accepted() {
    // Inside an enclosing `loop`, break loop and continue loop inside an inner `for` target the enclosing `loop`.
    let src = "\
fn main() {
    loop {
        for v in 0..3 {
            if v == 1 {
                break loop
            }
            continue loop
        }
    }
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(first_code(&r).is_none(), "no diagnostics: {:?}", r.diagnostics);
}

#[test]
fn break_loop_in_match_arm_inside_loop_accepted() {
    // The match arm is not a new loop: the labeled break still targets the
    // enclosing loop. Checker accepts; the emitter must label THAT loop
    // (stmt_has_loop_break now descends into match arms — pinned in
    // hy-emit tests).
    let src = "\
fn main() {
    o := some(1)
    loop {
        match o {
            some(v) => if v > 0 { break loop }
            none => {}
        }
    }
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn break_loop_in_select_inside_loop_accepted() {
    // Example 04's shape: break the LOOP from inside a select arm.
    let src = "\
fn main() {
    sq := make(chan int)
    total := 0
    loop {
        select {
            v, ok := <-sq:
                if !ok { break loop }
                total += v
        }
    }
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn plain_break_outside_loop_unchanged() {
    // Plain `break` (no label) is a parse-level concern outside this check's
    // scope — HYB-E3010 covers the LABELED form only. If the parser accepts
    // plain break here it must not gain an E3010.
    let r = check("fn main() {\n    break\n}\n");
    assert!(
        first_code(&r).as_deref() != Some("HYB-E3010"),
        "E3010 must not fire on unlabeled break: {:?}",
        r.diagnostics
    );
}

#[test]
fn closure_break_loop_without_enclosing_loop_rejects_e3010() {
    let src = "fn main() {\n    f := fn() {\n        break loop\n    }\n    f()\n}\n";
    let r = check(src);
    assert_eq!(first_code(&r), Some("HYB-E3010".to_string()));
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn closure_continue_loop_inside_outer_loop_rejects_e3010() {
    let src = "fn main() {\n    loop {\n        f := fn() {\n            continue loop\n        }\n        f()\n        break\n    }\n}\n";
    let r = check(src);
    assert_eq!(first_code(&r), Some("HYB-E3010".to_string()));
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn closure_for_range_break_loop_without_closure_loop_rejects_e3010() {
    let src = "fn main() {\n    loop {\n        f := fn() {\n            for i in 0..3 {\n                if i == 1 {\n                    break loop\n                }\n            }\n        }\n        f()\n        break\n    }\n}\n";
    let r = check(src);
    assert_eq!(first_code(&r), Some("HYB-E3010".to_string()));
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn closure_break_loop_inside_closure_own_loop_accepted() {
    let src = "fn main() {\n    f := fn() {\n        loop {\n            break loop\n        }\n    }\n    f()\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(first_code(&r).is_none(), "no diagnostics: {:?}", r.diagnostics);
}

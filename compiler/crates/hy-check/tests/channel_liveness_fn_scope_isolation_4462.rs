use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src).unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn codes(res: &hy_check::CheckResult) -> Vec<String> {
    res.diagnostics.iter().map(|d| d.code.to_string()).collect()
}

// ---------------------------------------------------------------------------
// issue #4462 — capability identity must not leak between function bodies.
//
// `ChannelLivenessChecker` keys capabilities by the bare name (`cap_to_pipe`)
// and `pop_scope` promotes a body's `moved_caps` into the file-level scope, so
// a `move tx` in one function made every later function's own `tx` look moved.
// The trigger is declaration order: helper-before-main is fine, main-before-
// helper is rejected — and `tx` is the name in the checker's own fix-it text.
// ---------------------------------------------------------------------------

const SUB: &str = r#"
fn push_one(tx hy.send_cap[int]) {
    tx.send(1)
    tx.close()
}
"#;

#[test]
fn later_fn_reusing_the_moved_name_is_accepted() {
    // main FIRST: its `move tx` is walked before the helper, which declares its
    // own `tx` parameter.
    let src = r#"
fn main() {
    tx, rx := hy.pipe[int]()
    task push_one(move tx)
    for v in rx {
        fmt.println("{v}")
    }
}

fn push_one(tx hy.send_cap[int]) {
    tx.send(1)
    tx.close()
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Accept, "{:?}", res.diagnostics);
    assert_eq!(res.diagnostics.len(), 0);
}

#[test]
fn renamed_parameter_is_the_control() {
    // The same program with a distinct parameter name is accepted even before
    // the fix — proof that the trigger is the shared name, not the shape.
    let src = r#"
fn main() {
    tx, rx := hy.pipe[int]()
    task push_one(move tx)
    for v in rx {
        fmt.println("{v}")
    }
}

fn push_one(out hy.send_cap[int]) {
    out.send(1)
    out.close()
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Accept, "{:?}", res.diagnostics);
}

#[test]
fn closure_move_still_poisons_the_enclosing_body() {
    // The scope isolation must not weaken the promotion that closures rely on:
    // a move performed inside a closure body is still visible after it.
    let src = r#"
fn sub(tx hy.send_cap[int]) {
    tx.close()
}

fn main() {
    tx, rx := hy.pipe[int]()
    task sub(move tx)
    tx.send(1)
    rx.drain()
}
"#;
    let res = check(src);
    // `move` at the spawn shadows nothing here: the later use is use-after-move.
    assert!(
        codes(&res).iter().any(|c| c == "HYB-E3003" || c == "HYB-E3001"),
        "{:?}",
        res.diagnostics
    );
}

#[test]
fn unclosed_second_fn_is_still_rejected_e4004() {
    // Guard against over-relaxing: the second function genuinely never closes
    // its own `tx`, so E4004 must survive the isolation.
    let src = r#"
fn main() {
    tx, rx := hy.pipe[int]()
    task sub(move tx)
    rx.drain()
}

fn sub(tx hy.send_cap[int]) {
    tx.close()
}

fn forgetful() {
    tx, rx := hy.pipe[int]()
    tx.send(1)
    rx.drain()
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject, "{:?}", res.diagnostics);
    assert!(codes(&res).iter().any(|c| c == "HYB-E4004"), "{:?}", res.diagnostics);
}

#[test]
fn closed_second_fn_is_not_flagged_by_the_first_function() {
    // Two functions, each with its own pipe, the second correct: only the
    // promoted state used to flag it.
    let src = r#"
fn main() {
    tx, rx := hy.pipe[int]()
    task sub(move tx)
    rx.drain()
}

fn sub(tx hy.send_cap[int]) {
    tx.close()
}

fn orderly() {
    tx, rx := hy.pipe[int]()
    tx.send(1)
    tx.close()
    rx.drain()
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Accept, "{:?}", res.diagnostics);
}

#[test]
fn unused_sub_helper_is_covered_by_the_constant() {
    // `SUB` exists so the shape stays visible next to the tests that inline it.
    let res = check(&format!("\nfn main() {{\n    tx, rx := hy.pipe[int]()\n    task push_one(move tx)\n    rx.drain()\n}}\n{SUB}"));
    assert_eq!(res.verdict, Verdict::Accept, "{:?}", res.diagnostics);
}

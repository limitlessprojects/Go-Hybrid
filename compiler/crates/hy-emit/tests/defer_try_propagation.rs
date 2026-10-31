//! Issue #2384: `defer f()?` (the HYB-E2001 documented legal shape) passed
//! `hyb check` but `hy-emit` died with "`?` outside a binding position".
//! P1 + E3: the deferred call runs at exit, so its `?` propagates at exit —
//! into the enclosing fn's (named) error result, first error wins. Where no
//! error result exists the emitter refuses loudly (E3b: never a silent
//! swallow).

fn emit(src: &str) -> String {
    let file = hy_syntax::parse_file("t.hy", src).expect("parse");
    hy_emit::emit_go(&file).unwrap_or_else(|e| panic!("emit failed: {}", e.message))
}

fn emit_err(src: &str) -> String {
    let file = hy_syntax::parse_file("t.hy", src).expect("parse");
    match hy_emit::emit_go(&file) {
        Ok(g) => panic!("expected an emit refusal, got:\n{}", g),
        Err(e) => e.message,
    }
}

const CLOSE_IT: &str = "fn close_it() error {\n    return nil\n}\n\n";

#[test]
fn issue_2384_repro_emits_exit_time_propagation() {
    let go = emit(&format!(
        "{CLOSE_IT}fn run() error {{\n    defer close_it()?\n    return nil\n}}\n\nfn main() {{\n    _ = run()\n}}\n"
    ));
    assert!(go.contains("func run() (hyErr error) {"), "{go}");
    assert!(go.contains("defer func() {"), "{go}");
    assert!(
        go.contains("if hyErr0 := close_it(); hyErr0 != nil && hyErr == nil {"),
        "{go}"
    );
    assert!(go.contains("hyErr = hyErr0"), "{go}");
}

#[test]
fn every_paren_placement_lowers_the_same_way() {
    for operand in ["(close_it())?", "(close_it()?)", "((close_it()))?"] {
        let go = emit(&format!(
            "{CLOSE_IT}fn run() error {{\n    defer {operand}\n    return nil\n}}\n\nfn main() {{\n    _ = run()\n}}\n"
        ));
        assert!(
            go.contains("if hyErr0 := close_it(); hyErr0 != nil && hyErr == nil {"),
            "`defer {operand}`:\n{go}"
        );
    }
}

#[test]
fn value_and_error_results_are_all_named_and_user_names_kept() {
    let go = emit(&format!(
        "{CLOSE_IT}fn count() (int, error) {{\n    defer close_it()?\n    return 1, nil\n}}\n\nfn named() (n int, err error) {{\n    defer close_it()?\n    return 2, nil\n}}\n\nfn main() {{\n    a, _ := count()\n    b, _ := named()\n    fmt.println(a + b)\n}}\n"
    ));
    assert!(
        go.contains("func count() (hyR0 int, hyErr error) {"),
        "{go}"
    );
    assert!(go.contains("func named() (n int, err error) {"), "{go}");
    assert!(go.contains("!= nil && err == nil {"), "{go}");
}

#[test]
fn local_arguments_are_evaluated_at_defer_time() {
    // Go evaluates a deferred call's arguments at the defer statement; the
    // closure lowering must not read `n` again at exit.
    let go = emit(
        "fn fail_if(n int) error {\n    return nil\n}\n\nfn run() error {\n    n := 1\n    defer fail_if(n)?\n    n = 0\n    return nil\n}\n\nfn main() {\n    _ = run()\n}\n",
    );
    let snap = go.find("hyDA0 := n").unwrap_or_else(|| panic!("{go}"));
    let defer_at = go.find("defer func()").unwrap_or_else(|| panic!("{go}"));
    assert!(snap < defer_at, "{go}");
    assert!(go.contains("fail_if(hyDA0)"), "{go}");
}

#[test]
fn constant_arguments_stay_inline() {
    let go = emit(
        "fn fail_if(n int) error {\n    return nil\n}\n\nfn run() error {\n    defer fail_if(1)?\n    return nil\n}\n\nfn main() {\n    _ = run()\n}\n",
    );
    assert!(!go.contains("hyDA"), "{go}");
    assert!(go.contains(":= fail_if(1);"), "{go}");
}

#[test]
fn main_wrapper_propagates_into_hyerr() {
    let go = emit(&format!(
        "{CLOSE_IT}fn main() {{\n    defer close_it()?\n    fmt.println(\"body\")\n}}\n"
    ));
    assert!(go.contains("hyErr := func() (hyErr error) {"), "{go}");
    assert!(
        go.contains("if hyErr0 := close_it(); hyErr0 != nil && hyErr == nil {"),
        "{go}"
    );
}

#[test]
fn no_error_result_refuses_instead_of_dropping_the_error() {
    let msg = emit_err(&format!(
        "{CLOSE_IT}fn noerr() {{\n    defer close_it()?\n}}\n\nfn main() {{\n    noerr()\n}}\n"
    ));
    assert!(msg.contains("silently dropped"), "{msg}");
}

#[test]
fn closure_body_refuses_instead_of_writing_the_outer_result() {
    // E4: `?` in a closure binds to the closure's own results, never the
    // enclosing fn's — the outer hyErr must not be the landing spot.
    let msg = emit_err(&format!(
        "{CLOSE_IT}fn main() {{\n    f := fn() error {{\n        defer close_it()?\n        return nil\n    }}\n    _ = f()\n}}\n"
    ));
    assert!(msg.contains("silently dropped"), "{msg}");
}

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

/// A lone multi-value call is the whole argument list when the callee takes
/// exactly that many parameters: `w(pair())` builds for a 2-param `w`. It is
/// one of Go's few exceptions to "arguments are single values", and a spawn
/// is no different from a call here — `go w(pair())` builds under the same
/// rule.
///
/// The check therefore has to hold two directions at once:
///   - a callee that cannot take the expanded values is rejected, once, with
///     no second opinion from another rule;
///   - a callee that takes exactly them is accepted, not rejected as a
///     "single-value context" violation.
fn check(src: &str, file: &str) -> (Verdict, Vec<String>) {
    let r = check_file(&parse_file(file, src).expect("parse"));
    (r.verdict, r.diagnostics.iter().map(|d| d.code.clone()).collect())
}

#[test]
fn forwards_a_multi_value_call_into_a_matching_parameter_list() {
    // Legal in Go for both a call and a spawn, so the checker must accept it.
    // Before this change the spawn form was rejected: the single-value rule
    // that #887 added for spawn arguments did not carry the #682 U7
    // exception that the call path already had.
    let (verdict, codes) = check(
        "fn pair() (int, int) { return 1, 2 }\n\
         fn w(a int, b int) { _ = a\n _ = b }\n\
         fn main() { task w(pair())\n (fn(a int, b int) { _ = a\n _ = b })(pair()) }",
        "ok.hy",
    );
    assert_eq!(verdict, Verdict::Accept, "unexpected diagnostics: {codes:?}");
    assert!(codes.is_empty(), "expected no diagnostics, got {codes:?}");
}

#[test]
fn rejects_mismatched_multi_value_forwarding_in_task_some_set_fnlit_and_fnfield() {
    // A named user function is owned by the arity rule and reports E1002;
    // every other callee shape — a builtin, a method call, a function
    // literal, a `fn`-typed field — is owned by the binding rule and reports
    // E2002. One diagnostic each: a mismatch must not be reported twice.
    for (src, expected) in [
        ("fn pair() (int, int) { return 1, 2 }\nfn w(a int) { _ = a }\nfn main() { task w(pair()) }", "HYB-E1002"),
        ("fn pair() (int, int) { return 1, 2 }\nfn w(a int) { _ = a }\nfn main() { w(pair()) }", "HYB-E1002"),
        ("fn pair() (int, int) { return 1, 2 }\nfn main() { x := some(pair())\n _ = x }", "HYB-E2002"),
        ("fn pair() (int, int) { return 1, 2 }\nfn main() { var s set[int]\n s.insert(pair())\n _ = s }", "HYB-E2002"),
        ("fn pair() (int, int) { return 1, 2 }\nfn main() { (fn(a int) { _ = a })(pair()) }", "HYB-E2002"),
        ("struct h { cb fn(a int) }\nfn pair() (int, int) { return 1, 2 }\nfn nop(a int) { _ = a }\nfn main() { x := h{cb: nop}\n x.cb(pair()) }", "HYB-E2002"),
    ] {
        let (verdict, codes) = check(src, "test.hy");
        assert_eq!(verdict, Verdict::Reject, "expected Reject for {src:?}");
        assert_eq!(codes, vec![expected.to_string()], "for {src:?}");
    }
}

#[test]
fn still_rejects_a_multi_value_call_that_is_not_the_whole_argument_list() {
    // The exception is narrow: it covers a lone multi-value call and nothing
    // else. Wrapped in an expression, or alongside another argument, the
    // value still sits in a single-value position and does not build.
    for src in [
        // alongside another argument, the special case does not apply
        "fn pair() (int, int) { return 1, 2 }\nfn w(a int, b int) { _ = a\n _ = b }\nfn main() { task w(pair(), 3) }",
        // second position is a single-value context like the first
        "fn pair() (int, int) { return 1, 2 }\nfn w(a int, b int) { _ = a\n _ = b }\nfn main() { task w(1, pair()) }",
        // wrapped in an expression it is no longer the argument itself
        "fn pair() (int, int) { return 1, 2 }\nfn w(a int, b int) { _ = a\n _ = b }\nfn main() { task w(1 + 1)\n _ = pair() }",
    ] {
        let (verdict, codes) = check(src, "test.hy");
        assert!(!codes.is_empty(), "expected a diagnostic for {src:?}");
        let _ = verdict;
    }
}

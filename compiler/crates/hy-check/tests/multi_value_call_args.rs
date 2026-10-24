use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    check_file(&parse_file("inline.hy", src).expect("must parse"))
}

fn assert_has(src: &str, code: &str) {
    let r = check(src);
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error && d.code == code),
        "expected {code}, got {:?}",
        r.diagnostics
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn multi_value_call_args_reject_type_and_arity_mismatches() {
    for src in [
        "package main\nfn takes_ints(a int, b int) {}\nfn returns_strings() (string, string) { return \"a\", \"b\" }\nfn main() {\n    takes_ints(returns_strings())\n}\n",
        "package main\nfn takes_ints(a int, b int) {}\nfn returns_strings() (string, string) { return \"a\", \"b\" }\nfn main() {\n    task takes_ints(returns_strings())\n}\n",
        "package main\nstruct st {}\nfn (s st) takes_ints(a int, b int) {}\nfn returns_strings() (string, string) { return \"a\", \"b\" }\nfn main() {\n    var s st\n    s.takes_ints(returns_strings())\n}\n",
        "package main\nstruct st {}\nfn (s st) takes_one(a int) {}\nfn two() (int, int) { return 1, 2 }\nfn main() {\n    var s st\n    s.takes_one(two())\n}\n",
        "package main\nfn two() (int, int) { return 1, 2 }\nfn main() {\n    cb := fn(a int) {}\n    cb(two())\n}\n",
    ] {
        assert_has(src, "HYB-E1002");
    }
}

#[test]
fn multi_value_call_args_accept_matching_method_and_closure_calls() {
    for src in [
        "package main\nstruct st {}\nfn (s st) takes_two(a int, b int) {}\nfn two() (int, int) { return 1, 2 }\nfn main() {\n    var s st\n    s.takes_two(two())\n}\n",
        "package main\nfn two() (int, int) { return 1, 2 }\nfn main() {\n    cb := fn(a int, b int) { _ = a + b }\n    cb(two())\n}\n",
    ] {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "expected accept for:\n{src}\ndiags: {:?}",
            r.diagnostics
        );
    }
}

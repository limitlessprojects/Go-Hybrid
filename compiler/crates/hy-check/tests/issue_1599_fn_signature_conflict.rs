//! issue_1599_fn_signature_conflict.rs — regression tests for issue #1599:
//! function types are never compared by signature: fn values with different
//! parameter/result types assign, pass and return freely, fail go build.

use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "test_1599.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn mismatched_fn_signatures_rejected_e1002() {
    let cases = [
        // 1. Param type mismatch: fn(int) int initialized with fn(string) int
        "fn g(s string) int { return len(s) }\nfn main() {\n    var f fn(int) int = g\n    fmt.println(f)\n}\n",
        // 2. Fn literal param type mismatch: fn(int) initialized with fn(string)
        "fn main() {\n    var f fn(int) = fn(s string) {}\n    fmt.println(f)\n}\n",
        // 3. Return type mismatch: fn() int initialized with fn() string
        "fn main() {\n    var f fn() int = fn() string { return \"hi\" }\n    fmt.println(f)\n}\n",
        // 4. Return type mismatch: fn(int) int initialized with fn(int) f64
        "fn g(n int) f64 { return 1.0 }\nfn main() {\n    var f fn(int) int = g\n    fmt.println(f)\n}\n",
        // 5. Void vs int return: fn(int) int initialized with fn(int)
        "fn g(n int) {}\nfn main() {\n    var f fn(int) int = g\n    fmt.println(f)\n}\n",
        // 6. Struct literal field mismatch
        "struct s_holder {\n    cb fn(int) int\n}\nfn g(s string) int { return len(s) }\nfn main() {\n    s := s_holder{cb: g}\n    fmt.println(s)\n}\n",
        // 7. Return statement mismatch
        "fn g(s string) int { return len(s) }\nfn mk() fn(int) int {\n    return g\n}\nfn main() {}\n",
        // 8. Assignment mismatch
        "fn g(s string) int { return len(s) }\nfn main() {\n    var f fn(int) int\n    f = g\n    fmt.println(f)\n}\n",
    ];

    for src in cases {
        let r = check(src);
        let cs = error_codes(&r);
        assert!(
            cs.iter().any(|c| c == "HYB-E1002"),
            "expected HYB-E1002 for mismatched fn signature:\n{src}\ngot diagnostics: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn matching_fn_signatures_accepted() {
    let cases = [
        "fn g(x int) int { return x + 1 }\nfn main() {\n    var f fn(int) int = g\n    fmt.println(f(1))\n}\n",
        "fn main() {\n    var f fn(int) int = fn(n int) int { return n * 2 }\n    fmt.println(f(2))\n}\n",
        "fn g() string { return \"ok\" }\nfn mk() fn() string {\n    return g\n}\nfn main() {}\n",
    ];

    for src in cases {
        let r = check(src);
        let cs = error_codes(&r);
        assert!(
            !cs.iter().any(|c| c == "HYB-E1002"),
            "matching fn signature must NOT fire HYB-E1002:\n{src}\ngot diagnostics: {:?}",
            r.diagnostics
        );
    }
}

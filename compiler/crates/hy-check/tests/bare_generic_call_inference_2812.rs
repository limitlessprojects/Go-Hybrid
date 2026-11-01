//! bare_generic_call_inference_2812.rs — issue #2812 (Row 13 BUILDABLE).
//!
//! Go infers a call's type arguments from its ARGUMENT types only — never from
//! the result type or the assignment target. `check_user_fn_arity` returned
//! early for every generic callee, so a bare call whose type parameter occurs
//! in no parameter type (`zero()`, `mk()`, `conv(1)`) passed `hyb check` and
//! failed `go build` (`in call to zero, cannot infer t`). Explicit
//! instantiation (`zero[int]()`) builds, and so does a callee whose parameter
//! types mention the parameter.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn bare_generic_calls_with_no_inferable_type_argument_reject() {
    // The issue's own reproducer: three partial-only type parameters.
    let r = check(
        "fn zero[t any]() t {\n    var z t\n    return z\n}\nfn mk[t any]() []t {\n    return []t{}\n}\nfn conv[a any, b any](x a) b {\n    var z b\n    return z\n}\nfn main() {\n    var n int = zero()\n    xs := mk()\n    _ = n\n    _ = xs\n    _ = conv(1)\n}\n",
    );
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().filter(|c| *c == "HYB-E1002").count() >= 3,
        "expected at least three HYB-E1002, got {codes:?}"
    );

    for (name, src) in [
        (
            "zero() in a println argument",
            "fn zero[t any]() t {\n    var z t\n    return z\n}\nfn main() {\n    fmt.println(zero())\n}\n",
        ),
        (
            "target-typed var",
            "fn zero[t any]() t {\n    var z t\n    return z\n}\nfn main() {\n    var n int = zero()\n    _ = n\n}\n",
        ),
        (
            "slice-of-param result",
            "fn mk[t any]() []t {\n    return []t{}\n}\nfn main() {\n    xs := mk()\n    _ = xs\n}\n",
        ),
        (
            "only the second parameter is uninferable",
            "fn conv[a any, b any](x a) b {\n    var z b\n    return z\n}\nfn main() {\n    _ = conv(1)\n}\n",
        ),
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "{name}");
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "{name}: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn inferable_and_explicitly_instantiated_calls_stay_accepted() {
    for (name, src) in [
        (
            "identity keeps its parameter",
            "fn id[t any](x t) t {\n    return x\n}\nfn main() {\n    _ = id(1)\n}\n",
        ),
        (
            "parameter mentioned inside a composite type",
            "fn wrap[t any](x t) []t {\n    return []t{x}\n}\nfn main() {\n    _ = wrap(1)\n}\n",
        ),
        (
            "parameter mentioned as a slice element",
            "fn first[t any](xs []t) t {\n    return xs[0]\n}\nfn main() {\n    _ = first([]int{1})\n}\n",
        ),
        (
            "explicit instantiation of the uninferable form",
            "fn zero[t any]() t {\n    var z t\n    return z\n}\nfn main() {\n    _ = zero[int]()\n}\n",
        ),
        (
            "explicit instantiation with two arguments",
            "fn conv[a any, b any](x a) b {\n    var z b\n    return z\n}\nfn main() {\n    _ = conv[int, string](1)\n}\n",
        ),
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Accept, "{name}: {:?}", r.diagnostics);
        assert!(error_codes(&r).is_empty(), "{name}: {:?}", r.diagnostics);
    }
}

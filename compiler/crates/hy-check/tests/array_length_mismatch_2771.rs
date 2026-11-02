//! array_length_mismatch_2771.rs — issue #2771 (Row 13 BUILDABLE).
//!
//! `OpKind::Array(Box<OpKind>)` keeps only the element kind, so `[3]int` and
//! `[4]int` looked identical: comparing, assigning or passing them across
//! passed `hyb check` and failed `go build` (`mismatched types [3]int and
//! [4]int`). Slices and same-length arrays stay accepted.

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
fn different_array_lengths_reject() {
    for (name, src) in [
        (
            "[3]int == [4]int",
            "fn main() {\n    var a [3]int\n    var b [4]int\n    _ = a == b\n}\n",
        ),
        (
            "[4]int = [3]int",
            "fn main() {\n    var a [3]int\n    var b [4]int\n    b = a\n    _ = b\n}\n",
        ),
        (
            "var c [4]int = a ([3]int)",
            "fn main() {\n    var a [3]int\n    var c [4]int = a\n    _ = c\n}\n",
        ),
        (
            "take(a) with take(x [4]int)",
            "fn take(x [4]int) int {\n    return x[0]\n}\nfn main() {\n    var a [3]int\n    _ = take(a)\n}\n",
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
fn same_length_arrays_and_slices_stay_accepted() {
    for (name, src) in [
        (
            "[3]int == [3]int",
            "fn main() {\n    var a [3]int\n    var b [3]int\n    _ = a == b\n}\n",
        ),
        (
            "[3]int = [3]int",
            "fn main() {\n    var a [3]int\n    var b [3]int\n    b = a\n    _ = b\n}\n",
        ),
        (
            "slices of any length",
            "fn main() {\n    var a []int = [1, 2]\n    var b []int = [1, 2, 3]\n    a = b\n    _ = a\n}\n",
        ),
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Accept, "{name}: {:?}", r.diagnostics);
        assert!(error_codes(&r).is_empty(), "{name}: {:?}", r.diagnostics);
    }
}

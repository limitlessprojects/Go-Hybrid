//! issue_2771_array_length_mismatch.rs — issue #2771 (Row 13 BUILDABLE).
//!
//! Fixed-size array lengths in Go are part of the type. Comparing, assigning,
//! declaring, or passing arrays of the same element type but different lengths
//! must be rejected with HYB-E1002. Same-length arrays and dynamic slices stay accepted.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
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
fn different_array_lengths_rejected() {
    for (name, src) in [
        (
            "[3]int == [4]int",
            "fn main() {\n    var a [3]int\n    var b [4]int\n    _ = a == b\n}\n",
        ),
        (
            "[3]int != [4]int",
            "fn main() {\n    var a [3]int\n    var b [4]int\n    _ = a != b\n}\n",
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
            "var c [4]int = [3]int{1, 2, 3}",
            "fn main() {\n    var c [4]int = [3]int{1, 2, 3}\n    _ = c\n}\n",
        ),
        (
            "b ([4]int) = [3]int{1, 2, 3}",
            "fn main() {\n    var b [4]int\n    b = [3]int{1, 2, 3}\n    _ = b\n}\n",
        ),
        (
            "take(a) with take(x [4]int)",
            "fn take(x [4]int) int {\n    return x[0]\n}\nfn main() {\n    var a [3]int\n    _ = take(a)\n}\n",
        ),
        (
            "take([3]int{1, 2, 3}) with take(x [4]int)",
            "fn take(x [4]int) int {\n    return x[0]\n}\nfn main() {\n    _ = take([3]int{1, 2, 3})\n}\n",
        ),
        (
            "method(a) with (s my_holder) method(x [4]int)",
            "struct my_holder {}\nfn (s my_holder) take(x [4]int) int {\n    return x[0]\n}\nfn main() {\n    var s my_holder\n    var a [3]int\n    _ = s.take(a)\n}\n",
        ),
        (
            "parallel assignment with mismatched array lengths",
            "fn main() {\n    var a [3]int\n    var b [4]int\n    var x int\n    b, x = a, 1\n    _ = b\n    _ = x\n}\n",
        ),
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for {name}");
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
            "var c [3]int = a ([3]int)",
            "fn main() {\n    var a [3]int\n    var c [3]int = a\n    _ = c\n}\n",
        ),
        (
            "var c [3]int = [3]int{1, 2, 3}",
            "fn main() {\n    var c [3]int = [3]int{1, 2, 3}\n    _ = c\n}\n",
        ),
        (
            "take(a) with take(x [3]int)",
            "fn take(x [3]int) int {\n    return x[0]\n}\nfn main() {\n    var a [3]int\n    _ = take(a)\n}\n",
        ),
        (
            "slices of different lengths stay accepted",
            "fn main() {\n    var a []int = [1, 2]\n    var b []int = [1, 2, 3]\n    a = b\n    _ = a\n}\n",
        ),
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Accept, "{name}: {:?}", r.diagnostics);
        assert!(error_codes(&r).is_empty(), "{name}: {:?}", r.diagnostics);
    }
}

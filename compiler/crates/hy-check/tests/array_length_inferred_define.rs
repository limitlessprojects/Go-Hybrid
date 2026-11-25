//! Arrays bound with `:=` keep their length: comparing, assigning, passing or
//! appending a `[3]int` inferred local where a `[2]int` is required passed
//! `hyb check` and failed `go build` (`mismatched types [2]int and [3]int`).

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

#[test]
fn inferred_array_length_mismatch_rejects() {
    for (name, src) in [
        (
            "a == b",
            "fn main() {\n    a := [2]int{1, 2}\n    b := [3]int{1, 2, 3}\n    _ = a == b\n}\n",
        ),
        (
            "a = b",
            "fn main() {\n    a := [2]int{1, 2}\n    b := [3]int{1, 2, 3}\n    a = b\n    _ = a\n}\n",
        ),
        (
            "var c [2]int = b",
            "fn main() {\n    b := [3]int{1, 2, 3}\n    var c [2]int = b\n    _ = c\n}\n",
        ),
        (
            "take(b)",
            "fn take(x [2]int) int {\n    return x[0]\n}\nfn main() {\n    b := [3]int{1, 2, 3}\n    _ = take(b)\n}\n",
        ),
        (
            "append(ss, b)",
            "fn main() {\n    b := [3]int{1, 2, 3}\n    var ss [][2]int\n    ss = append(ss, b)\n    _ = ss\n}\n",
        ),
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "{name}: {:?}", r.diagnostics);
    }
}

#[test]
fn inferred_array_same_length_stays_accepted() {
    for (name, src) in [
        (
            "a == b",
            "fn main() {\n    a := [2]int{1, 2}\n    b := [2]int{3, 4}\n    _ = a == b\n}\n",
        ),
        (
            "a = b copy",
            "fn main() {\n    a := [2]int{1, 2}\n    b := a\n    a = b\n    _ = a\n}\n",
        ),
        (
            "index and range",
            "fn main() {\n    a := [3]int{1, 2, 3}\n    s := 0\n    for _, v in a {\n        s += v\n    }\n    _ = a[1] + s\n}\n",
        ),
    ] {
        let r = check(src);
        assert_ne!(r.verdict, Verdict::Reject, "{name}: {:?}", r.diagnostics);
    }
}

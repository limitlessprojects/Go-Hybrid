//! Slice-to-slice conversions need identical element types (issue #1838,
//! rule BUILDABLE): `[]int(strs)` is accepted by `hyb check` but rejected by
//! `go build`.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn rejects_e1002(src: &str, needle: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{src}");
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.code == "HYB-E1002" && d.message.contains(needle)),
        "{:?}",
        r.diagnostics
    );
}

#[test]
fn string_slice_to_int_slice_rejects() {
    rejects_e1002(
        "fn main() {\n    var s []string = []string{\"a\", \"b\"}\n    var n = []int(s)\n    println(n[0])\n}\n",
        "incompatible element types",
    );
}

#[test]
fn int_slice_to_string_slice_rejects() {
    rejects_e1002(
        "fn main() {\n    s := []int{1, 2}\n    _ = []string(s)\n}\n",
        "incompatible element types",
    );
}

#[test]
fn int_slice_to_bool_slice_rejects() {
    rejects_e1002(
        "fn main() {\n    s := []int{1, 2}\n    _ = []bool(s)\n}\n",
        "incompatible element types",
    );
}

#[test]
fn string_slice_to_byte_slice_rejects() {
    rejects_e1002(
        "fn main() {\n    s := []string{\"a\"}\n    _ = []byte(s)\n}\n",
        "incompatible element types",
    );
}

#[test]
fn matching_slice_conversions_still_accepted() {
    let src = "fn main() {\n    s := []string{\"a\"}\n    t := []string(s)\n    n := []int{1}\n    m := []int(n)\n    b := []byte(\"hi\")\n    c := []byte(b)\n    r := []rune(\"hi\")\n    println(len(t), len(m), len(c), len(r))\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

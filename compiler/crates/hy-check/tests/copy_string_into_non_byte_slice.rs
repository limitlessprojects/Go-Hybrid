//! `copy(dst, string)` needs `dst` to be a byte slice (issue #1839, rule
//! BUILDABLE): `copy([]int, "s")` is accepted by `hyb check` but rejected by
//! `go build`.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn rejects(src: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{src}");
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.code == "HYB-E1002" && d.message.contains("cannot copy string into slice")),
        "{:?}",
        r.diagnostics
    );
}

#[test]
fn typed_var_int_slice_rejects() {
    rejects("fn main() {\n    var dst []int = []int{1, 2, 3, 4}\n    var src string = \"test\"\n    copy(dst, src)\n}\n");
}

#[test]
fn define_int64_slice_literal_rejects() {
    rejects("fn main() {\n    dst := []int64{1, 2}\n    copy(dst, \"ab\")\n}\n");
}

#[test]
fn make_rune_slice_rejects() {
    rejects("fn main() {\n    dst := make([]rune, 4)\n    copy(dst, \"ab\")\n}\n");
}

#[test]
fn int_slice_param_rejects() {
    rejects("fn f(dst []int, s string) {\n    copy(dst, s)\n}\n\nfn main() {\n    f([]int{1}, \"a\")\n}\n");
}

#[test]
fn subslice_of_int_slice_rejects() {
    rejects("fn main() {\n    dst := []int{1, 2, 3}\n    copy(dst[1:], \"ab\")\n}\n");
}

#[test]
fn byte_destinations_still_accepted() {
    let src = "fn f(dst []byte, s string) {\n    copy(dst, s)\n}\n\nfn main() {\n    a := make([]byte, 4)\n    copy(a, \"ab\")\n    var b []uint8 = make([]uint8, 2)\n    copy(b, \"x\")\n    c := []byte{1, 2}\n    copy(c[1:], \"y\")\n    f(a, \"z\")\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn shadowed_name_uses_inner_type() {
    let src = "fn main() {
    dst := []int{1}
    _ = dst
    n := 1
    if n > 0 {
        dst := make([]byte, 2)
        copy(dst, \"ab\")
    }
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn slice_to_slice_copy_unaffected() {
    let src = "fn main() {\n    a := []int{1, 2}\n    b := make([]int, 2)\n    copy(b, a)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

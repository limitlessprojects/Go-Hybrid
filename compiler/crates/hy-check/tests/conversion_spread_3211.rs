//! #3211: type conversions must reject variadic spread.
use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("parses");
    check_file(&ast)
}

fn has_e1002(r: &hy_check::CheckResult) -> bool {
    r.diagnostics.iter().any(|d| d.code == "HYB-E1002")
}

#[test]
fn spread_on_named_conversion_rejects() {
    let r = check("fn main() {\n    xs := []byte{65, 66}\n    s := string(xs...)\n    _ = s\n}\n");
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(has_e1002(&r), "{:?}", r.diagnostics);
}

#[test]
fn spread_on_slice_conversion_rejects() {
    let r = check("fn main() {\n    s := \"hello\"\n    b := []byte(s...)\n    _ = b\n}\n");
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(has_e1002(&r), "{:?}", r.diagnostics);
}

#[test]
fn spread_on_array_conversion_rejects() {
    let r = check("fn main() {\n    xs := []int{1, 2, 3}\n    a := [3]int(xs...)\n    _ = a\n}\n");
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(has_e1002(&r), "{:?}", r.diagnostics);
}

#[test]
fn non_spread_conversions_accepted() {
    let r = check("fn main() {\n    xs := []byte{65, 66}\n    _ = string(xs)\n    s := \"hello\"\n    _ = []byte(s)\n}\n");
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn append_spread_still_accepted() {
    let r = check("fn main() {\n    a := []int{1}\n    b := []int{2}\n    c := append(a, b...)\n    _ = c\n}\n");
    assert_eq!(r.verdict, Verdict::Accept);
}

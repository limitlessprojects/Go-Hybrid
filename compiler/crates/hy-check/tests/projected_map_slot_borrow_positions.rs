use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    check_file(&parse_file("inline.hy", src).expect("must parse"))
}

#[test]
fn projected_map_element_borrows_reject_e3104() {
    // Taking a mutable pointer to a map-slot field across a function call
    // (projected borrow) must reject HYB-E3104 (map-slot-loan cannot escape).
    let src = r#"struct bucket { items []int }
fn len(p *[]int) int { return p.len }
fn main() {
    m := map[string]bucket{"a": bucket{items: []int{1, 2}}}
    var n int = len(&m["a"].items)
    fmt.println(n)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected REJECT for projected map-slot borrow:\n{}", src);
    assert!(r.diagnostics.iter().any(|d| d.severity == Severity::Error && d.code == "HYB-E3104"),
            "expected HYB-E3104 for:\n{}\ndiags: {:?}", src, r.diagnostics);
}

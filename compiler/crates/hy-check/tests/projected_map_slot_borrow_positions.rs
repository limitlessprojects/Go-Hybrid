use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    check_file(&parse_file("inline.hy", src).expect("must parse"))
}

#[test]
fn projected_map_element_borrows_reject_e3104_while_slice_hop_accepts() {
    let reject_cases = [
        r#"struct Acct { bal int }
fn touch(p *int) { *p = 9 }
fn main() {
    m := map[string]Acct{"a": Acct{bal: 1}}
    touch(&m["a"].bal)
}
"#,
        r#"fn touch(p *int) { *p = 9 }
fn main() {
    m := map[string][2]int{"a": [2]int{1, 2}}
    touch(&m["a"][0])
}
"#,
        r#"struct Bucket { slots [2]int }
fn touch(p *int) { *p = 9 }
fn main() {
    m := map[string]Bucket{"a": Bucket{slots: [2]int{1, 2}}}
    touch(&m["a"].slots[0])
}
"#,
    ];
    for src in reject_cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected REJECT for:\n{}", src);
        assert!(r.diagnostics.iter().any(|d| d.severity == Severity::Error && d.code == "HYB-E3104"),
                "expected HYB-E3104 for:\n{}\ndiags: {:?}", src, r.diagnostics);
    }

    let ok = check(r#"struct Bucket { items []int }
fn touch(p *int) { *p = 9 }
fn main() {
    m := map[string]Bucket{"a": Bucket{items: []int{1, 2}}}
    touch(&m["a"].items[0])
}
"#);
    assert_eq!(ok.verdict, Verdict::Accept);
}

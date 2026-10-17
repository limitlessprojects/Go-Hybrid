use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("projection_guard.hy", src).expect("must parse");
    check_file(&ast)
}

#[test]
fn map_index_projection_stays_in_borrow_freeze_component() {
    let src = r#"
struct node {
    val int
}

fn worker(n node) {}

fn main() {
    n := node{val: 0}
    mp := map[int]*node{1: &n}
    p := mp[1]
    task worker(move n)
    p.val = 42
}
"#;

    let res = check(src);
    assert!(matches!(res.verdict, Verdict::Reject), "{:?}", res.diagnostics);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected O5.2 freeze through projection may-share component: {:?}",
        res.diagnostics
    );
}

#[test]
fn slice_index_projection_stays_in_borrow_freeze_component() {
    let src = r#"
struct node {
    val int
}

fn worker(n node) {}

fn main() {
    n := node{val: 0}
    s := []*node{&n}
    p := s[0]
    task worker(move n)
    p.val = 42
}
"#;

    let res = check(src);
    assert!(matches!(res.verdict, Verdict::Reject), "{:?}", res.diagnostics);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected O5.2 freeze through projection may-share component: {:?}",
        res.diagnostics
    );
}

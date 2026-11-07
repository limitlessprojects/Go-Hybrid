use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

#[test]
fn rejects_addr_of_target_in_assignment() {
    for src in [
        "fn main() {\n    var x = 1\n    &x = 2\n}",
        "fn main() {\n    x := 10\n    &x += 5\n}",
        "fn main() {\n    x := 1\n    &x++\n}",
        "struct point { x int }\nfn main() {\n    pt := point{x: 1}\n    &pt.x = 2\n}",
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for:\n{src}");
        assert!(
            r.diagnostics.iter().any(|d| d.code == "HYB-E2010"),
            "expected HYB-E2010 for:\n{src}, got: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn accepts_valid_assign_targets() {
    for src in [
        "fn main() {\n    var x = 1\n    x = 2\n}",
        "fn main() {\n    x := 10\n    x += 5\n}",
        "struct point { x int }\nfn main() {\n    pt := point{x: 1}\n    pt.x = 2\n}",
        "fn main() {\n    xs := []int{1, 2}\n    xs[0] = 3\n}",
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Accept, "expected accept for:\n{src}, got: {:?}", r.diagnostics);
    }
}

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> (Verdict, Vec<String>) {
    let ast = parse_file("test.hy", src).expect("must parse");
    let res = check_file(&ast);
    let codes = res.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.code.clone()).collect();
    (res.verdict, codes)
}

#[test]
fn rejects_elided_struct_literals_in_interface_and_non_slice_nested_slots() {
    for body in [
        "xs := []any{{x: 1}}\n fmt.println(len(xs))",
        "m := map[string]any{\"a\": {x: 1}}\n fmt.println(len(m))",
        "es := []error{{}}\n fmt.println(len(es))",
        "xs := []any{[{x: 1}]}\n fmt.println(len(xs))",
        "m := map[string]any{\"a\": [{x: 1}]}\n fmt.println(len(m))",
        "g := [][]point{[[{x: 1}]]}\n fmt.println(len(g))",
    ] {
        let src = format!("struct point {{ x int }}\nfn main() {{\n {body}\n}}\n");
        let (verdict, codes) = check(&src);
        assert_eq!(verdict, Verdict::Reject, "expected Reject for:\n{src}");
        assert!(codes.iter().any(|c| c == "HYB-E1002"), "expected HYB-E1002, got {codes:?}");
    }
}

#[test]
fn accepts_elided_struct_literals_in_struct_pointer_and_aliased_slots() {
    let src = "struct point { x int }\ntype pt = point\ntype row = []point\nfn main() {\n a := []point{{x: 1}}\n b := []*point{{x: 2}}\n c := [][]point{[{x: 3}]}\n d := []pt{{x: 4}}\n e := []row{[{x: 5}]}\n fmt.println(a[0].x, b[0].x, c[0][0].x, d[0].x, e[0][0].x)\n}\n";
    let (verdict, codes) = check(src);
    assert_eq!(verdict, Verdict::Accept, "expected Accept, got {codes:?}");
}

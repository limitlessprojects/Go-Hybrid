//! Regression tests for #3722: `resolve_fn_callee_ty` must resolve indirect
//! function callees reached through slice expressions (`fns[0:1][0]`),
//! `append` calls (`append(fns, minmax)[0]`), and struct field projections on
//! composite bases (`boxes[0:1][0].mm`, `append(boxes, h)[0].mm`).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).expect("must parse");
    check_file(&ast)
}

fn assert_only_e2002(src: &str) {
    let r = check(src);
    let cs: Vec<_> = r.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.code.as_str()).collect();
    assert_eq!(cs, vec!["HYB-E2002"], "expected only [HYB-E2002], got {cs:?} for:\n{src}");
    assert_eq!(r.verdict, Verdict::Reject);
}

fn assert_accept(src: &str) {
    let r = check(src);
    let cs: Vec<_> = r.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.code.as_str()).collect();
    assert!(cs.is_empty(), "expected 0 errors, got {cs:?} for:\n{src}");
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn composite_slice_append_and_field_fn_callees_reject_e2002() {
    assert_only_e2002(
        "fn minmax(a int, b int) (int, int) { return a, b }\nfn main() {\n    fns := []fn(a int, b int) int int{minmax}\n    p := fns[0:1][0](3, 9)\n    _ = p\n}\n",
    );
    assert_only_e2002(
        "fn minmax(a int, b int) (int, int) { return a, b }\nfn main() {\n    fns := []fn(a int, b int) int int{minmax}\n    p := append(fns, minmax)[0](3, 9)\n    _ = p\n}\n",
    );
    assert_only_e2002(
        "struct holder { mm fn(a int, b int) int int }\nfn minmax(a int, b int) (int, int) { return a, b }\nfn main() {\n    boxes := []holder{holder{mm: minmax}}\n    p := boxes[0:1][0].mm(3, 9)\n    _ = p\n}\n",
    );
    assert_only_e2002(
        "struct holder { mm fn(a int, b int) int int }\nfn minmax(a int, b int) (int, int) { return a, b }\nfn main() {\n    boxes := []holder{holder{mm: minmax}}\n    p := append(boxes, holder{mm: minmax})[0].mm(3, 9)\n    _ = p\n}\n",
    );
}

#[test]
fn composite_fn_callees_destructuring_accepts() {
    assert_accept(
        "struct holder { mm fn(a int, b int) int int }\nfn minmax(a int, b int) (int, int) { return a, b }\nfn main() {\n    fns := []fn(a int, b int) int int{minmax}\n    a, b := fns[0:1][0](3, 9)\n    boxes := []holder{holder{mm: minmax}}\n    c, d := boxes[0:1][0].mm(a, b)\n    _ = c\n    _ = d\n}\n",
    );
}

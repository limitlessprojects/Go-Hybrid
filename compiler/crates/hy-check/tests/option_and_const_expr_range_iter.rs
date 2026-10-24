use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn first_code(r: &hy_check::CheckResult) -> Option<String> {
    r.diagnostics
        .iter()
        .find(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code.clone())
}

#[test]
fn range_over_option_collections_and_some_rejects_e2009() {
    for src in [
        "fn run(opt option[[]int]) {\n  for x in opt { _ = x }\n}",
        "type maybe_str = option[string]\nfn run(opt maybe_str) {\n  for ch in opt { _ = ch }\n}",
        "fn main() {\n  xs := []int{1, 2}\n  for x in some(xs) { _ = x }\n}",
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected reject for:\n{src}");
        assert_eq!(first_code(&r).as_deref(), Some("HYB-E2009"), "{:?}", r.diagnostics);
    }
}

#[test]
fn range_over_ascribed_unwrap_and_int_const_expr_still_accepts() {
    let r = check(
        "const hi = 2 + 3\n\
         fn run(opt option[[]int]) {\n\
           var xs []int = opt.unwrap()\n\
           for x in xs { _ = x }\n\
           for i in 0..hi { _ = i }\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn compound_non_integer_const_range_bounds_reject_e2009() {
    for src in [
        "const hi = 1.5 + 0.5\nfn main() {\n  for i in 0..hi { _ = i }\n}",
        "const flag = !false\nfn main() {\n  for i in 0..flag { _ = i }\n}",
        "const label = \"a\" + \"b\"\nfn main() {\n  for i in 0..label { _ = i }\n}",
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected reject for:\n{src}");
        assert_eq!(first_code(&r).as_deref(), Some("HYB-E2009"), "{:?}", r.diagnostics);
    }
}

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn has_e1001(r: &hy_check::CheckResult) -> bool {
    r.diagnostics.iter().any(|d| d.code == "HYB-E1001")
}

#[test]
fn parenthesized_undefined_field_base_rejects_e1001() {
    for src in [
        "fn main() {\n  _ = (missing_var).x\n}",
        "fn main() {\n  (missing_var).call()\n}",
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected reject for:\n{src}");
        assert!(has_e1001(&r), "{:?}", r.diagnostics);
    }
}

#[test]
fn nested_folded_call_indices_reject_undefined_names_e1001() {
    for src in [
        "fn run(fns []fn(int) int, idx_map map[string]int) {\n  _ = fns[idx_map[missing_key]](1)\n}",
        "fn run(fns []fn(int) int, k string) {\n  _ = fns[missing_map[k]](1)\n}",
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected reject for:\n{src}");
        assert!(has_e1001(&r), "{:?}", r.diagnostics);
    }
}

#[test]
fn defined_parenthesized_struct_and_nested_indexed_callees_still_accept() {
    let r = check(
        "struct holder {\n  v int\n}\n\
         fn run(h holder, fns []fn(int) int, idx_map map[string]int, k string) {\n\
           _ = (h).v\n\
           _ = fns[idx_map[k]](1)\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

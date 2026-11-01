use hy_check::check_file;
use hy_syntax::parse_file;

fn has_code(src: &str, code: &str) -> bool {
    let file = parse_file("test.hy", src).expect("parse should succeed");
    check_file(&file).diagnostics.iter().any(|d| d.code == code)
}

#[test]
fn generic_fn_call_arity_and_type_arg_comparability_729() {
    assert!(has_code("fn id[t any](x t) t { return x }\nfn main() { _ = id[int]() }\n", "HYB-E1002"));
    assert!(has_code("fn id[t any](x t) t { return x }\nfn main() { _ = id(1, 2) }\n", "HYB-E1002"));
    assert!(has_code("fn eq[t comparable](a t, b t) bool { return a == b }\nfn main() { _ = eq[[]int]([1], [2]) }\n", "HYB-E2006"));
    assert!(has_code("fn eq[t comparable](a t, b t) bool { return a == b }\nfn main() { _ = eq([1, 2], [3, 4]) }\n", "HYB-E2006"));
    let f = parse_file("test.hy", "fn eq[t comparable](a t, b t) bool { return a == b }\nfn main() { _ = eq(1, 1) }\n").unwrap();
    assert!(check_file(&f).diagnostics.is_empty());
}

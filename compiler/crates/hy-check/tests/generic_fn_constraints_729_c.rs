use hy_check::check_file;
use hy_syntax::parse_file;

fn has_code(src: &str, code: &str) -> bool {
    let file = parse_file("test.hy", src).expect("parse should succeed");
    check_file(&file).diagnostics.iter().any(|d| d.code == code)
}

#[test]
fn generic_fn_bound_type_mismatch_and_struct_constraint_729() {
    assert!(has_code("fn same[t any](a t, b t) t { return a }\nfn main() { _ = same(1, \"two\") }\n", "HYB-E1002"));
    assert!(has_code("fn head[t any](xs []t) int { return len(xs) }\nfn main() { _ = head(42) }\n", "HYB-E1002"));
    assert!(has_code("struct table[k comparable, v any] { data map[k]v }\nfn main() { _ = table[[]int, string]{ data: map[[]int]string{} } }\n", "HYB-E2006"));
    assert!(has_code("struct box[t any] { val t }\nfn same_box[t any](a box[t], b box[t]) box[t] { return a }\nfn main() { _ = same_box(box[int]{ val: 1 }, box[string]{ val: \"hi\" }) }\n", "HYB-E1002"));
    let f = parse_file("test.hy", "struct box[t any] { val t }\nfn unbox[t any](b box[t]) t { return b.val }\nfn main() { _ = unbox(box[int]{ val: 7 }) }\n").unwrap();
    assert!(check_file(&f).diagnostics.is_empty());
}

fn has_e1002(src: &str) -> bool { hy_check::check_file(&hy_syntax::parse_file("test.hy", src).unwrap()).diagnostics.iter().any(|d| d.code == "HYB-E1002") }
#[test]
fn shadowed_unnamed_fn_param_and_new_make_type_operands_reject_e1002() {
    for src in ["struct item { x int }\nfn main() { item := 1\n var cb fn(item)\n _ = cb\n _ = item }\n", "struct item { x int }\nfn main() { item := 1\n p := new(item)\n _ = p\n _ = item }\n", "fn main() { option := 1\n p := new(option[int])\n _ = p\n _ = option }\n", "type ints = []int\nfn main() { ints := 1\n s := make(ints, 1)\n _ = s\n _ = ints }\n"] { assert!(has_e1002(src), "{src}"); }
    assert!(!has_e1002("struct item { x int }\nfn main() { var cb fn(item)\n p := new(item)\n _ = cb\n _ = p }\n"));
}

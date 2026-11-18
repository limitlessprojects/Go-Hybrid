use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let f = parse_file("test.hy", src).expect("parses");
    check_file(&f).diagnostics.into_iter().map(|d| d.code).collect()
}

#[test]
fn resolve_accepts_new_set_option_and_generic_type_params_3691() {
    let ok = "import \"fmt\"\nstruct box[t] {\n    val t\n}\nfn alloc_box[t any]() *box[t] {\n    return new(box[t])\n}\nfn alloc_set[t comparable]() *set[t] {\n    return new(set[t])\n}\nfn main() {\n    s := new(set[int])\n    s.insert(1)\n    o := new(option[int])\n    b := alloc_box[int]()\n    s2 := alloc_set[string]()\n    fmt.println(len(s), o.is_none(), b.val, len(s2))\n}\n";
    assert!(codes(ok).is_empty(), "{:?}", codes(ok));

    let bad_set_elem = "fn main() {\n    _ = new(set[[]int])\n}\n";
    assert_eq!(codes(bad_set_elem), vec!["HYB-E1002"]);

    let bad_undef_arg = "struct box[t] {\n    val t\n}\nfn main() {\n    _ = new(box[undef_ty])\n}\n";
    assert!(codes(bad_undef_arg).iter().any(|c| c == "HYB-E1001"), "{:?}", codes(bad_undef_arg));
}

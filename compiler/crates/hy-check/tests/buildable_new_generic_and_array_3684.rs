use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let f = parse_file("test.hy", src).expect("parses");
    check_file(&f).diagnostics.into_iter().map(|d| d.code).collect()
}

#[test]
fn buildable_accepts_new_generic_and_types_new_array_3684() {
    let ok = "import \"fmt\"\nstruct box[t] {\n    val t\n}\nfn main() {\n    b := new(box[int])\n    b.val = 7\n    a := new([2]int{})\n    fmt.println(b.val, len(a), cap(a), a[0], len(a[0:1]))\n}\n";
    assert!(codes(ok).is_empty(), "{:?}", codes(ok));

    let bad_field_ty = "struct box[t] {\n    val t\n}\nfn main() {\n    b := new(box[int])\n    _ = b.val + \"bad\"\n}\n";
    assert!(codes(bad_field_ty).iter().any(|c| c == "HYB-E1002"), "{:?}", codes(bad_field_ty));

    let bad_non_generic = "struct plain {\n    val int\n}\nfn main() {\n    _ = new(plain[int])\n}\n";
    assert!(codes(bad_non_generic).iter().any(|c| c == "HYB-E1002"), "{:?}", codes(bad_non_generic));

    let bad_value_index = "fn main() {\n    arr := [1]int{0}\n    _ = new(arr[0])\n}\n";
    assert!(codes(bad_value_index).iter().any(|c| c == "HYB-E2005"), "{:?}", codes(bad_value_index));
}

use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    check_file(&parse_file("test.hy", src).expect("parses")).diagnostics.into_iter().map(|d| d.code).collect()
}
fn assert_e1002(src: &str) { assert_eq!(codes(src), vec!["HYB-E1002".to_string()], "for:\n{src}"); }
fn assert_clean(src: &str) { let cs = codes(src); assert!(cs.is_empty(), "got {cs:?} for:\n{src}"); }

#[test]
fn struct_lit_channel_width_mismatch_is_rejected() {
    assert_e1002("struct pair { ch chan int32 }\nfn main() { var bad chan int64\n _ = pair{ ch: bad } }\n");
}

#[test]
fn struct_lit_channel_array_len_mismatch_is_rejected() {
    assert_e1002("struct pair { ch chan [3]int }\nfn main() { var bad chan [4]int\n _ = pair{ ch: bad } }\n");
}

#[test]
fn struct_lit_map_val_width_mismatch_is_rejected() {
    assert_e1002("struct pair { m map[string]int32 }\nfn main() { var bad map[string]int64\n _ = pair{ m: bad } }\n");
}

#[test]
fn struct_lit_map_key_width_mismatch_is_rejected() {
    assert_e1002("struct pair { m map[int32]string }\nfn main() { var bad map[int64]string\n _ = pair{ m: bad } }\n");
}

#[test]
fn struct_lit_slice_element_width_mismatch_is_rejected() {
    assert_e1002("struct pair { sl []int32 }\nfn main() { var bad []int64\n _ = pair{ sl: bad } }\n");
}

#[test]
fn struct_lit_array_alias_mismatch_is_rejected() {
    assert_e1002("type arr32 = [3]int32\ntype arr64 = [3]int64\nstruct pair { a arr32 }\nfn main() { var bad arr64\n _ = pair{ a: bad } }\n");
}

#[test]
fn struct_lit_matching_fields_are_accepted() {
    assert_clean("struct pair { ch chan int32\n m map[string]int32\n sl []int32 }\nfn main() { var c chan int32\n var m map[string]int32\n var sl []int32\n _ = pair{ ch: c, m: m, sl: sl } }\n");
}

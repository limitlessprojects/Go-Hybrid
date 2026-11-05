use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    check_file(&parse_file("test.hy", src).expect("parses")).diagnostics.into_iter().map(|d| d.code).collect()
}
fn assert_e1002(src: &str) { assert_eq!(codes(src), vec!["HYB-E1002".to_string()], "for:\n{src}"); }
fn assert_clean(src: &str) { let cs = codes(src); assert!(cs.is_empty(), "got {cs:?} for:\n{src}"); }

#[test]
fn channel_element_width_mismatch_on_return_is_rejected() {
    assert_e1002("fn f() chan int32 { var c chan int64\n return c }\nfn main() {}\n");
}

#[test]
fn map_value_width_mismatch_on_return_is_rejected() {
    assert_e1002("fn f() map[string]int32 { var m map[string]int64\n return m }\nfn main() {}\n");
}

#[test]
fn map_key_width_mismatch_on_return_is_rejected() {
    assert_e1002("fn f() map[int32]string { var m map[int64]string\n return m }\nfn main() {}\n");
}

#[test]
fn slice_element_width_mismatch_on_return_is_rejected() {
    assert_e1002("fn f() []int32 { var s []int64\n return s }\nfn main() {}\n");
}

#[test]
fn pointer_base_width_mismatch_on_return_is_rejected() {
    assert_e1002("fn f() *int32 { var p *int64\n return p }\nfn main() {}\n");
}

#[test]
fn array_alias_width_mismatch_on_return_is_rejected() {
    assert_e1002("type arr1 = [3]int32\ntype arr2 = [3]int64\nfn f() arr1 { var a arr2\n return a }\nfn main() {}\n");
}

#[test]
fn array_length_mismatch_on_return_is_rejected() {
    assert_e1002("fn f() [3]int { var a [4]int\n return a }\nfn main() {}\n");
}

#[test]
fn matching_container_returns_are_accepted() {
    assert_clean("fn f() chan int32 { var c chan int32\n return c }\nfn main() {}\n");
    assert_clean("fn f() map[string]int32 { var m map[string]int32\n return m }\nfn main() {}\n");
    assert_clean("fn f() []int32 { var s []int32\n return s }\nfn main() {}\n");
    assert_clean("fn f() *int32 { var p *int32\n return p }\nfn main() {}\n");
    assert_clean("fn f() [3]int { var a [3]int\n return a }\nfn main() {}\n");
}

use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("inline.hy", src).expect("parses");
    check_file(&ast).diagnostics.into_iter().map(|d| d.code).collect()
}

#[test]
fn issue_839_closure_capture_rebind_param_stores_and_multi_return() {
    let src1 = "fn worker(data map[string]int) { data[\"w\"] = 1 }\n\
                fn main() {\n\
                  outer := map[string]int{\"init\": 0}\n\
                  save := fn(src map[string]int) { outer = src }\n\
                  m := map[string]int{\"a\": 1}\n\
                  save(m)\n\
                  task worker(move m)\n\
                  outer[\"main\"] = 2\n\
                }";
    assert!(codes(src1).contains(&"HYB-E3003".to_string()));

    let src2 = "fn dup(m map[string]int) (map[string]int, bool) { return m, true }\n\
                fn worker(data map[string]int) { data[\"w\"] = 1 }\n\
                fn main() {\n\
                  outer := map[string]int{\"init\": 0}\n\
                  ok := false\n\
                  save := fn(src map[string]int) { outer, ok = dup(src) }\n\
                  m := map[string]int{\"a\": 1}\n\
                  save(m)\n\
                  task worker(move m)\n\
                  outer[\"main\"] = 2\n\
                }";
    assert!(codes(src2).contains(&"HYB-E3003".to_string()));
}

use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("inline.hy", src).expect("parses");
    check_file(&ast).diagnostics.into_iter().map(|d| d.code).collect()
}

#[test]
fn issue_839_closure_capture_rebind_non_owned_and_shared_return() {
    let src3 = "fn worker(data map[string]int) { data[\"w\"] = 1 }\n\
                fn launder(p map[string]int) {\n\
                  outer := map[string]int{\"init\": 0}\n\
                  save := fn(src map[string]int) { outer = src }\n\
                  save(p)\n\
                  out2 := outer\n\
                  outer = map[string]int{\"fresh\": 1}\n\
                  task worker(move out2)\n\
                }";
    assert!(codes(src3).contains(&"HYB-E3004".to_string()));

    let src4 = "fn worker(data map[string]int) { data[\"w\"] = 1 }\n\
                fn pair_via_closure(b map[string]int) (map[string]int, map[string]int) {\n\
                  outer := map[string]int{\"init\": 0}\n\
                  save := fn(src map[string]int) { outer = src }\n\
                  save(b)\n\
                  return outer, b\n\
                }\n\
                fn main() {\n\
                  seed := map[string]int{\"k\": 1}\n\
                  x, y := pair_via_closure(seed)\n\
                  task worker(move x)\n\
                  y[\"race\"] = 2\n\
                }";
    assert!(codes(src4).contains(&"HYB-E3003".to_string()));
}

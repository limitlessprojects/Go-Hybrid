use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("inline.hy", src).expect("parses");
    check_file(&ast).diagnostics.into_iter().map(|d| d.code).collect()
}

#[test]
fn issue_839_closure_capture_rebind_a3_and_shadow_control() {
    let src_a3 = "struct entry { n int }\n\
                  fn main() {\n\
                    slot_m := map[string]entry{\"a\": entry{n: 1}}\n\
                    rebind_slot_m := fn() { slot_m = map[string]entry{\"a\": entry{n: 500}} }\n\
                    e := &slot_m[\"a\"]\n\
                    rebind_slot_m()\n\
                    e.n = 99\n\
                  }";
    assert!(codes(src_a3).contains(&"HYB-E3102".to_string()));

    let src_ok = "struct entry { n int }\n\
                  fn worker(data map[string]int) { data[\"w\"] = 1 }\n\
                  fn main() {\n\
                    outer := map[string]int{\"init\": 0}\n\
                    save_shadow := fn(src map[string]int) {\n\
                      var outer map[string]int\n\
                      outer = src\n\
                      _ = outer\n\
                    }\n\
                    m := map[string]int{\"a\": 1}\n\
                    save_shadow(m)\n\
                    task worker(move m)\n\
                    outer[\"main\"] = 2\n\
                    slot_m := map[string]entry{\"a\": entry{n: 1}}\n\
                    shadow_slot_m := fn() {\n\
                      var slot_m map[string]entry\n\
                      slot_m = map[string]entry{\"a\": entry{n: 500}}\n\
                      _ = slot_m\n\
                    }\n\
                    e := &slot_m[\"a\"]\n\
                    shadow_slot_m()\n\
                    e.n = 99\n\
                  }";
    assert!(codes(src_ok).is_empty());
}

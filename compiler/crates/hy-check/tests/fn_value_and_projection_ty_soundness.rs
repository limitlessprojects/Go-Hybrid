use hy_check::{check_file, CheckResult, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("test.hy", src).expect("parse ok");
    check_file(&ast)
}

fn codes(src: &str) -> Vec<String> {
    check(src).diagnostics.into_iter().map(|d| d.code).collect()
}

#[test]
fn type_aliased_fn_binding_call_preserves_return_shape_and_freezes_alias() {
    let src = "package main\nfn worker(m map[string]int, done chan<- bool) {\n    m[\"w\"] = 1\n    done <- true\n}\nfn main() {\n    m := map[string]int{\"a\": 1}\n    var g getter = fn(x map[string]int) map[string]int { return x }\n    alias := g(m)\n    copy_of_alias := alias\n    done := make(chan bool, 1)\n    task worker(move m, done)\n    copy_of_alias[\"k\"] = 99\n    <-done\n}\ntype getter = fn(x map[string]int) map[string]int\n";
    assert!(codes(src).iter().any(|c| c == "HYB-E3003"));
}

#[test]
fn struct_fn_field_call_preserves_return_shape_and_freezes_alias() {
    let src = "package main\nstruct holder { cb fn(x map[string]int) map[string]int }\nfn worker(m map[string]int, done chan<- bool) {\n    m[\"w\"] = 1\n    done <- true\n}\nfn main() {\n    m := map[string]int{\"a\": 1}\n    h := holder{cb: fn(x map[string]int) map[string]int { return x }}\n    alias := h.cb(m)\n    copy_of_alias := alias\n    done := make(chan bool, 1)\n    task worker(move m, done)\n    copy_of_alias[\"k\"] = 99\n    <-done\n}\n";
    assert!(codes(src).iter().any(|c| c == "HYB-E3003"));
}

#[test]
fn sliced_and_appended_fn_table_call_preserves_non_owned_origin() {
    let src = "package main\nfn worker(m map[string]int, done chan<- bool) {\n    m[\"w\"] = 1\n    done <- true\n}\nfn borrow_escape(borrowed map[string]int) {\n    cbs := []fn(x map[string]int) map[string]int{fn(x map[string]int) map[string]int { return x }}\n    sub := cbs[:]\n    sub2 := append(sub, cbs[0])\n    alias := sub2[0](borrowed)\n    var escaped map[string]int\n    escaped = alias\n    done := make(chan bool, 1)\n    task worker(move escaped, done)\n    <-done\n}\n";
    assert!(codes(src).iter().any(|c| c == "HYB-E3004"));
}

#[test]
fn multi_return_pointer_struct_fn_field_call_preserves_shapes() {
    let src = "package main\nstruct pair_box { cb fn(x map[string]int) map[string]int bool }\nfn pair_id(x map[string]int) (map[string]int, bool) { return x, true }\nfn worker(m map[string]int, done chan<- bool) {\n    m[\"w\"] = 1\n    done <- true\n}\nfn main() {\n    m := map[string]int{\"a\": 1}\n    box := pair_box{cb: pair_id}\n    p := &box\n    alias, ok := p.cb(m)\n    _ = ok\n    copy_of_alias := alias\n    done := make(chan bool, 1)\n    task worker(move m, done)\n    copy_of_alias[\"k\"] = 99\n    <-done\n}\n";
    assert!(codes(src).iter().any(|c| c == "HYB-E3003"));
}

#[test]
fn make_type_alias_and_comma_ok_lookup_preserve_inner_fn_ty() {
    let src = "package main\nfn worker(m map[string]int, done chan<- bool) {\n    m[\"w\"] = 1\n    done <- true\n}\nfn main() {\n    m := map[string]int{\"a\": 1}\n    tbl := make(fn_map)\n    cb, ok := tbl[\"id\"]\n    _ = ok\n    alias := cb(m)\n    copy_of_alias := alias\n    done := make(chan bool, 1)\n    task worker(move m, done)\n    copy_of_alias[\"k\"] = 99\n    <-done\n}\ntype fn_map = map[string]fn(x map[string]int) map[string]int\n";
    assert!(codes(src).iter().any(|c| c == "HYB-E3003"));
}

#[test]
fn channel_receive_preserves_fn_ty_in_comma_ok_form() {
    let src = "package main\nfn worker(m map[string]int, done chan<- bool) {\n    m[\"w\"] = 1\n    done <- true\n}\nfn receive_and_call(ch <-chan fn(x map[string]int) map[string]int) {\n    m := map[string]int{\"a\": 1}\n    cb, ok := <-ch\n    _ = ok\n    alias := cb(m)\n    copy_of_alias := alias\n    done := make(chan bool, 1)\n    task worker(move m, done)\n    copy_of_alias[\"k\"] = 99\n    <-done\n}\n";
    assert!(codes(src).iter().any(|c| c == "HYB-E3003"));
}

#[test]
fn positive_control_fn_value_calls_without_race_accept() {
    let src = "package main\ntype getter = fn(x map[string]int) int\nstruct holder { cb getter }\nfn main() {\n    m := map[string]int{\"a\": 1}\n    h := holder{cb: fn(x map[string]int) int { return x[\"a\"] }}\n    n := h.cb(m)\n    m[\"b\"] = n + 1\n}\n";
    assert_eq!(check(src).verdict, Verdict::Accept);
}

use hy_check::{check_file, CheckResult, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult { check_file(&parse_file("test.hy", src).expect("parses")) }
fn codes(r: &CheckResult) -> Vec<String> { r.diagnostics.iter().map(|d| d.code.clone()).collect() }

#[test]
fn push_string_during_live_borrow_loan_is_rejected_e3101() {
    let r_method = check("import \"rust/serde_json\"\nfn main() {\n doc := serde_json.parse(`{\"items\": [1, 2]}`)?\n for u in doc.array(\"items\")? {\n doc.push_string(\"mutated\")?\n fmt.println(u.int(\"k\")?)\n }\n}\n");
    assert_eq!(r_method.verdict, Verdict::Reject);
    assert!(codes(&r_method).iter().any(|c| c == "HYB-E3101"));
    let r_pkg = check("import \"rust/serde_json\"\nfn main() {\n doc := serde_json.parse(`{\"items\": [1, 2]}`)?\n items := doc.array(\"items\")?\n serde_json.push_string(doc, \"mutated\")?\n fmt.println(items.len())\n}\n");
    assert_eq!(r_pkg.verdict, Verdict::Reject);
    assert!(codes(&r_pkg).iter().any(|c| c == "HYB-E3101"));
}

#[test]
fn new_array_alias_used_after_move_is_rejected_e3003() {
    let r = check("import \"rust/serde_json\"\nfn ship(v serde_json.value, done chan<- bool) {\n v.push_string(\"worker\")\n done <- true\n}\nfn main() {\n arr := serde_json.new_array()\n alias := arr\n done := make(chan bool, 1)\n task ship(move arr, done)\n alias.push_string(\"main\")\n <-done\n}\n");
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(codes(&r).iter().any(|c| c == "HYB-E3003"));
}

#[test]
fn new_array_unmoved_at_spawn_is_rejected_e3001_and_moved_cleanly_accepts() {
    let r_unmoved = check("import \"rust/serde_json\"\nfn ship(v serde_json.value) {}\nfn main() {\n arr := serde_json.new_array()\n task ship(arr)\n}\n");
    assert_eq!(r_unmoved.verdict, Verdict::Reject);
    assert_eq!(codes(&r_unmoved), vec!["HYB-E3001"]);
    let r_moved = check("import \"rust/serde_json\"\nfn ship(v serde_json.value) {}\nfn main() {\n arr := serde_json.new_array()\n arr.push_string(\"ok\")\n task ship(move arr)\n}\n");
    assert_eq!(r_moved.verdict, Verdict::Accept);
}

#[test]
fn json_set_consumes_handle_value_argument_and_rejects_post_use_or_loan_conflict() {
    let r_ok = check("import \"rust/serde_json\"\nfn build() serde_json.value {\n v := serde_json.new_object()\n let arr := serde_json.new_array()\n arr.push_string(\"ada\")\n v.set(\"active\", arr)\n return v\n}\nfn main() {}\n");
    assert_eq!(r_ok.verdict, Verdict::Accept, "expected 0 diagnostics, got {:?}", r_ok.diagnostics);
    let r_post_use = check("import \"rust/serde_json\"\nfn main() {\n v := serde_json.new_object()\n arr := serde_json.new_array()\n v.set(\"active\", arr)\n arr.push_string(\"use-after-consume\")\n}\n");
    assert_eq!(r_post_use.verdict, Verdict::Reject);
    assert_eq!(codes(&r_post_use), vec!["HYB-E3003"]);
    assert_eq!(r_post_use.diagnostics[0].tier0_rule.as_deref(), Some("A7"));
    let r_pkg_post_use = check("import \"rust/serde_json\"\nfn main() {\n v := serde_json.new_object()\n child := serde_json.new_object()\n serde_json.set(v, \"sub\", child)?\n child.set(\"k\", 1)?\n}\n");
    assert_eq!(r_pkg_post_use.verdict, Verdict::Reject);
    assert_eq!(codes(&r_pkg_post_use), vec!["HYB-E3003"]);
    let r_loan = check("import \"rust/serde_json\"\nfn main() {\n v := serde_json.new_object()\n child := serde_json.parse(`{\"items\": [1, 2]}`)?\n items := child.array(\"items\")?\n v.set(\"sub\", child)?\n fmt.println(items.len())\n}\n");
    assert_eq!(r_loan.verdict, Verdict::Reject);
    assert!(codes(&r_loan).iter().any(|c| c == "HYB-E3101"));
}

#[test]
fn const_value_struct_pointer_receiver_mutation_is_rejected_e3006() {
    let r = check("struct counter { n int }\nconst shared = counter{n: 0}\nfn (c *counter) inc() { c.n = c.n + 1 }\nfn worker(done chan<- bool) {\n shared.inc()\n done <- true\n}\nfn main() {\n done := make(chan bool, 1)\n task worker(done)\n shared.inc()\n <-done\n}\n");
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(codes(&r).iter().any(|c| c == "HYB-E3006"));
    let r_ro = check("import \"fmt\"\nstruct counter { n int }\nconst shared = counter{n: 7}\nfn (c *counter) get() int { return c.n }\nfn main() {\n fmt.println(shared.get())\n}\n");
    assert_eq!(r_ro.verdict, Verdict::Accept);
}

#[test]
fn const_value_struct_auto_addressed_pointer_param_mutation_is_rejected_e3006() {
    let r = check("struct counter { n int }\nconst shared = counter{n: 0}\nfn bump(c *counter) { c.n = c.n + 1 }\nfn main() {\n bump(shared)\n}\n");
    assert_eq!(r.verdict, Verdict::Reject);
    assert_eq!(codes(&r), vec!["HYB-E3006"]);
}

#[test]
fn const_value_struct_addr_of_borrow_in_closure_and_rebind_is_rejected() {
    let r_closure = check("struct counter { n int }\nconst shared = counter{n: 0}\nfn main() {\n f := fn() {\n p := &shared\n p.n = 1\n }\n f()\n}\n");
    assert_eq!(r_closure.verdict, Verdict::Reject);
    assert!(codes(&r_closure).iter().any(|c| c == "HYB-E3006"));
    let r_rebind_move = check("struct counter { n int }\nconst shared = counter{n: 0}\nfn worker(p *counter) { p.n = 1 }\nfn main() {\n other := counter{n: 1}\n p := &shared\n p2 := p\n p = &other\n task worker(move p2)\n}\n");
    assert_eq!(r_rebind_move.verdict, Verdict::Reject);
    assert!(codes(&r_rebind_move).iter().any(|c| c == "HYB-E3004"));
    let r_multi_root = check("struct counter { n int }\nconst shared = counter{n: 0}\nfn worker(arr []*counter) { arr[1].n = 1 }\nfn main() {\n other := counter{n: 1}\n arr := []*counter{&other, &shared}\n task worker(move arr)\n}\n");
    assert_eq!(r_multi_root.verdict, Verdict::Reject);
    assert!(codes(&r_multi_root).iter().any(|c| c == "HYB-E3004"));
}

#[test]
fn json_set_rejects_non_owned_param_and_closure_capture_consume_e3004() {
    let r_method_param = check("import \"rust/serde_json\"\nfn store(doc serde_json.value, child serde_json.value) {\n doc.set(\"c\", child)?\n}\nfn main() {\n doc := serde_json.new_object()\n child := serde_json.new_object()\n store(doc, child)\n child.set(\"k\", 1)?\n}\n");
    assert_eq!(r_method_param.verdict, Verdict::Reject);
    assert!(codes(&r_method_param).iter().any(|c| c == "HYB-E3004"));

    let r_pkg_param = check("import \"rust/serde_json\"\nfn store(doc serde_json.value, child serde_json.value) {\n serde_json.set(doc, \"c\", child)?\n}\nfn main() {}\n");
    assert_eq!(r_pkg_param.verdict, Verdict::Reject);
    assert!(codes(&r_pkg_param).iter().any(|c| c == "HYB-E3004"));

    let r_capture = check("import \"rust/serde_json\"\nfn main() {\n doc := serde_json.new_object()\n child := serde_json.new_object()\n f := fn() {\n doc.set(\"c\", child)?\n }\n f()\n child.set(\"k\", 1)?\n}\n");
    assert_eq!(r_capture.verdict, Verdict::Reject);
    assert!(codes(&r_capture).iter().any(|c| c == "HYB-E3004"));
}

#[test]
fn json_set_consumes_projected_struct_slice_and_map_handles() {
    let r_field = check("import \"rust/serde_json\"\nstruct box { v serde_json.value }\nfn main() {\n doc := serde_json.new_object()\n child := serde_json.new_object()\n b := box{v: child}\n doc.set(\"c\", b.v)?\n child.set(\"k\", 1)?\n}\n");
    assert_eq!(r_field.verdict, Verdict::Reject);
    assert!(codes(&r_field).iter().any(|c| c == "HYB-E3003"));

    let r_slice = check("import \"rust/serde_json\"\nfn main() {\n doc := serde_json.new_object()\n child := serde_json.new_object()\n xs := []serde_json.value{child}\n serde_json.set(doc, \"c\", xs[0])?\n xs[0].set(\"k\", 1)?\n}\n");
    assert_eq!(r_slice.verdict, Verdict::Reject);
    assert!(codes(&r_slice).iter().any(|c| c == "HYB-E3003"));

    let r_map = check("import \"rust/serde_json\"\nfn main() {\n doc := serde_json.new_object()\n child := serde_json.new_object()\n mp := map[string]serde_json.value{\"k\": child}\n doc.set(\"c\", mp[\"k\"])?\n child.set(\"k\", 1)?\n}\n");
    assert_eq!(r_map.verdict, Verdict::Reject);
    assert!(codes(&r_map).iter().any(|c| c == "HYB-E3003"));

    let r_loan = check("import \"rust/serde_json\"\nstruct box { v serde_json.value }\nfn main() {\n doc := serde_json.new_object()\n child := serde_json.parse(`{\"items\": [1, 2]}`)?\n items := child.array(\"items\")?\n b := box{v: child}\n doc.set(\"c\", b.v)?\n _ = items.len()\n}\n");
    assert_eq!(r_loan.verdict, Verdict::Reject);
    assert!(codes(&r_loan).iter().any(|c| c == "HYB-E3101"));
}

#[test]
fn json_set_consumes_unrooted_and_forwarding_handle_expressions() {
    let r_comp_field = check("import \"rust/serde_json\"\nstruct box { v serde_json.value }\nfn main() {\n doc := serde_json.new_object()\n child := serde_json.new_object()\n doc.set(\"c\", (box{v: child}).v)?\n child.set(\"k\", 1)?\n}\n");
    assert_eq!(r_comp_field.verdict, Verdict::Reject);
    assert!(codes(&r_comp_field).iter().any(|c| c == "HYB-E3003"));

    let r_comp_slice = check("import \"rust/serde_json\"\nfn main() {\n doc := serde_json.new_object()\n child := serde_json.new_object()\n doc.set(\"c\", []serde_json.value{child}[0])?\n child.set(\"k\", 1)?\n}\n");
    assert_eq!(r_comp_slice.verdict, Verdict::Reject);
    assert!(codes(&r_comp_slice).iter().any(|c| c == "HYB-E3003"));

    let r_forward_call = check("import \"rust/serde_json\"\nfn id(v serde_json.value) serde_json.value { return v }\nfn main() {\n doc := serde_json.new_object()\n child := serde_json.new_object()\n doc.set(\"c\", id(child))?\n child.set(\"k\", 1)?\n}\n");
    assert_eq!(r_forward_call.verdict, Verdict::Reject);
    assert!(codes(&r_forward_call).iter().any(|c| c == "HYB-E3003"));

    let r_forward_param = check("import \"rust/serde_json\"\nfn id(v serde_json.value) serde_json.value { return v }\nfn store(doc serde_json.value, child serde_json.value) {\n doc.set(\"c\", id(child))?\n}\nfn main() {}\n");
    assert_eq!(r_forward_param.verdict, Verdict::Reject);
    assert!(codes(&r_forward_param).iter().any(|c| c == "HYB-E3004"));
}

#[test]
fn json_set_accepts_non_handle_fresh_read_and_managed_consumes() {
    let r_ok = check("import \"rust/serde_json\"\nstruct box { v serde_json.value }\nfn make_fresh(other serde_json.value) serde_json.value {\n _ = other\n return serde_json.new_object()\n}\nfn managed_store(doc serde_json.value, child serde_json.value) error {\n managed(\"json\", \"interop\") {\n doc.set(\"c\", child)?\n }\n return nil\n}\nfn main() {\n doc := serde_json.new_object()\n nums := []int{1, 2}\n doc.set(\"n\", nums[0])?\n doc.set(\"fresh\", serde_json.new_object())?\n other := serde_json.new_object()\n doc.set(\"from_fn\", make_fresh(other))?\n other.set(\"ok\", 1)?\n doc2 := serde_json.parse(`{\"a\": 1}`)?\n doc.set(\"read_int\", doc2.int(\"a\")?)?\n doc2.set(\"ok\", 2)?\n child := serde_json.new_object()\n b := box{v: child}\n doc.set(\"boxed\", b.v)?\n}\n");
    assert_eq!(r_ok.verdict, Verdict::Accept, "expected 0 diagnostics, got {:?}", r_ok.diagnostics);
}

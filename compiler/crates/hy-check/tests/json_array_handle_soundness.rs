//! json_array_handle_soundness.rs — regression tests for:
//! 1. `serde_json.new_array` (`OwnHandle("serde_json.value")` producer),
//!    `push_string` (`CallMode::Mutate`), and `v.set(key, child_handle)` /
//!    `serde_json.set(v, key, child_handle)` handle-value consumption (A7/O13
//!    `HYB-E3003` and A2 `HYB-E3101`).
//! 2. `const` value-struct immutability (T22 `HYB-E3006` and O5.1/O5.4
//!    `HYB-E3004`) across pointer-receiver method calls (`shared.inc()`),
//!    auto-addressed `*T` parameters (`bump(shared)`), and address-of borrows
//!    (`p := &shared`) inside closures and across rebinds.

use hy_check::{check_file, CheckResult, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("test.hy", src).expect("test snippet must parse");
    check_file(&ast)
}

fn codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

#[test]
fn push_string_during_live_borrow_loan_is_rejected_e3101() {
    let r_method = check(
        "import \"rust/serde_json\"\n\
         fn main() {\n\
             doc := serde_json.parse(`{\"items\": [1, 2]}`)?\n\
             for u in doc.array(\"items\")? {\n\
                 doc.push_string(\"mutated\")?\n\
                 fmt.println(u.int(\"k\")?)\n\
             }\n\
         }\n",
    );
    assert_eq!(r_method.verdict, Verdict::Reject, "got {:?}", r_method.diagnostics);
    assert!(codes(&r_method).iter().any(|c| c == "HYB-E3101"));

    let r_pkg = check(
        "import \"rust/serde_json\"\n\
         fn main() {\n\
             doc := serde_json.parse(`{\"items\": [1, 2]}`)?\n\
             items := doc.array(\"items\")?\n\
             serde_json.push_string(doc, \"mutated\")?\n\
             fmt.println(items.len())\n\
         }\n",
    );
    assert_eq!(r_pkg.verdict, Verdict::Reject, "got {:?}", r_pkg.diagnostics);
    assert!(codes(&r_pkg).iter().any(|c| c == "HYB-E3101"));
}

#[test]
fn new_array_alias_used_after_move_is_rejected_e3003() {
    let r = check(
        "import \"rust/serde_json\"\n\
         fn ship(v serde_json.value, done chan<- bool) {\n\
             v.push_string(\"worker\")\n\
             done <- true\n\
         }\n\
         fn main() {\n\
             arr := serde_json.new_array()\n\
             alias := arr\n\
             done := make(chan bool, 1)\n\
             task ship(move arr, done)\n\
             alias.push_string(\"main\")\n\
             <-done\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "got {:?}", r.diagnostics);
    assert!(codes(&r).iter().any(|c| c == "HYB-E3003"));
}

#[test]
fn new_array_unmoved_at_spawn_is_rejected_e3001_and_moved_cleanly_accepts() {
    let r_unmoved = check(
        "import \"rust/serde_json\"\n\
         fn ship(v serde_json.value) {}\n\
         fn main() {\n\
             arr := serde_json.new_array()\n\
             task ship(arr)\n\
         }\n",
    );
    assert_eq!(r_unmoved.verdict, Verdict::Reject, "got {:?}", r_unmoved.diagnostics);
    assert_eq!(codes(&r_unmoved), vec!["HYB-E3001"]);

    let r_moved = check(
        "import \"rust/serde_json\"\n\
         fn ship(v serde_json.value) {}\n\
         fn main() {\n\
             arr := serde_json.new_array()\n\
             arr.push_string(\"ok\")\n\
             task ship(move arr)\n\
         }\n",
    );
    assert_eq!(r_moved.verdict, Verdict::Accept, "got {:?}", r_moved.diagnostics);
}

#[test]
fn json_set_consumes_handle_value_argument_and_rejects_post_use_or_loan_conflict() {
    let r_post_use = check(
        "import \"rust/serde_json\"\n\
         fn main() {\n\
             v := serde_json.new_object()\n\
             arr := serde_json.new_array()\n\
             v.set(\"active\", arr)\n\
             arr.push_string(\"use-after-consume\")\n\
         }\n",
    );
    assert_eq!(r_post_use.verdict, Verdict::Reject, "got {:?}", r_post_use.diagnostics);
    assert!(codes(&r_post_use).iter().any(|c| c == "HYB-E3003"));
    assert_eq!(r_post_use.diagnostics[0].tier0_rule.as_deref(), Some("A7"));

    let r_pkg_post_use = check(
        "import \"rust/serde_json\"\n\
         fn main() {\n\
             v := serde_json.new_object()\n\
             child := serde_json.new_object()\n\
             serde_json.set(v, \"sub\", child)?\n\
             child.set(\"k\", 1)?\n\
         }\n",
    );
    assert_eq!(r_pkg_post_use.verdict, Verdict::Reject, "got {:?}", r_pkg_post_use.diagnostics);
    assert!(codes(&r_pkg_post_use).iter().any(|c| c == "HYB-E3003"));

    let r_loan = check(
        "import \"rust/serde_json\"\n\
         fn main() {\n\
             v := serde_json.new_object()\n\
             child := serde_json.parse(`{\"items\": [1, 2]}`)?\n\
             items := child.array(\"items\")?\n\
             v.set(\"sub\", child)?\n\
             fmt.println(items.len())\n\
         }\n",
    );
    assert_eq!(r_loan.verdict, Verdict::Reject, "got {:?}", r_loan.diagnostics);
    assert!(codes(&r_loan).iter().any(|c| c == "HYB-E3101"));
}

#[test]
fn const_value_struct_pointer_receiver_mutation_is_rejected_e3006() {
    let r = check(
        "struct counter { n int }\n\
         const shared = counter{n: 0}\n\
         fn (c *counter) inc() { c.n = c.n + 1 }\n\
         fn worker(done chan<- bool) {\n\
             shared.inc()\n\
             done <- true\n\
         }\n\
         fn main() {\n\
             done := make(chan bool, 1)\n\
             task worker(done)\n\
             shared.inc()\n\
             <-done\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "got {:?}", r.diagnostics);
    assert!(codes(&r).iter().any(|c| c == "HYB-E3006"));

    // Read-only pointer receiver on `const` value struct remains accepted.
    let r_ro = check(
        "import \"fmt\"\n\
         struct counter { n int }\n\
         const shared = counter{n: 7}\n\
         fn (c *counter) get() int { return c.n }\n\
         fn main() {\n\
             fmt.println(shared.get())\n\
         }\n",
    );
    assert_eq!(r_ro.verdict, Verdict::Accept, "got {:?}", r_ro.diagnostics);
}

#[test]
fn const_value_struct_auto_addressed_pointer_param_mutation_is_rejected_e3006() {
    let r = check(
        "struct counter { n int }\n\
         const shared = counter{n: 0}\n\
         fn bump(c *counter) { c.n = c.n + 1 }\n\
         fn main() {\n\
             bump(shared)\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "got {:?}", r.diagnostics);
    assert_eq!(codes(&r), vec!["HYB-E3006"]);
}

#[test]
fn const_value_struct_addr_of_borrow_in_closure_and_rebind_is_rejected() {
    let r_closure = check(
        "struct counter { n int }\n\
         const shared = counter{n: 0}\n\
         fn main() {\n\
             f := fn() {\n\
                 p := &shared\n\
                 p.n = 1\n\
             }\n\
             f()\n\
         }\n",
    );
    assert_eq!(r_closure.verdict, Verdict::Reject, "got {:?}", r_closure.diagnostics);
    assert_eq!(codes(&r_closure), vec!["HYB-E3006"]);

    let r_rebind_move = check(
        "struct counter { n int }\n\
         const shared = counter{n: 0}\n\
         fn worker(p *counter) { p.n = 1 }\n\
         fn main() {\n\
             other := counter{n: 1}\n\
             p := &shared\n\
             p2 := p\n\
             p = &other\n\
             task worker(move p2)\n\
         }\n",
    );
    assert_eq!(r_rebind_move.verdict, Verdict::Reject, "got {:?}", r_rebind_move.diagnostics);
    assert!(codes(&r_rebind_move).iter().any(|c| c == "HYB-E3004"));
}

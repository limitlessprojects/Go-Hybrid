//! Regression tests for issue #1683:
//! In-place `append(dst, elem...)` into a slice with spare capacity (`s[:1]`,
//! `make([]T, 1, 2)`, or a helper parameter `dst`) writes `elem` into `dst`'s
//! backing array at runtime. Ensure `hy-check` records `dst <-> elem` sharing,
//! `non_owned_origin`, `borrow_bindings`, `LoanTarget::MapSlot` window
//! extensions, interprocedural parameter stores, multi-return sharing, and
//! `const` alias stores across direct, closure, and helper call sites.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check_codes(src: &str) -> (Verdict, Vec<String>) {
    let ast = parse_file("issue_1683.hy", src).expect("test program must parse");
    let res = check_file(&ast);
    let codes = res.diagnostics.iter().map(|d| d.code.clone()).collect();
    (res.verdict, codes)
}

#[test]
fn issue_1683_blank_target_inplace_append_rejects_post_move_alias_write() {
    let src = r#"package main

fn worker(m map[string]int, done chan<- bool) {
    for i in 0..10 {
        m["k"] = i
    }
    done <- true
}

fn main() {
    dummy1 := map[string]int{"k": 1}
    dummy2 := map[string]int{"k": 2}
    s := []map[string]int{dummy1, dummy2}
    real_m := map[string]int{"k": 0}
    _ = append(s[:1], real_m)
    done := make(chan bool, 1)
    task worker(move real_m, done)
    s[1]["k"] = 99
    <-done
}
"#;
    let (verdict, codes) = check_codes(src);
    assert_eq!(verdict, Verdict::Reject);
    assert!(
        codes.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 for post-move write through s[1] after _ = append(s[:1], real_m), got {:?}",
        codes
    );
}

#[test]
fn issue_1683_helper_param_store_via_append_rejects_post_move_alias_write() {
    let src = r#"package main

fn put(dst []map[string]int, m map[string]int) {
    dst = append(dst, m)
}

fn worker(m map[string]int, done chan<- bool) {
    m["k"] = 1
    done <- true
}

fn main() {
    dummy1 := map[string]int{"k": 1}
    dummy2 := map[string]int{"k": 2}
    s := []map[string]int{dummy1, dummy2}
    real_m := map[string]int{"k": 0}
    put(s[:1], real_m)
    done := make(chan bool, 1)
    task worker(move real_m, done)
    s[1]["k"] = 99
    <-done
}
"#;
    let (verdict, codes) = check_codes(src);
    assert_eq!(verdict, Verdict::Reject);
    assert!(
        codes.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 for interprocedural helper put(s[:1], real_m), got {:?}",
        codes
    );
}

#[test]
fn issue_1683_non_owned_param_laundered_via_inplace_append_rejects_move() {
    let src = r#"package main

fn worker(box []map[string]int, done chan<- bool) {
    box[1]["k"] = 1
    done <- true
}

fn spawn_with_param(param_m map[string]int, done chan<- bool) {
    dummy1 := map[string]int{"k": 1}
    dummy2 := map[string]int{"k": 2}
    s := []map[string]int{dummy1, dummy2}
    _ = append(s[:1], param_m)
    task worker(move s, done)
}

fn main() {
    caller_m := map[string]int{"k": 0}
    done := make(chan bool, 1)
    spawn_with_param(caller_m, done)
    caller_m["k"] = 2
    <-done
}
"#;
    let (verdict, codes) = check_codes(src);
    assert_eq!(verdict, Verdict::Reject);
    assert!(
        codes.iter().any(|c| c == "HYB-E3004"),
        "expected HYB-E3004 when moving s after _ = append(s[:1], param_m), got {:?}",
        codes
    );
}

#[test]
fn issue_1683_multi_return_pair_via_inplace_append_rejects_post_move_write() {
    let src = r#"package main

fn pair_via_append() ([]map[string]int, map[string]int) {
    dummy1 := map[string]int{"k": 1}
    dummy2 := map[string]int{"k": 2}
    s := []map[string]int{dummy1, dummy2}
    real_m := map[string]int{"k": 0}
    _ = append(s[:1], real_m)
    return s, real_m
}

fn worker(m map[string]int, done chan<- bool) {
    m["k"] = 1
    done <- true
}

fn main() {
    s, m := pair_via_append()
    done := make(chan bool, 1)
    task worker(move m, done)
    s[1]["k"] = 99
    <-done
}
"#;
    let (verdict, codes) = check_codes(src);
    assert_eq!(verdict, Verdict::Reject);
    assert!(
        codes.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 for multi-return pair sharing via _ = append(s[:1], real_m), got {:?}",
        codes
    );
}

#[test]
fn issue_1683_const_elem_stored_via_inplace_append_rejects_mutation() {
    let src = r#"package main

const cm = map[string]int{"k": 42}

fn mutate_via_append(m map[string]int) {
    dummy1 := map[string]int{"k": 1}
    dummy2 := map[string]int{"k": 2}
    s := []map[string]int{dummy1, dummy2}
    _ = append(s[:1], m)
    s[1]["k"] = 99
}

fn main() {
    dummy1 := map[string]int{"k": 1}
    dummy2 := map[string]int{"k": 2}
    s := []map[string]int{dummy1, dummy2}
    _ = append(s[:1], cm)
    s[1]["k"] = 99
    mutate_via_append(cm)
}
"#;
    let (verdict, codes) = check_codes(src);
    assert_eq!(verdict, Verdict::Reject);
    let e3006_count = codes.iter().filter(|c| c.as_str() == "HYB-E3006").count();
    assert!(
        e3006_count >= 2,
        "expected at least 2 HYB-E3006 diagnostics (direct + interprocedural), got {:?}",
        codes
    );
}

#[test]
fn issue_1683_map_slot_loan_stored_via_inplace_append_enforces_a3_and_o2() {
    let src = r#"package main

fn escape_slot() []*int {
    m := map[string]int{"a": 1}
    s := make([]*int, 2)
    p := &m["a"]
    _ = append(s[:1], p)
    m["b"] = 2
    return s
}

fn main() {
    s := escape_slot()
    _ = s
}
"#;
    let (verdict, codes) = check_codes(src);
    assert_eq!(verdict, Verdict::Reject);
    assert!(
        codes.iter().any(|c| c == "HYB-E3102"),
        "expected HYB-E3102 when m is mutated while slot loan p lives in s, got {:?}",
        codes
    );
    assert!(
        codes.iter().any(|c| c == "HYB-E3104"),
        "expected HYB-E3104 when s carrying slot loan p is returned, got {:?}",
        codes
    );
}

#[test]
fn issue_1683_positive_controls_accept_safe_append_forms() {
    let src = r#"package main

const cm = map[string]int{"k": 42}

fn worker(m map[string]int, done chan<- bool) {
    m["k"] = 7
    done <- true
}

fn main() {
    // 1. Appending a fresh slice literal []map[string]int{a} does not mutate a in place.
    a := map[string]int{"k": 1}
    b := map[string]int{"k": 2}
    _ = append([]map[string]int{a}, b)
    done := make(chan bool, 1)
    task worker(move b, done)
    a["k"] = 3
    <-done

    // 2. Appending const map cm into s[:1] and only reading s[1]["k"] is legal.
    d1 := map[string]int{"k": 1}
    d2 := map[string]int{"k": 2}
    s := []map[string]int{d1, d2}
    _ = append(s[:1], cm)
    println(s[1]["k"])
}
"#;
    let (verdict, codes) = check_codes(src);
    assert_eq!(
        verdict,
        Verdict::Accept,
        "expected positive controls to be accepted with 0 diagnostics, got {:?}",
        codes
    );
    assert!(codes.is_empty());
}

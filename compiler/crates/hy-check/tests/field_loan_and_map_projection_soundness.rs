//! Regression tests for O8 field-loan T24 guards, call-backed map-slot loans,
//! unaddressable map-slot projections in operand positions, and slot-loan
//! alias rebinds / match-guard borrows (HYB-E3104).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn assert_rejects_e3104(src: &str) {
    let ast = parse_file("test.hy", src).expect("test program must parse");
    let res = check_file(&ast);
    assert_eq!(
        res.verdict,
        Verdict::Reject,
        "expected Reject for:\n{src}\ngot: {:?}",
        res.diagnostics
    );
    assert!(
        res.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error && d.code == "HYB-E3104"),
        "expected HYB-E3104 for:\n{src}\ngot: {:?}",
        res.diagnostics
    );
}

fn assert_accepts(src: &str) {
    let ast = parse_file("test.hy", src).expect("test program must parse");
    let res = check_file(&ast);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "expected Accept for:\n{src}\ngot: {:?}",
        res.diagnostics
    );
    assert!(res.diagnostics.is_empty(), "got: {:?}", res.diagnostics);
}

#[test]
fn field_loan_addr_of_move_and_pointer_rebind_reject_e3104() {
    for body in [
        "n := &h.a\n    p := &n\n    _ = p",
        "n := &h.a\n    p := &n.bal\n    _ = p",
        "n := &h.a\n    p := &n.arr[0]\n    _ = p",
        "n := &h.a\n    n = &acct{bal: 2, arr: [1, 2]}\n    _ = n.bal",
        "n := &h.a\n    n = nil\n    _ = n",
        "n := &h.a\n    n, tag := &acct{bal: 2, arr: [1, 2]}, 1\n    _ = n.bal + tag",
        "n := &h.a\n    var ok bool\n    n, ok = next_acct()\n    _ = ok",
        "var slot *acct\n    slot = &h.a\n    task worker(move slot)",
        "var slot *acct\n    slot = &h.a\n    p := slot\n    task worker(move p)",
        "var slot *acct\n    slot = &h.a\n    ch <- move slot",
        "f := fn() {\n        n := &h.a\n        n = &acct{bal: 9, arr: [1, 2]}\n        _ = n.bal\n    }\n    f()",
        "f := fn() {\n        var s *acct\n        s = &h.a\n        select {\n        ch <- move s:\n            {}\n        default:\n            {}\n        }\n    }\n    f()",
    ] {
        let src = format!(
            "struct acct {{\n    bal int\n    arr [2]int\n}}\n\
             struct holder {{ a acct }}\n\
             fn next_acct() (*acct, bool) {{\n    a := acct{{bal: 5, arr: [1, 2]}}\n    return &a, true\n}}\n\
             fn worker(p *acct) {{ p.bal = 9 }}\n\
             fn main() {{\n\
                 h := holder{{a: acct{{bal: 1, arr: [1, 2]}}}}\n\
                 ch := make(chan *acct, 1)\n\
                 {body}\n\
                 _ = h\n\
                 _ = ch\n\
             }}\n"
        );
        assert_rejects_e3104(&src);
    }
}

#[test]
fn call_returned_map_slot_loan_enforces_t24_e3104() {
    for body in [
        "n := &get_map()[\"a\"]\n    p := &n\n    _ = p",
        "n := &get_map()[\"a\"]\n    p := &n.bal\n    _ = p",
        "n := &get_map()[\"a\"]\n    n = &acct{bal: 2}\n    _ = n.bal",
        "var slot *acct\n    slot = &get_map()[\"a\"]\n    task worker(move slot)",
    ] {
        let src = format!(
            "struct acct {{ bal int }}\n\
             fn get_map() map[string]acct {{ return map[string]acct{{\"a\": acct{{bal: 1}}}} }}\n\
             fn worker(p *acct) {{ p.bal = 9 }}\n\
             fn main() {{\n\
                 {body}\n\
             }}\n"
        );
        assert_rejects_e3104(&src);
    }
}

#[test]
fn unaddressable_map_slot_projections_in_operand_positions_reject_e3104() {
    for body in [
        "touch_int(&m[\"a\"].bal)",
        "touch_int(&m[\"a\"].sub.x)",
        "touch_int(&m[\"a\"].arr[0])",
        "touch_int(&arr_map[\"a\"][0])",
        "xs := []*int{&m[\"a\"].bal}\n    _ = xs",
        "ch <- &m[\"a\"].bal",
        "task worker_int(&m[\"a\"].bal)",
        "if &m[\"a\"].bal != nil {}",
        "var p *int = &m[\"a\"].bal\n    _ = p",
    ] {
        let src = format!(
            "struct inner {{ x int }}\n\
             struct acct {{\n    bal int\n    sub inner\n    arr [2]int\n}}\n\
             fn touch_int(p *int) {{ _ = p }}\n\
             fn worker_int(p *int) {{ _ = p }}\n\
             fn main() {{\n\
                 m := map[string]acct{{\"a\": acct{{bal: 1, sub: inner{{x: 2}}, arr: [3, 4]}}}}\n\
                 arr_map := map[string][2]int{{\"a\": [5, 6]}}\n\
                 ch := make(chan *int, 1)\n\
                 {body}\n\
                 _ = m\n\
                 _ = arr_map\n\
                 _ = ch\n\
             }}\n"
        );
        assert_rejects_e3104(&src);
    }
}

#[test]
fn slot_loan_rebind_to_loan_ident_and_match_guard_addr_of_reject_e3104() {
    for body in [
        "q := &m[\"a\"]\n    r := &m[\"b\"]\n    q = r\n    _ = q.bal",
        "q := &m[\"a\"]\n    r := q\n    q = r\n    _ = q.bal",
        "q := &m[\"a\"]\n    q, tag := q, 1\n    _ = q.bal + tag",
        "q := &m[\"a\"]\n    match some(q) {\n    some(b) if &b != nil:\n        {}\n    none:\n        {}\n    }",
        "q := &m[\"a\"]\n    match some(q) {\n    some(b) if &b.bal != nil:\n        {}\n    none:\n        {}\n    }",
    ] {
        let src = format!(
            "struct acct {{ bal int }}\n\
             fn main() {{\n\
                 m := map[string]acct{{\"a\": acct{{bal: 1}}, \"b\": acct{{bal: 2}}}}\n\
                 {body}\n\
             }}\n"
        );
        assert_rejects_e3104(&src);
    }
}

#[test]
fn valid_field_loans_reloans_and_pointer_or_slice_map_projections_accept() {
    assert_accepts(
        r#"struct acct { bal int }
struct holder {
    a acct
    b acct
}
fn touch_int(p *int) { _ = p }
fn main() {
    h := holder{a: acct{bal: 1}, b: acct{bal: 2}}
    n := &h.a
    n.bal = 10
    n = &h.b
    n.bal = 30

    m := map[string]acct{"k": acct{bal: 3}}
    n = &m["k"]
    n.bal = 40

    a := acct{bal: 5}
    ptr_map := map[string]*acct{"k": &a}
    touch_int(&ptr_map["k"].bal)

    slice_map := map[string][]int{"k": []int{7, 8}}
    touch_int(&slice_map["k"][0])
}
"#,
    );
}

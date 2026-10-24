//! slot_loan_callarg_positions.rs — O3/O8 position rule (HYB-E3104): a map-slot
//! borrow `&m[k]` is a loan only where it is the WHOLE value of a binding
//! (`n := &m[k]`, `n = &m[k]`, parens transparent), because that is the shape
//! the emitter turns into copy-out / store-back. Everywhere else the borrow is
//! emitted as written and Go refuses it:
//!
//!   invalid operation: cannot take address of m["a"] (map index expression)
//!
//! so the checker rejects those positions with HYB-E3104 (tier-0 rule
//! LOAN-ESCAPE). The rule is scoped to MAP slots: slice and fixed-array
//! elements are addressable, keep their lowering, and are covered by the
//! separate slice-borrow soundness track.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_e3104(src: &str, what: &str) {
    let r = check(src);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3104"),
        "{}: expected HYB-E3104, got {:?}",
        what,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "{}", what);
}

fn accept(src: &str, what: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "{}: got {:?}", what, codes(&r));
}

#[test]
fn map_field_slot_borrow_as_call_arg_is_e3104() {
    assert_e3104(
        "struct service { replicas int }\n\
         struct env { services map[string]service }\n\
         fn touch(s *service) { s.replicas = 9 }\n\
         fn main() {\n\
             e := env{services: map[string]service{\"a\": service{replicas: 1}}}\n\
             touch(&e.services[\"a\"])\n\
             v := e.services[\"a\"].replicas\n\
             fmt.println(\"r {v}\")\n\
         }\n",
        "&e.services[\"a\"] in argument position",
    );
}

#[test]
fn local_map_slot_borrow_as_call_arg_is_e3104() {
    assert_e3104(
        "struct acct { bal int }\n\
         fn touch(p *acct) { p.bal = 9 }\n\
         fn main() {\n\
             m := map[string]acct{\"a\": acct{bal: 1}}\n\
             touch(&m[\"a\"])\n\
             v := m[\"a\"].bal\n\
             fmt.println(\"r {v}\")\n\
         }\n",
        "&m[\"a\"] in argument position",
    );
}

#[test]
fn call_arg_borrow_while_loan_is_live_is_e3104() {
    assert_e3104(
        "struct acct { bal int }\n\
         fn touch(p *acct) { p.bal = 9 }\n\
         fn main() {\n\
             m := map[string]acct{\"a\": acct{bal: 1}}\n\
             a := &m[\"a\"]\n\
             touch(&m[\"a\"])\n\
             a.bal = a.bal + 1\n\
             fmt.println(\"r {a.bal}\")\n\
         }\n",
        "argument borrow on a slot that already has a live loan",
    );
}

#[test]
fn second_operand_position_borrow_is_e3104() {
    assert_e3104(
        "struct acct { bal int }\n\
         fn touch(p *acct) { p.bal = 9 }\n\
         fn main() {\n\
             m := map[string]acct{\"a\": acct{bal: 1}}\n\
             touch(&m[\"a\"])\n\
             m[\"a\"].bal = 2\n\
             v := m[\"a\"].bal\n\
             fmt.println(\"v {v}\")\n\
         }\n",
        "argument borrow followed by a base field store",
    );
}

#[test]
fn legal_map_slot_loan_bindings_stay_accepted() {
    accept(
        "struct graph {\n\
             nodes map[int]int\n\
         }\n\
         fn main() {\n\
             g := graph{nodes: map[int]int{1: 10}}\n\
             n := &g.nodes[1]\n\
             fmt.println(\"n {n}\")\n\
         }\n",
        "n := &g.nodes[1] (the binding initializer is the slot-loan position)",
    );
    accept(
        "type int_ptr = *int\n\
         fn main() (out int_ptr) {\n\
             m := map[int]int{1: 10}\n\
             out = &m[1]\n\
             fmt.println(\"o {out}\")\n\
         }\n",
        "out = &m[1] (a re-binding assignment is the slot-loan position)",
    );
    accept(
        "struct acct { bal int }\n\
         fn main() {\n\
             m := map[string]acct{\"a\": acct{bal: 1}}\n\
             p := (&m[\"a\"])\n\
             p.bal = 7\n\
             v := m[\"a\"].bal\n\
             fmt.println(\"v {v}\")\n\
         }\n",
        "parenthesised binding initializer `p := (&m[\"a\"])`",
    );
}

#[test]
fn corrected_call_arg_pattern_is_accepted() {
    accept(
        "struct acct { bal int }\n\
         fn touch(p *acct) { p.bal = 7 }\n\
         fn main() {\n\
             m := map[string]acct{\"a\": acct{bal: 1}}\n\
             tmp := m[\"a\"]\n\
             touch(&tmp)\n\
             fmt.println(\"t {tmp.bal}\")\n\
         }\n",
        "copy the slot out (`tmp := m[\"a\"]`) then pass `&tmp`",
    );
}

#[test]
fn slice_and_array_element_borrows_keep_their_lowering() {
    accept(
        "struct acct { bal int }\n\
         fn touch(p *acct) { p.bal = 9 }\n\
         fn main() {\n\
             a := [2]acct{acct{bal: 1}, acct{bal: 2}}\n\
             touch(&a[0])\n\
             fmt.println(\"a0 {a[0].bal}\")\n\
         }\n",
        "array element borrow as argument (addressable, no loan lowering needed)",
    );
}

#[test]
fn non_lowered_map_slot_borrow_positions_rejected_issue_1423() {
    // 1. `var` declaration (`LocalKind::Var`)
    assert_e3104(
        "struct acct { bal int }\n\
         fn main() {\n\
             m := map[string]acct{\"a\": acct{bal: 1}}\n\
             var p = &m[\"a\"]\n\
             fmt.println(p.bal)\n\
         }\n",
        "var declaration cannot borrow map slot",
    );

    // 2. Non-identifier assignment target (`PlaceKind::Index`)
    assert_e3104(
        "struct acct { bal int }\n\
         fn main() {\n\
             m := map[string]acct{\"a\": acct{bal: 1}}\n\
             ptrs := make([]*acct, 1)\n\
             ptrs[0] = &m[\"a\"]\n\
             fmt.println(ptrs[0].bal)\n\
         }\n",
        "index assignment target cannot borrow map slot",
    );

    // 3. Mixed-target parallel assignment (`PlaceKind::Ident` + `PlaceKind::Index`)
    assert_e3104(
        "struct acct { bal int }\n\
         fn main() {\n\
             m := map[string]acct{\"a\": acct{bal: 1}}\n\
             var q acct = acct{bal: 0}\n\
             arr := []int{0}\n\
             q, arr[0] = &m[\"a\"], 1\n\
             fmt.println(q.bal, arr[0])\n\
         }\n",
        "mixed-target parallel assignment cannot borrow map slot",
    );

    // 4. `any`-typed struct-literal field
    assert_e3104(
        "struct acct { bal int }\n\
         struct box { p any }\n\
         fn main() {\n\
             m := map[string]acct{\"a\": acct{bal: 1}}\n\
             b := box{p: &m[\"a\"]}\n\
             fmt.println(b.p)\n\
         }\n",
        "any-typed struct-literal field cannot borrow map slot",
    );
}

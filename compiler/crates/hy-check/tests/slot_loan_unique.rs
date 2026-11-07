//! slot_loan_unique.rs — O8 uniqueness (HYB-E3102): a live slot loan
//! `p := &m[k]` is the only access path to its slot. The emitter lowers the
//! loan to copy-out / store-back, so a second loan on the same slot, or a
//! field store `m[k].f = v` through the base, would be silently overwritten
//! by (or overwrite) the loan's store-back. Loans on provably distinct
//! slots (different literal keys), and accesses after the loan's last use,
//! stay legal.

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

fn assert_e3102(body: &str, what: &str) {
    let src = format!("struct acct {{ bal int }}\n\nfn main() {{\n{}}}\n", body);
    let r = check(&src);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3102"),
        "{}: expected HYB-E3102, got {:?}",
        what,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "{}", what);
}

#[test]
fn second_loan_on_same_map_slot_is_e3102() {
    assert_e3102(
        "    m := map[string]acct{\"a\": acct{bal: 1}}\n    p := &m[\"a\"]\n    q := &m[\"a\"]\n    q.bal = 50\n    p.bal = p.bal + 1\n",
        "two loans on m[\"a\"]",
    );
}

#[test]
fn second_loan_on_same_slice_element_is_e3102() {
    assert_e3102(
        "    ns := []acct{acct{bal: 1}}\n    p := &ns[0]\n    q := &ns[0]\n    q.bal = 50\n    p.bal = p.bal + 1\n",
        "two loans on ns[0]",
    );
}

#[test]
fn parallel_loans_with_maybe_equal_keys_is_e3102() {
    assert_e3102(
        "    m := map[string]acct{\"a\": acct{bal: 1}}\n    k1 := \"a\"\n    k2 := \"a\"\n    p, q := &m[k1], &m[k2]\n    q.bal = 50\n    p.bal = p.bal + 1\n",
        "non-literal keys may name the same slot",
    );
}

#[test]
fn second_loan_through_shallow_alias_is_e3102() {
    assert_e3102(
        "    m := map[string]acct{\"a\": acct{bal: 1}}\n    alias := m\n    p := &m[\"a\"]\n    q := &alias[\"a\"]\n    q.bal = 50\n    p.bal = p.bal + 1\n",
        "loan through a may-share alias",
    );
}

#[test]
fn field_store_through_base_inside_window_is_e3102() {
    assert_e3102(
        "    ns := []acct{acct{bal: 1}}\n    p := &ns[0]\n    ns[0].bal = 50\n    p.bal = p.bal + 1\n",
        "ns[0].bal store while p is live",
    );
}

#[test]
fn distinct_slots_and_closed_windows_stay_legal() {
    let src = "struct acct { bal int }\n\nfn main() {\n    m := map[string]acct{\"a\": acct{bal: 1}, \"b\": acct{bal: 2}}\n    p := &m[\"a\"]\n    q := &m[\"b\"]\n    q.bal = 50\n    p.bal = p.bal + 1\n    ns := []acct{acct{bal: 1}, acct{bal: 2}}\n    x, y := &ns[0], &ns[1]\n    x.bal = 7\n    y.bal = 8\n    ns[1].bal = 9\n    r := &m[\"a\"]\n    r.bal = 3\n    s := &m[\"a\"]\n    s.bal = s.bal + 1\n    ns[0].bal = 4\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "got {:?}", codes(&r));
}

#[test]
fn issue_2226_second_loan_differently_spelled_integer_literals_is_e3102() {
    assert_e3102(
        "    m := map[int]acct{10: acct{bal: 1}}\n    p := &m[10]\n    q := &m[0xA]\n    q.bal = 50\n    p.bal = p.bal + 1\n",
        "differently spelled hex int literal 0xA vs 10 on map slot",
    );
    assert_e3102(
        "    m := map[int]acct{10: acct{bal: 1}}\n    p := &m[10]\n    q := &m[0b1010]\n    q.bal = 50\n    p.bal = p.bal + 1\n",
        "differently spelled binary int literal 0b1010 vs 10 on map slot",
    );
    assert_e3102(
        "    m := map[int]acct{10: acct{bal: 1}}\n    p := &m[10]\n    q := &m[0o12]\n    q.bal = 50\n    p.bal = p.bal + 1\n",
        "differently spelled octal int literal 0o12 vs 10 on map slot",
    );
    assert_e3102(
        "    m := map[int]acct{10: acct{bal: 1}}\n    p := &m[10]\n    q := &m[1_0]\n    q.bal = 50\n    p.bal = p.bal + 1\n",
        "differently spelled underscore int literal 1_0 vs 10 on map slot",
    );
    assert_e3102(
        "    m := map[int]acct{10: acct{bal: 1}}\n    p := &m[10]\n    q := &m['\\n']\n    q.bal = 50\n    p.bal = p.bal + 1\n",
        "differently spelled rune literal '\\n' vs 10 on map slot",
    );
    assert_e3102(
        "    ns := []acct{acct{bal: 1}, acct{bal: 2}}\n    p := &ns[1]\n    q := &ns[0x1]\n    q.bal = 50\n    p.bal = p.bal + 1\n",
        "differently spelled hex int literal 0x1 vs 1 on slice slot",
    );
}

#[test]
fn issue_2226_field_store_differently_spelled_integer_literal_is_e3102() {
    assert_e3102(
        "    ns := []acct{acct{bal: 1}, acct{bal: 2}}\n    p := &ns[1]\n    ns[0x1].bal = 50\n    p.bal = p.bal + 1\n",
        "field store with 0x1 while slice slot loan on 1 is live",
    );
}

//! map_element_addressable.rs — accepted programs must not store into part
//! of a Go map element, and slot loans re-bound into pointer variables must
//! stay buildable.
//!
//! Go map elements are not addressable: `m[k].f = v`, `m[k].f++`,
//! `m[k][i] = v` on an array element, and the store-back of
//! `n := &m[k].f` all fail `go build`. They passed `hyb check` with 0
//! diagnostics; they now reject as HYB-E2010 (rule BUILDABLE). The controls
//! pin the shapes Go accepts (reference-typed hops, slices, pointers,
//! shadows) so the rule cannot over-reject.
//!
//! A slot loan assigned into a pointer-typed variable (`var q *acct;
//! q = &m[k]`) is accepted, and its lowering is pinned in hy-emit. The
//! reverse — a live loan re-bound to a general pointer — is HYB-E3104.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn rejects_with(src: &str, code: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == code),
        "expected {}, got {:?}\nsrc:\n{}",
        code,
        cs,
        src
    );
    assert_eq!(r.verdict, Verdict::Reject, "src:\n{}", src);
}

fn accepts(src: &str) {
    let r = check(src);
    assert!(
        error_codes(&r).is_empty() && r.verdict == Verdict::Accept,
        "must accept, got {:?}\nsrc:\n{}",
        r.diagnostics,
        src
    );
}

// ---------------------------------------------------------------------------
// Rejects: a store into part of a map element (HYB-E2010)
// ---------------------------------------------------------------------------

/// `m[k].f = v`
#[test]
fn rejects_field_store() {
    rejects_with(
        r#"struct acct {
    bal int
}

fn main() {
    m := map[string]acct{"a": acct{bal: 1}}
    m["a"].bal = 9
    fmt.println("{m["a"].bal}")
}
"#,
        "HYB-E2010",
    );
}

/// `m[k].f += v`
#[test]
fn rejects_compound_store() {
    rejects_with(
        r#"struct acct {
    bal int
}

fn main() {
    m := map[string]acct{"a": acct{bal: 1}}
    m["a"].bal += 1
    fmt.println("{m["a"].bal}")
}
"#,
        "HYB-E2010",
    );
}

/// `m[k].f++`
#[test]
fn rejects_inc_dec() {
    rejects_with(
        r#"struct acct {
    bal int
}

fn main() {
    m := map[string]acct{"a": acct{bal: 1}}
    m["a"].bal++
    fmt.println("{m["a"].bal}")
}
"#,
        "HYB-E2010",
    );
}

/// `q := &m[k].f` — the store-back `m[k].f = q` cannot build
#[test]
fn rejects_field_loan() {
    rejects_with(
        r#"struct acct {
    bal int
}

fn main() {
    m := map[string]acct{"a": acct{bal: 1}}
    q := &m["a"].bal
    fmt.println("{q}")
}
"#,
        "HYB-E2010",
    );
}

/// `m[k][i] = v` on a map of arrays
#[test]
fn rejects_array_elem_store() {
    rejects_with(
        r#"fn main() {
    var arr [2]int
    m := map[string][2]int{}
    m["a"] = arr
    m["a"][0] = 7
    fmt.println("{m["a"][0]}")
}
"#,
        "HYB-E2010",
    );
}

/// `m[k].arr[i] = v`
#[test]
fn rejects_nested_array_store() {
    rejects_with(
        r#"struct acct {
    arr [2]int
}

fn main() {
    m := map[string]acct{"a": acct{}}
    m["a"].arr[0] = 3
    fmt.println("{m["a"].arr[0]}")
}
"#,
        "HYB-E2010",
    );
}

/// `b.accts[k].in.n = v` through a receiver
#[test]
fn rejects_method_field_chain() {
    rejects_with(
        r#"struct inner {
    n int
}

struct acct {
    in inner
}

struct bank {
    accts map[string]acct
}

fn (b *bank) bump(k string) {
    b.accts[k].in.n = 3
}

fn main() {
    b := bank{accts: map[string]acct{"a": acct{in: inner{n: 1}}}}
    b.bump("a")
    fmt.println("{b.accts["a"].in.n}")
}
"#,
        "HYB-E2010",
    );
}

/// the store sits inside a closure body
#[test]
fn rejects_closure_body() {
    rejects_with(
        r#"struct acct {
    bal int
}

fn main() {
    m := map[string]acct{"a": acct{bal: 1}}
    f := fn() {
        m["a"].bal = 3
    }
    f()
    fmt.println("{m["a"].bal}")
}
"#,
        "HYB-E2010",
    );
}

/// closure-local maps, closure-parameter maps, loop/select/multi-value bound maps,
/// and untyped `var m; m = map[...]` inside closure bodies must also enforce
/// HYB-E2010, HYB-E2006, and HYB-E3104.
#[test]
fn rejects_closure_local_param_and_multival_map_element_store_and_loan() {
    // 1. Closure-local map field store (`m["a"].bal = 3`) -> HYB-E2010.
    rejects_with(
        r#"struct acct {
    bal int
}

fn main() {
    f := fn() {
        m := map[string]acct{"a": acct{bal: 1}}
        m["a"].bal = 3
        fmt.println("{m["a"].bal}")
    }
    f()
}
"#,
        "HYB-E2010",
    );

    // 2. Closure-local map field loan (`q := &m["a"].bal`) -> HYB-E2010.
    rejects_with(
        r#"struct acct {
    bal int
}

fn main() {
    f := fn() {
        m := map[string]acct{"a": acct{bal: 1}}
        q := &m["a"].bal
        fmt.println("{q}")
    }
    f()
}
"#,
        "HYB-E2010",
    );

    // 3. Closure parameter map field store (`fn(m map[string]acct) { m["a"].bal = 3 }`) -> HYB-E2010.
    rejects_with(
        r#"struct acct {
    bal int
}

fn main() {
    f := fn(m map[string]acct) {
        m["a"].bal = 3
    }
    f(map[string]acct{"a": acct{bal: 1}})
}
"#,
        "HYB-E2010",
    );

    // 4. Multi-value short declaration inside a closure body (`m, ok := make_map()`)
    // must bind `m`'s map shape in `cst`, rejecting `m["a"].bal = 3` (HYB-E2010),
    // `m == m` (HYB-E2006), and `take(&m["a"])` (HYB-E3104).
    rejects_with(
        r#"struct acct {
    bal int
}

fn make_map() (map[string]acct, bool) {
    return map[string]acct{"a": acct{bal: 1}}, true
}

fn main() {
    f := fn() {
        m, ok := make_map()
        m["a"].bal = 3
        fmt.println("{m["a"].bal} {ok}")
    }
    f()
}
"#,
        "HYB-E2010",
    );

    rejects_with(
        r#"struct acct {
    bal int
}

fn make_map() (map[string]acct, bool) {
    return map[string]acct{"a": acct{bal: 1}}, true
}

fn main() {
    f := fn() {
        m, ok := make_map()
        same := m == m
        fmt.println("{same} {ok}")
    }
    f()
}
"#,
        "HYB-E2006",
    );

    rejects_with(
        r#"struct acct {
    bal int
}

fn make_map() (map[string]acct, bool) {
    return map[string]acct{"a": acct{bal: 1}}, true
}

fn take(p *acct) {
    fmt.println("{p.bal}")
}

fn main() {
    f := fn() {
        m, ok := make_map()
        take(&m["a"])
        fmt.println("{ok}")
    }
    f()
}
"#,
        "HYB-E3104",
    );
}

// ---------------------------------------------------------------------------
// Controls: Go-addressable shapes stay accepted
// ---------------------------------------------------------------------------

/// a slice/map FIELD of a map element is a reference: `m[k].sl[0] = v`, `m[k].mp[x] = v` build
#[test]
fn accepts_slice_and_map_fields() {
    accepts(
        r#"struct acct {
    sl []int
    mp map[string]int
}

fn main() {
    m := map[string]acct{"a": acct{sl: []int{1}, mp: map[string]int{}}}
    m["a"].sl[0] = 3
    m["a"].mp["x"] = 4
    fmt.println("{m["a"].sl[0]} {m["a"].mp["x"]}")
}
"#,
    );
}

/// slice elements, local array elements, and a slice inside a map are addressable
#[test]
fn accepts_slice_and_array_elems() {
    accepts(
        r#"struct acct {
    bal int
}

fn main() {
    s := []acct{acct{bal: 1}}
    s[0].bal = 4
    var arr [1]acct
    arr[0].bal = 5
    mp := map[string][]acct{"a": []acct{acct{bal: 1}}}
    mp["a"][0].bal = 6
    fmt.println("{s[0].bal} {arr[0].bal} {mp["a"][0].bal}")
}
"#,
    );
}

/// a closure-local `m` and a closure param `m` are slices, not the outer map
#[test]
fn accepts_closure_shadow_and_param() {
    accepts(
        r#"struct acct {
    bal int
}

fn main() {
    m := map[string]acct{"a": acct{bal: 1}}
    f := fn() {
        m := []acct{acct{bal: 2}}
        m[0].bal = 3
        fmt.println("{m[0].bal}")
    }
    f()
    g := fn(m []acct) {
        m[0].bal = 4
    }
    g([]acct{acct{bal: 0}})
    fmt.println("{m["a"].bal}")
}
"#,
    );
}

/// a block-scoped shadow, and the O8 loan form the fix-it suggests
#[test]
fn accepts_block_shadow_and_loop_loan() {
    accepts(
        r#"struct acct {
    bal int
}

fn main() {
    m := map[string]acct{"a": acct{bal: 1}}
    if true {
        m := []acct{acct{bal: 2}}
        m[0].bal = 3
        fmt.println("{m[0].bal}")
    }
    for _, v in []int{1} {
        n := &m["a"]
        n.bal = v
    }
    fmt.println("{m["a"].bal}")
}
"#,
    );
}

/// `map[string]*acct`: the element is a pointer, Go auto-dereferences
#[test]
fn accepts_map_of_pointers() {
    accepts(
        r#"struct acct {
    bal int
}

fn main() {
    m := map[string]*acct{"a": &acct{bal: 1}}
    m["a"].bal = 3
    fmt.println("{m["a"].bal}")
}
"#,
    );
}

// ---------------------------------------------------------------------------
// Slot loans re-bound into pointer variables stay accepted
// ---------------------------------------------------------------------------

/// `var q *acct; q = &m[k]`
#[test]
fn accepts_slot_loan_into_var_pointer() {
    accepts(
        r#"struct acct {
    bal int
}

fn main() {
    m := map[string]acct{"a": acct{bal: 1}}
    var q *acct
    q = &m["a"]
    q.bal = 9
    fmt.println("{m["a"].bal}")
}
"#,
    );
}

/// `q := &acct{}` then `q = &m[k]`
#[test]
fn accepts_slot_loan_into_rebound_pointer() {
    accepts(
        r#"struct acct {
    bal int
}

fn main() {
    m := map[string]acct{"a": acct{bal: 1}}
    q := &acct{bal: 0}
    q = &m["a"]
    q.bal = 9
    fmt.println("{m["a"].bal}")
}
"#,
    );
}

/// a pointer parameter re-bound to a slot
#[test]
fn accepts_slot_loan_into_pointer_param() {
    accepts(
        r#"struct acct {
    bal int
}

fn set(p *acct, m map[string]acct) {
    p = &m["a"]
    p.bal = 9
}

fn main() {
    m := map[string]acct{"a": acct{bal: 1}}
    x := acct{bal: 0}
    set(&x, m)
    fmt.println("{m["a"].bal}")
}
"#,
    );
}

/// `var cur *acct; cur = &b.accts[k]` in a method
#[test]
fn accepts_slot_loan_into_method_pointer_local() {
    accepts(
        r#"struct acct {
    bal int
}

struct bank {
    accts map[string]acct
}

fn (b *bank) bump() {
    var cur *acct
    cur = &b.accts["a"]
    cur.bal = cur.bal + 1
}

fn main() {
    b := bank{accts: map[string]acct{"a": acct{bal: 1}}}
    b.bump()
    fmt.println("{b.accts["a"].bal}")
}
"#,
    );
}

/// the value-typed `var q acct` form
#[test]
fn accepts_slot_loan_into_value_var() {
    accepts(
        r#"struct acct {
    bal int
}

fn main() {
    m := map[string]acct{"a": acct{bal: 1}}
    var q acct
    q = &m["a"]
    q.bal = 9
    fmt.println("{m["a"].bal}")
}
"#,
    );
}

/// A live loan re-bound to a general pointer: the loan lowers to a copy
/// plus store-back, so the pointer would silently be written into the slot.
#[test]
fn rejects_live_loan_rebound_to_general_pointer() {
    rejects_with(
        r#"struct acct {
    bal int
}

fn main() {
    m := map[string]acct{"a": acct{bal: 1}}
    q := &m["a"]
    q.bal = 2
    q = &acct{bal: 5}
    q.bal = 9
    fmt.println("{m["a"].bal} {q.bal}")
}
"#,
        "HYB-E3104",
    );
}

#[test]
fn rejects_live_loan_rebound_via_multi_return_and_closure_and_after_value_update() {
    // 1. Multi-return call re-binding a live slot loan to `*acct` must reject with HYB-E3104.
    rejects_with(
        r#"struct acct {
    bal int
}

fn next_acct() (*acct, bool) {
    a := acct{bal: 5}
    return &a, true
}

fn main() {
    m := map[string]acct{"a": acct{bal: 1}}
    q := &m["a"]
    var ok bool
    q, ok = next_acct()
    fmt.println("{q.bal} {ok}")
}
"#,
        "HYB-E3104",
    );

    // 2. Closure body re-binding a live slot loan to a general pointer must reject with HYB-E3104.
    rejects_with(
        r#"struct acct {
    bal int
}

fn main() {
    f := fn() {
        m := map[string]acct{"a": acct{bal: 1}}
        q := &m["a"]
        q = &acct{bal: 5}
        fmt.println("{q.bal}")
    }
    f()
}
"#,
        "HYB-E3104",
    );

    // 3. Whole-value update (`q = acct{bal: 2}`) on an active O8 slot-loan variable
    // preserves `slot_loan`, so taking `&q` or `&q.bal` afterward still rejects with HYB-E3104.
    rejects_with(
        r#"struct acct {
    bal int
}

fn main() {
    m := map[string]acct{"a": acct{bal: 1}}
    var q acct
    q = &m["a"]
    q = acct{bal: 2}
    p := &q
    fmt.println("{p}")
}
"#,
        "HYB-E3104",
    );
}

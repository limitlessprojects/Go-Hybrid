//! Soundness regression tests for `addressable.rs` (`HYB-E2010`):
//! 1. Closure-local maps, closure map parameters, closure `for` loop map
//!    bindings, and captured outer maps used before a later shadow or in a
//!    sibling branch must reject unaddressable map element stores and O8 slot
//!    loans (`HYB-E2010`), while closure slice/struct shadows continue to pass.
//! 2. Subexpression `&m[k].f` / `&m[k][i]` borrows inside struct literals,
//!    slice literals, map literals, call arguments, and binary/unary
//!    expressions on `Stmt::Local` / `Stmt::Assign` RHS values must be
//!    rejected with `HYB-E2010`.
//! 3. Pointer-to-map (`&map[K]V` / `*map[K]V`) and pointer-to-array-of-maps
//!    (`&[N]map[K]V`) bases must reject stores into and O8 borrows of parts of
//!    map elements (`m[k].f = v`, `q := &m[k].f`), while whole-slot assignment
//!    (`m[k] = v`) and pointer hops above the map index (`m[k].ptr_arr[0].f = v`)
//!    stay accepted.

use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let file = parse_file("test.hyb", src).expect("source parses");
    let mut codes: Vec<String> = check_file(&file)
        .diagnostics
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code)
        .collect();
    codes.sort();
    codes
}

#[test]
fn rejects_closure_param_and_closure_local_map_element_store_and_loan() {
    let src = r#"package p

struct acct {
    bal int
}

fn bad_closure_param() {
    f := fn(m map[string]acct) {
        m["a"].bal = 3
        q := &m["a"].bal
        _ = q
    }
    _ = f
}

fn bad_closure_local() {
    g := fn() {
        m := map[string]acct{"a": acct{bal: 1}}
        m["a"].bal += 2
        q := &m["a"].bal
        _ = q
    }
    _ = g
}
"#;
    assert_eq!(
        codes(src),
        vec!["HYB-E2010", "HYB-E2010", "HYB-E2010", "HYB-E2010"]
    );
}

#[test]
fn rejects_closure_for_loop_map_and_captured_map_before_later_or_sibling_shadow() {
    let src = r#"package p

struct acct {
    bal int
}

fn bad_loop_and_shadow_order(maps []map[string]acct) {
    outer := map[string]acct{"a": acct{bal: 1}}
    f := fn() {
        for _, m in maps {
            m["a"].bal = 5
        }
        outer["a"].bal = 6
        if false {
            outer := []acct{acct{bal: 1}}
            outer[0].bal = 7
        }
    }
    _ = f
}
"#;
    assert_eq!(codes(src), vec!["HYB-E2010", "HYB-E2010"]);
}

#[test]
fn rejects_subexpression_addr_of_map_element_field_in_local_and_assign_rhs() {
    let src = r#"package p

struct acct {
    bal int
    arr [2]int
}

fn bad_subexpr_loans() {
    var m map[string]acct
    a := (&m["a"].bal) == nil
    b := (&m["a"].arr[0]) != nil
    var c bool
    c = (&m["a"].bal) == nil
    _ = a
    _ = b
    _ = c
}
"#;
    assert_eq!(codes(src), vec!["HYB-E2010", "HYB-E2010", "HYB-E2010"]);
}

#[test]
fn rejects_pointer_to_array_of_maps_element_mutation_and_loan() {
    let src = r#"package p

struct acct {
    bal int
}

fn bad_ptr_array_of_maps(arr *[2]map[string]acct) {
    arr[0]["a"].bal = 9
    q := &arr[0]["a"].bal
    _ = q
}
"#;
    assert_eq!(codes(src), vec!["HYB-E2010", "HYB-E2010"]);
}

#[test]
fn accepts_pointer_to_array_of_maps_whole_slot_store_and_pointer_hop_above_map_index() {
    let src = r#"package p

struct leaf {
    n int
}

struct node {
    items []leaf
}

fn ok_ptr_array_of_maps(
    m *[2]map[string]*leaf,
    m2 *[2]map[string]node,
    m3 *[2]map[string]leaf,
) {
    m[0]["a"].n = 3
    m2[0]["a"].items[0].n = 4
    m3[0]["a"] = leaf{n: 5}
}
"#;
    assert!(
        codes(src).is_empty(),
        "expected 0 diagnostics, got: {:?}",
        codes(src)
    );
}

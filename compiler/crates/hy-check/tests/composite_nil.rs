//! composite_nil.rs — regression tests for #2036. On current main N1
//! (`check_composite_nils` in sendability.rs) already rejects nil slice/array
//! items and map values; the nil **map key** slot was the one that still
//! passed. These tests pin that gap and the already-covered slots.

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

#[test]
fn nil_map_key_of_int_is_e1002() {
    let r = check("fn main() {\n    m := map[int]int{nil: 1}\n    _ = m\n}\n");
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for a nil int map key, got {cs:?}"
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn parenthesized_nil_map_key_is_e1002() {
    let r = check("fn main() {\n    m := map[string]int{(nil): 1}\n    _ = m\n}\n");
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for a parenthesized nil map key, got {cs:?}"
    );
}

#[test]
fn nil_map_value_stays_rejected() {
    // N1 owns this slot; the point is that the key fix did not disturb it.
    let r = check("fn main() {\n    m := map[string]int{\"k\": nil}\n    _ = m\n}\n");
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(!codes(&r).is_empty());
}

#[test]
fn nil_slice_item_stays_rejected() {
    let r = check("fn main() {\n    s := []int{nil}\n    _ = s\n}\n");
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(!codes(&r).is_empty());
}

#[test]
fn nil_pointer_map_key_accepts() {
    let r = check("fn main() {\n    m := map[*int]int{nil: 1}\n    _ = m\n}\n");
    let cs = codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E1002"),
        "nil is a valid key for a pointer key type, got {cs:?}"
    );
}

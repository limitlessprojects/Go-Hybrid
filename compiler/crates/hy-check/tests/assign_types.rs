//! assign_types.rs — HYB-E1002 for variable-to-variable assignments of
//! different scalar types (issue #1112): Go rejects the assignment
//! (`cannot use y (variable of type int) as float64 value in assignment`),
//! so an accepted Hybrid program would emit unbuildable Go (GUARANTEES
//! row 13 BUILDABLE).

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    use hy_diag::Severity;
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

// ---------------------------------------------------------------------------
// The issue #1112 repro — the main case
// ---------------------------------------------------------------------------

#[test]
fn int_variable_to_float64_target_is_e1002() {
    let r = check(
        "fn main() {\n    var x float64\n    var y int = 3\n    x = y\n    fmt.println(\"{x}\")\n}\n",
    );
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for int -> float64 assign, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

// ---------------------------------------------------------------------------
// The string baseline the issue claims already rejects
// ---------------------------------------------------------------------------

#[test]
fn string_variable_to_int_target_is_e1002() {
    let r = check(
        "fn main() {\n    var x int\n    var s string = \"hi\"\n    x = s\n    fmt.println(\"{x}\")\n}\n",
    );
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for string -> int assign, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

// ---------------------------------------------------------------------------
// The compound form the issue mentions (`float64 += int`)
// ---------------------------------------------------------------------------

#[test]
fn float64_compound_plus_int_is_e1002() {
    let r = check(
        "fn main() {\n    var x float64 = 1.5\n    var y int = 3\n    x += y\n    fmt.println(\"{x}\")\n}\n",
    );
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for float64 += int, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

// ---------------------------------------------------------------------------
// Accept paths — same-type assign and untyped constants must stay accepted
// ---------------------------------------------------------------------------

#[test]
fn same_type_assign_stays_accepted() {
    let r = check(
        "fn main() {\n    var x int\n    var y int = 3\n    x = y\n    fmt.println(\"{x}\")\n}\n",
    );
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "same-type assign must stay accepted: {:?}",
        codes(&r)
    );
}

#[test]
fn untyped_literal_constant_stays_accepted() {
    // Go's implicit constant conversion: `var x float64; x = 3` builds.
    let r = check(
        "fn main() {\n    var x float64\n    x = 3\n    fmt.println(\"{x}\")\n}\n",
    );
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "untyped constant assign must stay accepted: {:?}",
        codes(&r)
    );
}

#[test]
fn composite_assign_stays_accepted() {
    let r = check(
        "fn main() {\n    var a map[string]int\n    b := map[string]int{}\n    a = b\n    a[\"k\"] = 1\n}\n",
    );
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "same-shape map assign must stay accepted: {:?}",
        codes(&r)
    );
}

#[test]
fn alias_resolves_to_same_base_and_stays_accepted() {
    let r = check(
        "type myint = int\n\nfn main() {\n    var x myint\n    var y int = 3\n    x = y\n    fmt.println(\"{x}\")\n}\n",
    );
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "alias-to-same-base assign must stay accepted: {:?}",
        codes(&r)
    );
}

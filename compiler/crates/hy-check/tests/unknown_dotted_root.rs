//! unknown_dotted_root.rs — issue #1131: a bare dotted access whose root is
//! neither declared, imported, nor a curated package root (`widgets.count`
//! with no `import`) passed `hyb check` with 0 diagnostics and then failed
//! `go build` with `undefined: widgets` (BUILDABLE row 13).

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
// The repro from the issue
// ---------------------------------------------------------------------------

#[test]
fn bare_unknown_dotted_root_is_rejected() {
    let r = check("fn main() {\n  fmt.println(widgets.count)\n}\n");
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1001"),
        "expected HYB-E1001 for `widgets.count` with no import, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn bare_unknown_dotted_root_in_assignment_is_rejected() {
    let r = check("fn main() {\n  x := widgets.count\n  fmt.println(\"{x}\")\n}\n");
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1001"),
        "expected HYB-E1001 for `x := widgets.count`, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

// ---------------------------------------------------------------------------
// Accept paths — every root the checker CAN see must stay accepted
// ---------------------------------------------------------------------------

#[test]
fn builtin_package_root_stays_accepted() {
    let r = check("fn main() {\n  fmt.println(\"ok\")\n}\n");
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "builtin package root must stay accepted: {:?}",
        codes(&r)
    );
}

#[test]
fn local_struct_field_access_stays_accepted() {
    let r = check(
        "struct point {\n  x int\n  y int\n}\n\nfn main() {\n  p := point{x: 1, y: 2}\n  fmt.println(\"{p.x}\")\n}\n",
    );
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "local struct field access must stay accepted: {:?}",
        codes(&r)
    );
}

#[test]
fn file_level_const_field_access_stays_accepted() {
    let r = check(
        "const limit int = 5\n\nfn main() {\n  fmt.println(\"{limit}\")\n}\n",
    );
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "file-level const must stay accepted: {:?}",
        codes(&r)
    );
}

#[test]
fn type_name_member_access_stays_accepted() {
    // `point.origin` — the root is a declared type name, not a package.
    let r = check(
        "struct point {\n  x int\n}\n\nfn main() {\n  p := point{x: 1}\n  fmt.println(\"{p.x}\")\n}\n",
    );
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "declared type name must stay accepted: {:?}",
        codes(&r)
    );
}

#[test]
fn param_receiver_and_closure_capture_roots_stay_accepted() {
    let r = check(
        "struct box {\n  v int\n}\n\nfn (b *box) get() int {\n  return b.v\n}\n\nfn main() {\n  b := box{v: 1}\n  fmt.println(\"{b.get()}\")\n}\n",
    );
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "receiver / param roots must stay accepted: {:?}",
        codes(&r)
    );
}

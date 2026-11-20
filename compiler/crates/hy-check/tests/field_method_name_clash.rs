//! field_method_name_clash.rs — regression tests for #1119: Go refuses a type
//! whose field and method share a name.

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
fn value_method_clashing_with_field_is_e1002() {
    let r = check(
        "struct counter {\n    n int\n}\n\nfn (c counter) n() int {\n    return 1\n}\n\nfn main() {\n    c := counter{n: 2}\n    _ = c\n}\n",
    );
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for field `n` + value method `n`, got {cs:?}"
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn pointer_method_clashing_with_field_is_e1002() {
    let r = check(
        "struct counter {\n    n int\n}\n\nfn (c *counter) n() int {\n    return 1\n}\n\nfn main() {\n    c := counter{n: 2}\n    _ = c\n}\n",
    );
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for field `n` + pointer method `n`, got {cs:?}"
    );
}

#[test]
fn distinct_field_and_method_names_accept() {
    let r = check(
        "struct counter {\n    n int\n}\n\nfn (c counter) get() int {\n    return c.n\n}\n\nfn main() {\n    c := counter{n: 2}\n    _ = c.get()\n}\n",
    );
    let cs = codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E1002"),
        "distinct names must stay accepted, got {cs:?}"
    );
}

#[test]
fn clash_is_detected_when_method_precedes_struct() {
    let r = check(
        "fn (c counter) n() int {\n    return 1\n}\n\nstruct counter {\n    n int\n}\n\nfn main() {\n    c := counter{n: 2}\n    _ = c\n}\n",
    );
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "declaration order must not hide the clash, got {cs:?}"
    );
}

//! t22_let_immutability.rs — regression tests for issue #655:
//! T22: `let` binding-level immutability (no rebinding/reassignment; interior mutation allowed).

use hy_check::{check_file, CheckResult};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn let_direct_reassign_rejected_e3006() {
    let src = "\
fn main() {
    let limit = 10
    limit = 20
    _ = limit
}
";
    let r = check(src);
    let c = codes(&r);
    assert!(
        c.contains(&"HYB-E3006".to_string()),
        "expected HYB-E3006 on let reassign, got: {:?}",
        c
    );
}

#[test]
fn let_compound_assign_rejected_e3006() {
    let src = "\
fn main() {
    let limit = 10
    limit += 1
    _ = limit
}
";
    let r = check(src);
    let c = codes(&r);
    assert!(
        c.contains(&"HYB-E3006".to_string()),
        "expected HYB-E3006 on let compound assign, got: {:?}",
        c
    );
}

#[test]
fn let_inc_dec_rejected_e3006() {
    let src = "\
fn main() {
    let limit = 10
    limit++
    _ = limit
}
";
    let r = check(src);
    let c = codes(&r);
    assert!(
        c.contains(&"HYB-E3006".to_string()),
        "expected HYB-E3006 on let inc, got: {:?}",
        c
    );
}

#[test]
fn let_closure_reassign_rejected_e3006() {
    let src = "\
fn main() {
    let limit = 10
    bump := fn() {
        limit = 99
    }
    bump()
    _ = limit
}
";
    let r = check(src);
    let c = codes(&r);
    assert!(
        c.contains(&"HYB-E3006".to_string()),
        "expected HYB-E3006 on let assign in closure, got: {:?}",
        c
    );
}

#[test]
fn let_multi_assign_rejected_e3006() {
    let src = "\
fn two() (int, int) {
    return 1, 2
}

fn main() {
    let limit = 10
    var other = 0
    limit, other = two()
    _ = limit
    _ = other
}
";
    let r = check(src);
    let c = codes(&r);
    assert!(
        c.contains(&"HYB-E3006".to_string()),
        "expected HYB-E3006 on let in multi-target assign, got: {:?}",
        c
    );
}

#[test]
fn let_composite_binding_reassign_rejected_e3006() {
    let src = "\
fn main() {
    let names = []string{\"x\", \"y\"}
    names = []string{\"a\", \"b\"}
    _ = names
}
";
    let r = check(src);
    let c = codes(&r);
    assert!(
        c.contains(&"HYB-E3006".to_string()),
        "expected HYB-E3006 on let slice binding reassign, got: {:?}",
        c
    );
}

#[test]
fn let_shallow_interior_mutation_accepted() {
    let src = "\
fn main() {
    let names = []string{\"x\", \"y\"}
    names[0] = \"z\"
    _ = names
}
";
    let r = check(src);
    let c = codes(&r);
    assert!(
        c.is_empty(),
        "expected 0 errors for shallow interior mutation on let slice, got: {:?}",
        c
    );
}

#[test]
fn let_inner_scope_shadow_accepted() {
    let src = "\
fn main() {
    let x = 10
    if true {
        x := 20
        _ = x
    }
    _ = x
}
";
    let r = check(src);
    let c = codes(&r);
    assert!(
        c.is_empty(),
        "expected 0 errors for inner block scope shadowing let, got: {:?}",
        c
    );
}

//! Regression for issue #2004: `&` applied to a PROJECTION of a composite
//! literal (`&(box{v: 1}.v)`, `&([2]int{1, 2}[0])`) or of a value-returning
//! call (`&(box(o).v)`, `&(box.make().v)`, `&(fns[0]().v)`) passed
//! `hy-check` with 0 diagnostics, but the emitted Go fails `go build`
//! (`cannot take address of …`). A composite literal is only addressable as
//! the DIRECT operand of `&`.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
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

fn assert_has(r: &hy_check::CheckResult, code: &str) {
    let cs = error_codes(r);
    assert!(
        cs.iter().any(|c| c == code),
        "expected {}, got {:?}",
        code,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

const BOX_PRELUDE: &str = "package main\n\nimport \"fmt\"\n\nstruct box {\n    v int\n}\n\nstruct other_box {\n    v int\n}\n\nfn take(p *int) {\n    fmt.println(\"{p}\")\n}\n";

#[test]
fn addr_of_struct_lit_field_projection_rejected() {
    let r = check(&format!(
        "{BOX_PRELUDE}\nfn main() {{\n    take(&(box{{v: 1}}.v))\n}}\n"
    ));
    assert_has(&r, "HYB-E1002");
}

#[test]
fn addr_of_array_lit_index_projection_rejected() {
    let r = check(&format!(
        "{BOX_PRELUDE}\nfn main() {{\n    take(&([2]int{{1, 2}}[0]))\n}}\n"
    ));
    assert_has(&r, "HYB-E1002");
}

#[test]
fn addr_of_lit_field_binding_rejected() {
    let r = check(&format!(
        "{BOX_PRELUDE}\nfn main() {{\n    p := &(box{{v: 1}}.v)\n    fmt.println(p)\n}}\n"
    ));
    assert_has(&r, "HYB-E1002");
}

#[test]
fn addr_of_struct_conversion_projection_rejected() {
    let r = check(
        "package main\n\nstruct box {\n    v int\n}\n\nstruct other_box {\n    v int\n}\n\n\
         fn main() {\n    o := other_box{v: 1}\n    p1 := &(box(o).v)\n    _ = p1\n}\n",
    );
    assert_has(&r, "HYB-E1002");
}

#[test]
fn addr_of_associated_call_projection_rejected() {
    let r = check(
        "package main\n\nstruct box {\n    v int\n}\n\nfn box.make() box {\n    return box{v: 1}\n}\n\n\
         fn main() {\n    p2 := &(box.make().v)\n    _ = p2\n}\n",
    );
    assert_has(&r, "HYB-E1002");
}

#[test]
fn addr_of_indirect_call_projection_rejected() {
    let r = check(
        "package main\n\nstruct box {\n    v int\n}\n\nfn mk() box {\n    return box{v: 2}\n}\n\n\
         fn main() {\n    fns := []fn() box{mk}\n    p3 := &(fns[0]().v)\n    _ = p3\n}\n",
    );
    assert_has(&r, "HYB-E1002");
}

#[test]
fn direct_composite_literal_addr_stays_clean() {
    let r = check(&format!(
        "{BOX_PRELUDE}\nfn main() {{\n    b := &box{{v: 1}}\n    take(&b.v)\n    a := &[2]int{{1, 2}}\n    take(&a[0])\n    x := box{{v: 3}}\n    take(&x.v)\n    arr := [2]int{{1, 2}}\n    take(&arr[1])\n}}\n"
    ));
    let cs = error_codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E1002"),
        "did not expect HYB-E1002, got {:?}",
        cs
    );
}

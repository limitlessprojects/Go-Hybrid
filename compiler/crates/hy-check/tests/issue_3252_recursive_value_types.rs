//! #3252: recursive value types via generic instantiation, option[T], and
//! aliases escape the size check. These types have infinite size — Go rejects
//! them at `go build`. The checker must reject them as HYB-E1002 (BUILDABLE).

use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).unwrap_or_else(|d| panic!("must parse: {:?}", d));
    check_file(&ast)
}

fn rejects(src: &str) {
    let r = check(src);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E1002"),
        "expected HYB-E1002, got {:?}\nsrc:\n{}", r.diagnostics, src
    );
}

fn accepts(src: &str) {
    let r = check(src);
    assert!(
        !r.diagnostics.iter().any(|d| d.severity == Severity::Error),
        "expected no error, got {:?}\nsrc:\n{}", r.diagnostics, src
    );
}

#[test]
fn repro1_generic_instantiation_cycle_rejected() {
    rejects("struct box[t] {\n    v t\n}\nstruct node {\n    b box[node]\n}\nfn main() {\n    var n node\n    _ = n\n}\n");
}

#[test]
fn repro2_option_payload_cycle_rejected() {
    rejects("struct node {\n    o option[node]\n}\nfn main() {\n    var n node\n    _ = n\n}\n");
}

#[test]
fn repro3_alias_cycle_rejected() {
    rejects("type bad = [3]bad\nfn main() {\n    var b bad\n    _ = b\n}\n");
}

#[test]
fn direct_struct_self_cycle_rejected() {
    rejects("struct node {\n    next node\n}\nfn main() {\n    var n node\n    _ = n\n}\n");
}

#[test]
fn mutual_struct_cycle_rejected() {
    rejects("struct a {\n    b b\n}\nstruct b {\n    a a\n}\nfn main() {\n    var x a\n    _ = x\n}\n");
}

#[test]
fn array_elem_self_cycle_rejected() {
    rejects("struct node {\n    arr [3]node\n}\nfn main() {\n    var n node\n    _ = n\n}\n");
}

#[test]
fn cycle_through_two_generics_rejected() {
    rejects("struct box[t] {\n    v t\n}\nstruct wrap[t] {\n    inner box[t]\n}\nstruct node {\n    w wrap[node]\n}\nfn main() {\n    var n node\n    _ = n\n}\n");
}

#[test]
fn pointer_field_accepted() {
    accepts("struct node {\n    next *node\n}\nfn main() {\n    var n node\n    _ = n\n}\n");
}

#[test]
fn slice_field_accepted() {
    accepts("struct node {\n    children []node\n}\nfn main() {\n    var n node\n    _ = n\n}\n");
}

#[test]
fn map_field_accepted() {
    accepts("struct node {\n    m map[string]node\n}\nfn main() {\n    var n node\n    _ = n\n}\n");
}

#[test]
fn chan_field_accepted() {
    accepts("struct node {\n    ch chan node\n}\nfn main() {\n    var n node\n    _ = n\n}\n");
}

#[test]
fn generic_struct_non_recursive_accepted() {
    accepts("struct box[t] {\n    v t\n}\nfn main() {\n    b := box[int]{ v: 42 }\n    _ = b\n}\n");
}

#[test]
fn primitive_struct_accepted() {
    accepts("struct point {\n    x int\n    y int\n}\nfn main() {\n    var p point\n    _ = p\n}\n");
}

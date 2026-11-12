//! #3112: selectors on **T must be rejected (Go refuses them at build time).
use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn reject(src: &str) {
    let result = check_file(&parse_file("issue_3112.hy", src).expect("parses"));
    assert_eq!(result.verdict, Verdict::Reject, "expected reject:\n{src}\n{:?}", result.diagnostics);
    assert!(result.diagnostics.iter().any(|d| d.code == "HYB-E1002"), "missing E1002:\n{src}\n{:?}", result.diagnostics);
}

fn accept(src: &str) {
    let result = check_file(&parse_file("issue_3112.hy", src).expect("parses"));
    assert_eq!(result.verdict, Verdict::Accept, "expected accept:\n{src}\n{:?}", result.diagnostics);
}

#[test]
fn pptr_field_access_is_rejected() {
    reject("struct point { x int }\nfn main() {\n    p := point{x: 1}\n    ptr := &p\n    pptr := &ptr\n    _ = pptr.x\n}\n");
}

#[test]
fn pptr_method_call_is_rejected() {
    reject("struct point { x int }\nfn (p point) bump() {}\nfn main() {\n    p := point{x: 1}\n    ptr := &p\n    pptr := &ptr\n    pptr.bump()\n}\n");
}

#[test]
fn ppptr_field_access_is_rejected() {
    reject("struct point { x int }\nfn main() {\n    p := point{x: 1}\n    ptr := &p\n    pptr := &ptr\n    ppptr := &pptr\n    _ = ppptr.x\n}\n");
}

#[test]
fn single_pointer_field_access_is_accepted() {
    accept("struct point { x int }\nfn (p point) bump() {}\nfn main() {\n    p := point{x: 1}\n    ptr := &p\n    _ = ptr.x\n    ptr.bump()\n}\n");
}

#[test]
fn direct_struct_field_and_method_are_accepted() {
    accept("struct point { x int }\nfn (p point) bump() {}\nfn main() {\n    p := point{x: 1}\n    _ = p.x\n    p.bump()\n}\n");
}

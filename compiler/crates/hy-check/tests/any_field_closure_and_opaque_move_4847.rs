//! Regression tests for #4847: storing a closure or unmovable leaf
//! (`ForeignOpaque` / `BorrowHandle`) into an `any`-typed composite field
//! (`struct box { v any }`) must be rejected at `move` and send boundaries,
//! while pure value-copy `any` payloads (`42`, `"ok"`) remain accepted.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let file = parse_file("test.hy", src).expect("parse");
    check_file(&file)
}

#[test]
fn rejects_move_struct_with_any_field_initialized_to_closure() {
    let r = check(
        "struct box {\n\
           v any\n\
         }\n\
         fn worker(b box) {\n\
           _ = b\n\
         }\n\
         fn main() {\n\
           x := 0\n\
           b := box{v: fn() { x = 1 }}\n\
           task worker(move b)\n\
           x = 2\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "got: {:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3001"),
        "expected HYB-E3001, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn rejects_move_struct_with_any_field_assigned_closure_and_transitive_copy() {
    let r = check(
        "struct box {\n\
           v any\n\
         }\n\
         fn worker(b box) {\n\
           _ = b\n\
         }\n\
         fn main() {\n\
           x := 0\n\
           var b box\n\
           f := fn() { x = 1 }\n\
           b.v = f\n\
           b2 := b\n\
           task worker(move b2)\n\
           x = 2\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "got: {:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3001"),
        "expected HYB-E3001, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn rejects_move_struct_with_any_field_closure_across_conditional_and_shadow() {
    let r_cond = check(
        "struct box {\n\
           v any\n\
         }\n\
         fn worker(b box) {\n\
           _ = b\n\
         }\n\
         fn main() {\n\
           x := 0\n\
           b := box{v: 42}\n\
           if true {\n\
             b.v = fn() { x = 1 }\n\
           }\n\
           task worker(move b)\n\
           x = 2\n\
         }\n",
    );
    assert_eq!(r_cond.verdict, Verdict::Reject, "got: {:?}", r_cond.diagnostics);

    let r_shadow = check(
        "struct box {\n\
           v any\n\
         }\n\
         fn worker(b box) {\n\
           _ = b\n\
         }\n\
         fn main() {\n\
           x := 0\n\
           b := box{v: fn() { x = 1 }}\n\
           if true {\n\
             b := box{v: 42}\n\
             _ = b\n\
           }\n\
           task worker(move b)\n\
           x = 2\n\
         }\n",
    );
    assert_eq!(r_shadow.verdict, Verdict::Reject, "got: {:?}", r_shadow.diagnostics);
}

#[test]
fn rejects_move_struct_with_any_field_holding_foreign_opaque() {
    let r_lit = check(
        "import \"sync\"\n\
         struct box {\n\
           v any\n\
         }\n\
         fn worker(b box) {\n\
           _ = b\n\
         }\n\
         fn main() {\n\
           var mu sync.mutex\n\
           b := box{v: mu}\n\
           task worker(move b)\n\
         }\n",
    );
    assert_eq!(r_lit.verdict, Verdict::Reject, "got: {:?}", r_lit.diagnostics);
    assert!(
        r_lit.diagnostics.iter().any(|d| d.code == "HYB-E3001"),
        "expected HYB-E3001, got: {:?}",
        r_lit.diagnostics
    );

    let r_assign = check(
        "import \"sync\"\n\
         struct box {\n\
           v any\n\
         }\n\
         fn worker(b box) {\n\
           _ = b\n\
         }\n\
         fn main() {\n\
           var mu sync.mutex\n\
           var b box\n\
           b.v = mu\n\
           task worker(move b)\n\
         }\n",
    );
    assert_eq!(r_assign.verdict, Verdict::Reject, "got: {:?}", r_assign.diagnostics);
    assert!(
        r_assign.diagnostics.iter().any(|d| d.code == "HYB-E3001"),
        "expected HYB-E3001, got: {:?}",
        r_assign.diagnostics
    );
}

#[test]
fn accepts_move_struct_with_any_field_holding_primitive_control() {
    let r = check(
        "struct box {\n\
           v any\n\
         }\n\
         fn worker(b box) {\n\
           _ = b\n\
         }\n\
         fn main() {\n\
           b := box{v: 42}\n\
           task worker(move b)\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Accept, "got: {:?}", r.diagnostics);
    assert!(r.diagnostics.is_empty(), "got: {:?}", r.diagnostics);
}

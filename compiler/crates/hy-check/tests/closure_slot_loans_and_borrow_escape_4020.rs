//! Regression tests for issue #4020:
//! Closure A3 map-slot loan invalidation (HYB-E3102) across move, projection,
//! match arm, and iterate bindings, and closure borrow-escape (HYB-E3104) across
//! move, multi-value define, and match arm bindings.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn has_code(src: &str, code: &str) -> (Verdict, bool) {
    let ast = parse_file("inline.hy", src).expect("must parse");
    let r = check_file(&ast);
    let hit = r
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error && d.code == code);
    (r.verdict, hit)
}

#[test]
fn rejects_closure_slot_loan_map_move_mutation() {
    let src = r#"
fn main() {
    f := fn() -> int {
        var m map[string]int
        slot := &m["a"]
        m2 := move m
        m2["b"] = 1
        return *slot
    }
    _ = f()
}
"#;
    let (v, hit) = has_code(src, "HYB-E3102");
    assert!(v == Verdict::Reject && hit, "expected HYB-E3102 for move map mutation inside closure");
}

#[test]
fn rejects_closure_slot_loan_map_move_assign_mutation() {
    let src = r#"
fn main() {
    f := fn() -> int {
        var m map[string]int
        var m2 map[string]int
        slot := &m["a"]
        m2 = move m
        m2["b"] = 1
        return *slot
    }
    _ = f()
}
"#;
    let (v, hit) = has_code(src, "HYB-E3102");
    assert!(v == Verdict::Reject && hit, "expected HYB-E3102 for move-assign map mutation inside closure");
}

#[test]
fn rejects_closure_slot_loan_projection_and_wrap_mutation() {
    let src = r#"
struct holder {
    m map[string]int
}
fn main() {
    f := fn() -> int {
        var m map[string]int
        slot := &m["a"]
        h := holder{m: m}
        h.m["b"] = 1
        return *slot
    }
    _ = f()
}
"#;
    let (v, hit) = has_code(src, "HYB-E3102");
    assert!(v == Verdict::Reject && hit, "expected HYB-E3102 for wrapped map mutation inside closure");
}

#[test]
fn rejects_closure_slot_loan_iterate_mutation() {
    let src = r#"
fn main() {
    f := fn() -> int {
        var m map[string]int
        slot := &m["a"]
        ms := []map[string]int{m}
        for _, m2 in ms {
            m2["b"] = 1
        }
        return *slot
    }
    _ = f()
}
"#;
    let (v, hit) = has_code(src, "HYB-E3102");
    assert!(v == Verdict::Reject && hit, "expected HYB-E3102 for iterate map mutation inside closure");
}

#[test]
fn rejects_closure_borrow_escape_after_move() {
    let src = r#"
fn main() {
    f := fn() -> *int {
        x := 42
        p := &x
        p2 := move p
        return p2
    }
    _ = f()
}
"#;
    let (v, hit) = has_code(src, "HYB-E3104");
    assert!(v == Verdict::Reject && hit, "expected HYB-E3104 for moved borrow escape from closure");
}

#[test]
fn rejects_closure_borrow_escape_after_move_assign() {
    let src = r#"
fn main() {
    f := fn() -> *int {
        x := 42
        p := &x
        var p2 *int
        p2 = move p
        return p2
    }
    _ = f()
}
"#;
    let (v, hit) = has_code(src, "HYB-E3104");
    assert!(v == Verdict::Reject && hit, "expected HYB-E3104 for move-assigned borrow escape from closure");
}

#[test]
fn rejects_closure_borrow_escape_match_arm() {
    let src = r#"
fn main() {
    f := fn() -> *int {
        x := 42
        opt := some(&x)
        match opt {
            some(p) => return p,
            none => return new(int),
        }
    }
    _ = f()
}
"#;
    let (v, hit) = has_code(src, "HYB-E3104");
    assert!(v == Verdict::Reject && hit, "expected HYB-E3104 for match-arm borrow escape from closure");
}

#[test]
fn accepts_valid_closure_borrows_and_after_loan_mutations() {
    let src = r#"
fn main() {
    f := fn() -> int {
        var m map[string]int
        m["a"] = 1
        slot := &m["a"]
        v := *slot
        m["b"] = 2
        return v
    }
    _ = f()
}
"#;
    let ast = parse_file("inline.hy", src).expect("must parse");
    let r = check_file(&ast);
    assert_eq!(r.verdict, Verdict::Accept, "expected Verdict::Accept, got: {:?}", r.diagnostics);
}

//! match_arm_binding_soundness.rs — regression tests for issue #243:
//! `match` arm pattern bindings `some(v)` and `some(x)` constructors must
//! propagate shape, type, may-share / alias edges, `non_owned_origin`,
//! `const` deep immutability, and A3 map-slot loan conflict tracking across
//! Pass A (`walk.rs`), Pass B (`lib.rs`, `loans.rs`), and the closure passes
//! (`closures.rs`, `scan_closure_body`, `collect_mutation_roots`).

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

// ---------------------------------------------------------------------------
// 1. HYB-E3001 / HYB-E3002 — non-move spawn & send of shared map via some(v)
// ---------------------------------------------------------------------------

#[test]
fn match_some_binding_spawn_without_move_is_e3001() {
    let src = "\
fn worker(m map[string]int) {
    m[\"k\"] = 2
}

fn main() {
    m := map[string]int{\"k\": 1}
    opt := some(m)
    match opt {
        some(v) => {
            task worker(v)
        }
        none => {}
    }
    m[\"k\"] = 3
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        error_codes(&r).contains(&"HYB-E3001".to_string()),
        "expected HYB-E3001, got {:?}",
        r.diagnostics
    );
}

#[test]
fn match_some_binding_send_without_move_is_e3002() {
    let src = "\
fn main() {
    ch := make(chan map[string]int, 1)
    m := map[string]int{\"k\": 1}
    opt := some(m)
    match opt {
        some(v) => {
            ch <- v
        }
        none => {}
    }
    m[\"k\"] = 3
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        error_codes(&r).contains(&"HYB-E3002".to_string()),
        "expected HYB-E3002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn closure_match_some_binding_send_without_move_is_e3002() {
    let src = "\
fn main() {
    ch := make(chan map[string]int, 1)
    m := map[string]int{\"k\": 1}
    opt := some(m)
    f := fn() {
        match opt {
            some(v) => {
                ch <- v
            }
            none => {}
        }
    }
    f()
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        error_codes(&r).contains(&"HYB-E3002".to_string()),
        "expected HYB-E3002, got {:?}",
        r.diagnostics
    );
}

// ---------------------------------------------------------------------------
// 2. HYB-E3003 — use-after-move across match some(v) and outer map
// ---------------------------------------------------------------------------

#[test]
fn move_outer_map_then_use_match_some_binding_is_e3003() {
    let src = "\
fn main() {
    ch := make(chan map[string]int, 1)
    m := map[string]int{\"k\": 1}
    opt := some(m)
    match opt {
        some(v) => {
            ch <- move m
            v[\"k\"] = 99
        }
        none => {}
    }
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        error_codes(&r).contains(&"HYB-E3003".to_string()),
        "expected HYB-E3003, got {:?}",
        r.diagnostics
    );
}

#[test]
fn move_match_some_binding_then_use_outer_map_is_e3003() {
    let src = "\
fn main() {
    ch := make(chan map[string]int, 1)
    m := map[string]int{\"k\": 1}
    opt := some(m)
    match opt {
        some(v) => {
            ch <- move v
        }
        none => {}
    }
    m[\"k\"] = 99
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        error_codes(&r).contains(&"HYB-E3003".to_string()),
        "expected HYB-E3003, got {:?}",
        r.diagnostics
    );
}

#[test]
fn move_self_shadowing_match_some_binding_then_use_outer_map_is_e3003() {
    let src = "\
fn main() {
    ch := make(chan map[string]int, 1)
    m := map[string]int{\"k\": 1}
    opt := some(m)
    match opt {
        some(m) => {
            ch <- move m
        }
        none => {}
    }
    m[\"k\"] = 99
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        error_codes(&r).contains(&"HYB-E3003".to_string()),
        "expected HYB-E3003, got {:?}",
        r.diagnostics
    );
}

// ---------------------------------------------------------------------------
// 3. HYB-E3004 — non-owned move of parameter / some(param) via match some(v)
// ---------------------------------------------------------------------------

#[test]
fn move_match_some_binding_from_option_param_is_e3004() {
    let src = "\
fn bad_param(opt option[map[string]int], ch chan map[string]int) {
    match opt {
        some(v) => {
            ch <- move v
        }
        none => {}
    }
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        error_codes(&r).contains(&"HYB-E3004".to_string()),
        "expected HYB-E3004, got {:?}",
        r.diagnostics
    );
}

#[test]
fn move_match_some_binding_from_some_of_map_param_is_e3004() {
    let src = "\
fn bad_param(m map[string]int, ch chan map[string]int) {
    opt := some(m)
    match opt {
        some(v) => {
            ch <- move v
        }
        none => {}
    }
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        error_codes(&r).contains(&"HYB-E3004".to_string()),
        "expected HYB-E3004, got {:?}",
        r.diagnostics
    );
}

#[test]
fn move_self_shadowing_match_some_binding_from_option_param_is_e3004() {
    let src = "\
fn bad_param(opt option[map[string]int], ch chan map[string]int) {
    match opt {
        some(opt) => {
            ch <- move opt
        }
        none => {}
    }
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        error_codes(&r).contains(&"HYB-E3004".to_string()),
        "expected HYB-E3004, got {:?}",
        r.diagnostics
    );
}

// ---------------------------------------------------------------------------
// 4. HYB-E3006 — const deep immutability write/delete via match some(v)
// ---------------------------------------------------------------------------

#[test]
fn write_and_delete_through_match_some_of_const_is_e3006() {
    let src = "\
fn main() {
    m := map[string]int{\"k\": 1}
    const opt option[map[string]int] = some(m)
    match opt {
        some(v) => {
            v[\"k\"] = 99
            delete(v, \"k\")
        }
        none => {}
    }
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    let codes = error_codes(&r);
    assert!(
        codes.iter().filter(|c| *c == "HYB-E3006").count() >= 2,
        "expected HYB-E3006 for both index write and delete, got {:?}",
        r.diagnostics
    );
}

#[test]
fn closure_write_and_delete_through_match_some_of_const_is_e3006() {
    let src = "\
fn main() {
    m := map[string]int{\"k\": 1}
    const opt option[map[string]int] = some(m)
    let f = fn() {
        match opt {
            some(v) => {
                v[\"k\"] = 99
                delete(v, \"k\")
            }
            none => {}
        }
    }
    f()
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    let codes = error_codes(&r);
    assert!(
        codes.iter().filter(|c| *c == "HYB-E3006").count() >= 2,
        "expected HYB-E3006 for both closure index write and delete, got {:?}",
        r.diagnostics
    );
}

// ---------------------------------------------------------------------------
// 5. HYB-E3102 — A3 map-slot loan invalidation via match some(v)
// ---------------------------------------------------------------------------

#[test]
fn slot_loan_invalidated_by_match_some_insert_and_delete_is_e3102() {
    let src = "\
import \"fmt\"

fn main() {
    m := map[string]int{\"a\": 1}
    opt := some(m)
    p := &m[\"a\"]
    match opt {
        some(v) => {
            v[\"b\"] = 2
            delete(v, \"a\")
        }
        none => {}
    }
    fmt.println(p)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    let codes = error_codes(&r);
    assert!(
        codes.iter().filter(|c| *c == "HYB-E3102").count() >= 2,
        "expected HYB-E3102 for both insert and delete while slot loan is live, got {:?}",
        r.diagnostics
    );
}

#[test]
fn slot_loan_invalidated_by_callee_mutating_match_some_param_is_e3102() {
    let src = "\
import \"fmt\"

fn mutate_opt(opt option[map[string]int]) {
    match opt {
        some(v) => {
            v[\"b\"] = 2
        }
        none => {}
    }
}

fn main() {
    m := map[string]int{\"a\": 1}
    opt := some(m)
    p := &m[\"a\"]
    mutate_opt(opt)
    fmt.println(p)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        error_codes(&r).contains(&"HYB-E3102".to_string()),
        "expected HYB-E3102 for interprocedural mutation via match some(v), got {:?}",
        r.diagnostics
    );
}

#[test]
fn slot_loan_invalidated_by_closure_mutating_match_some_capture_is_e3102() {
    let src = "\
import \"fmt\"

fn main() {
    m := map[string]int{\"a\": 1}
    opt := some(m)
    mutator := fn() {
        match opt {
            some(v) => {
                v[\"b\"] = 2
            }
            none => {}
        }
    }
    p := &m[\"a\"]
    mutator()
    fmt.println(p)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        error_codes(&r).contains(&"HYB-E3102".to_string()),
        "expected HYB-E3102 for closure mutation via match some(v), got {:?}",
        r.diagnostics
    );
}

// ---------------------------------------------------------------------------
// 6. Positive controls — primitive option[int] and owned move without post-use
// ---------------------------------------------------------------------------

#[test]
fn primitive_option_match_arm_send_is_accepted() {
    let src = "\
fn main() {
    ch := make(chan int, 1)
    opt := some(42)
    match opt {
        some(v) => {
            ch <- v
        }
        none => {}
    }
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    assert!(error_codes(&r).is_empty(), "{:?}", r.diagnostics);
}

#[test]
fn owned_map_option_match_arm_move_send_without_post_use_is_accepted() {
    let src = "\
fn main() {
    ch := make(chan map[string]int, 1)
    m := map[string]int{\"k\": 1}
    opt := some(m)
    match opt {
        some(v) => {
            ch <- move v
        }
        none => {}
    }
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    assert!(error_codes(&r).is_empty(), "{:?}", r.diagnostics);
}

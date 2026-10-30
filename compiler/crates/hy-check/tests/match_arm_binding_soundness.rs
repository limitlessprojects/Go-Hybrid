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

// ---------------------------------------------------------------------------
// 7. Issue #752 — move-wrapped expressions in match, for-in, return, copy,
//    and map mutations preserve non-owned, const, borrow, and A3 provenance
// ---------------------------------------------------------------------------

#[test]
fn match_move_some_param_and_copy_move_param_are_e3004() {
    let src = "\
fn launder_match(param []int, ch chan []int) {
    match move some(param) {
        some(v) => {
            ch <- move v
        }
        none => {}
    }
}

fn launder_copy(src [][]int, ch chan [][]int) {
    dst := make([][]int, 1)
    copy(dst, move src)
    ch <- move dst
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    let codes = error_codes(&r);
    assert!(
        codes.iter().filter(|c| *c == "HYB-E3004").count() >= 2,
        "expected HYB-E3004 for both match move some(param) and copy(dst, move src), got {:?}",
        r.diagnostics
    );
}

#[test]
fn match_move_const_and_for_in_move_const_are_e3006() {
    let src = "\
fn main() {
    const xs = []int{1, 2, 3}
    opt := some(xs)
    match move opt {
        some(v) => {
            v[0] = 99
        }
        none => {}
    }

    const slices = [][]int{[]int{1, 2}}
    for _, s in move slices {
        s[0] = 88
    }
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    let codes = error_codes(&r);
    assert!(
        codes.iter().filter(|c| *c == "HYB-E3006").count() >= 2,
        "expected HYB-E3006 for both match move opt and for-in move slices, got {:?}",
        r.diagnostics
    );
}

#[test]
fn return_move_wrapped_borrow_pointer_is_e3104() {
    let src = "\
fn escape_local() *int {
    x := 42
    p := &x
    q := move p
    return q
}

fn escape_direct() *int {
    x := 42
    p := &x
    return move p
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    let codes = error_codes(&r);
    assert!(
        codes.iter().filter(|c| *c == "HYB-E3104").count() >= 2,
        "expected HYB-E3104 for both q := move p; return q and return move p, got {:?}",
        r.diagnostics
    );
}

#[test]
fn slot_loan_invalidated_by_move_wrapped_delete_call_and_alias_is_e3102() {
    let src = "\
import \"fmt\"

fn mutate(x map[string]int) {
    x[\"b\"] = 2
}

fn main() {
    m := map[string]int{\"a\": 1}
    p := &m[\"a\"]
    delete(move m, \"a\")
    fmt.println(p)

    m2 := map[string]int{\"a\": 1}
    p2 := &m2[\"a\"]
    mutate(move m2)
    fmt.println(p2)

    m3 := map[string]int{\"a\": 1}
    p3 := &m3[\"a\"]
    m3_alias := move m3
    m3_alias[\"b\"] = 3
    fmt.println(p3)

    m4 := map[string]int{\"a\": 1}
    opt4 := some(m4)
    p4 := &m4[\"a\"]
    match move opt4 {
        some(v) => {
            v[\"b\"] = 4
        }
        none => {}
    }
    fmt.println(p4)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    let codes = error_codes(&r);
    assert!(
        codes.iter().filter(|c| *c == "HYB-E3102").count() >= 4,
        "expected HYB-E3102 for delete(move m), mutate(move m2), m3_alias insert, and match move opt4 insert, got {:?}",
        r.diagnostics
    );
}

#[test]
fn owned_match_move_opt_send_is_accepted() {
    let src = "\
fn main() {
    ch := make(chan map[string]int, 1)
    m := map[string]int{\"k\": 1}
    opt := some(m)
    match move opt {
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

#[test]
fn interprocedural_and_closure_slot_loan_invalidation_via_move_is_e3102() {
    let src = "\
import \"fmt\"

fn mutate_via_move_alias(x map[string]int) {
    y := move x
    y[\"b\"] = 2
}

fn delete_via_move_param(x map[string]int) {
    delete(move x, \"a\")
}

fn main(cb fn(x map[string]int)) {
    m1 := map[string]int{\"a\": 1}
    p1 := &m1[\"a\"]
    mutate_via_move_alias(m1)
    fmt.println(p1)

    m2 := map[string]int{\"a\": 1}
    p2 := &m2[\"a\"]
    delete_via_move_param(m2)
    fmt.println(p2)

    m3 := map[string]int{\"a\": 1}
    p3 := &m3[\"a\"]
    f := fn() {
        delete(move m3, \"a\")
    }
    f2 := move f
    f2()
    fmt.println(p3)

    m4 := map[string]int{\"a\": 1}
    p4 := &m4[\"a\"]
    cb(move m4)
    fmt.println(p4)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    let codes = error_codes(&r);
    assert!(
        codes.iter().filter(|c| *c == "HYB-E3102").count() >= 4,
        "expected at least 4 HYB-E3102 diagnostics, got {:?}",
        r.diagnostics
    );
}

#[test]
fn borrow_handle_view_loan_extended_across_move_and_match_is_e3101() {
    let src = "\
import \"fmt\"
import \"rust/serde_json\"

fn main() {
    doc := serde_json.parse(`{}`)?
    u := doc.array(\"users\")?
    match some(u) {
        some(u2) => {
            doc.set(\"k\", 1)?
            fmt.println(u2.len())
        }
        none => {}
    }

    doc2 := serde_json.parse(`{}`)?
    match some(doc2.array(\"users\")?) {
        some(u4) => {
            doc2.set(\"k\", 1)?
            fmt.println(u4.len())
        }
        none => {}
    }

    doc3 := serde_json.parse(`{}`)?
    u5 := move (doc3.array(\"users\")?)
    doc3.set(\"k\", 1)?
    fmt.println(u5.len())
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    let codes = error_codes(&r);
    assert!(
        codes.iter().filter(|c| *c == "HYB-E3101").count() >= 3,
        "expected at least 3 HYB-E3101 diagnostics, got {:?}",
        r.diagnostics
    );
}

#[test]
fn interprocedural_out_param_store_with_move_arg_preserves_sharing_provenance() {
    let src = "\
import \"fmt\"

fn store_into(d [][]int, s []int) {
    d[0] = s
}

fn bad_use_after_move() {
    ch := make(chan []int, 1)
    dst := make([][]int, 1)
    src := []int{1, 2, 3}
    store_into(move dst, src)
    ch <- move src
    fmt.println(dst)
}

fn bad_non_owned_param(s []int, ch chan [][]int) {
    dst := make([][]int, 1)
    store_into(move dst, s)
    ch <- move dst
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    let codes = error_codes(&r);
    assert!(
        codes.contains(&"HYB-E3003".to_string()),
        "expected HYB-E3003 for bad_use_after_move, got {:?}",
        r.diagnostics
    );
    assert!(
        codes.contains(&"HYB-E3004".to_string()),
        "expected HYB-E3004 for bad_non_owned_param, got {:?}",
        r.diagnostics
    );
}

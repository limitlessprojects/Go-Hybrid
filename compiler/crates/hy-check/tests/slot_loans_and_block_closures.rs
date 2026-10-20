use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let file = parse_file("test.hy", src).expect("expected clean parse");
    check_file(&file)
}

fn codes(res: &hy_check::CheckResult) -> Vec<String> {
    res.diagnostics.iter().map(|d| d.code.clone()).collect()
}

// ===========================================================================
// Family 1: Block-scoped closure captures (HYB-E3001 / E3002 / E3003 / E3004 / E3006)
// ===========================================================================

#[test]
fn block_scoped_closure_shadowed_primitive_spawn_is_e3001() {
    let res = check(
        r#"
fn worker(m map[string]int, done chan int) {
    m["k"] = 1
    done <- 1
}

fn main() {
    done := make(chan int, 1)
    m := 1
    _ = m
    if true {
        m := map[string]int{"k": 0}
        f := fn() {
            task worker(m, done)
        }
        f()
        m["k"] = 2
    }
    <-done
}
"#,
    );
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        codes(&res).contains(&"HYB-E3001".to_string()),
        "expected HYB-E3001 on block-shadowed map capture, got {:?}",
        res.diagnostics
    );
}

#[test]
fn block_scoped_closure_capture_move_is_e3004() {
    let res = check(
        r#"
fn worker(m map[string]int, done chan int) {
    m["k"] = 1
    done <- 1
}

fn main() {
    done := make(chan int, 2)
    if true {
        m := map[string]int{"k": 0}
        f := fn() {
            var alias map[string]int = m
            task worker(move alias, done)
        }
        f()
        f()
    }
    <-done
    <-done
}
"#,
    );
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        codes(&res).contains(&"HYB-E3004".to_string()),
        "expected HYB-E3004 on move of block-scoped capture alias, got {:?}",
        res.diagnostics
    );
}

#[test]
fn block_scoped_closure_use_after_move_is_e3003() {
    let res = check(
        r#"
fn worker(m map[string]int, done chan int) {
    m["k"] = 1
    done <- 1
}

fn main() {
    done := make(chan int, 1)
    if true {
        m := map[string]int{"k": 0}
        task worker(move m, done)
        f := fn() {
            m["k"] = 99
        }
        f()
    }
    <-done
}
"#,
    );
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        codes(&res).contains(&"HYB-E3003".to_string()),
        "expected HYB-E3003 on closure use of moved block-scoped binding, got {:?}",
        res.diagnostics
    );
}

#[test]
fn block_scoped_closure_const_write_is_e3006() {
    let res = check(
        r#"
fn main() {
    if true {
        const c = map[string]int{"k": 0}
        f := fn() {
            c["k"] = 99
        }
        f()
    }
}
"#,
    );
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        codes(&res).contains(&"HYB-E3006".to_string()),
        "expected HYB-E3006 on closure write to block-scoped const, got {:?}",
        res.diagnostics
    );
}

#[test]
fn block_scoped_closure_positive_control_accepts() {
    let res = check(
        r#"
fn main() {
    done := make(chan int, 1)
    if true {
        n := 42
        f := fn() {
            done <- n
        }
        f()
    }
    <-done
}
"#,
    );
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "unexpected diagnostics: {:?}",
        res.diagnostics
    );
}

// ===========================================================================
// Family 2: O8 slot/field-loan store-backs & const pointer borrows (HYB-E3006 / E3004)
// ===========================================================================

#[test]
fn const_map_slot_loan_without_explicit_write_is_e3006() {
    let res = check(
        r#"
struct svc {
    count int
}

const shared = map[string]svc{"a": svc{count: 1}}

fn worker(done chan int) {
    s := &shared["a"]
    _ = s.count
    done <- 1
}
"#,
    );
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        codes(&res).contains(&"HYB-E3006".to_string()),
        "expected HYB-E3006 on &shared[\"a\"] O8 loan (unconditional block-exit store-back), got {:?}",
        res.diagnostics
    );
}

#[test]
fn const_struct_field_loan_inside_closure_is_e3006() {
    let res = check(
        r#"
struct svc {
    count int
}

struct holder {
    s svc
}

const cfg = holder{s: svc{count: 1}}

fn main() {
    f := fn() {
        p := &cfg.s
        _ = p.count
    }
    f()
}
"#,
    );
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        codes(&res).contains(&"HYB-E3006".to_string()),
        "expected HYB-E3006 on &cfg.s O8 field loan inside closure, got {:?}",
        res.diagnostics
    );
}

#[test]
fn const_array_pointer_borrow_write_is_e3006() {
    let res = check(
        r#"
const arr [2]int = [10, 20]

fn main() {
    p := &arr
    p[0] = 99
}
"#,
    );
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        codes(&res).contains(&"HYB-E3006".to_string()),
        "expected HYB-E3006 on write through pointer borrow of const array, got {:?}",
        res.diagnostics
    );
}

#[test]
fn const_struct_pointer_borrow_move_to_task_is_e3004() {
    let res = check(
        r#"
struct cfg {
    count int
}

const global_cfg = cfg{count: 1}

fn worker(p *cfg, done chan int) {
    p.count = 99
    done <- 1
}

fn main() {
    done := make(chan int, 1)
    p := &global_cfg
    task worker(move p, done)
    <-done
}
"#,
    );
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        codes(&res).contains(&"HYB-E3004".to_string()),
        "expected HYB-E3004 on moving pointer borrow of const across task boundary, got {:?}",
        res.diagnostics
    );
}

// ===========================================================================
// Family 3: A3 (HYB-E3102) slot-loan window bypasses & parenthesized borrows
// ===========================================================================

#[test]
fn slot_loan_alias_extends_window_is_e3102() {
    let res = check(
        r#"
struct svc {
    count int
}

fn main() {
    m := map[string]svc{"a": svc{count: 1}}
    s := &m["a"]
    p := s
    m["b"] = svc{count: 2}
    p.count = 99
}
"#,
    );
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        codes(&res).contains(&"HYB-E3102".to_string()),
        "expected HYB-E3102 when map is mutated before last use of slot-loan alias p, got {:?}",
        res.diagnostics
    );
}

#[test]
fn loop_carried_slot_loan_insert_after_use_in_loop_is_e3102() {
    let res = check(
        r#"
struct svc {
    count int
}

fn main() {
    m := map[string]svc{"a": svc{count: 1}}
    s := &m["a"]
    for i in 0..2 {
        s.count = s.count + 1
        m["b"] = svc{count: i}
    }
}
"#,
    );
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        codes(&res).contains(&"HYB-E3102".to_string()),
        "expected HYB-E3102 when loop-carried slot loan is used on next iteration after map insert, got {:?}",
        res.diagnostics
    );
}

#[test]
fn user_method_call_mutating_receiver_map_in_window_is_e3102() {
    let res = check(
        r#"
struct svc {
    count int
}

struct env {
    services map[string]svc
}

fn (e *env) bump() {
    e.services["b"] = svc{count: 2}
}

fn main() {
    e := env{services: map[string]svc{"a": svc{count: 1}}}
    s := &e.services["a"]
    e.bump()
    s.count = 99
}
"#,
    );
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        codes(&res).contains(&"HYB-E3102".to_string()),
        "expected HYB-E3102 when user method mutates loaned map inside slot-loan window, got {:?}",
        res.diagnostics
    );
}

#[test]
fn callee_local_alias_insert_in_window_is_e3102() {
    let res = check(
        r#"
struct svc {
    count int
}

fn touch(t map[string]svc) {
    alias := t
    alias["b"] = svc{count: 2}
}

fn main() {
    m := map[string]svc{"a": svc{count: 1}}
    s := &m["a"]
    touch(m)
    s.count = 99
}
"#,
    );
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        codes(&res).contains(&"HYB-E3102".to_string()),
        "expected HYB-E3102 when callee mutates parameter through local alias, got {:?}",
        res.diagnostics
    );
}

#[test]
fn closure_param_copied_closure_inline_closure_and_closure_slot_loan_are_e3102() {
    // (a) Closure parameter called with loaned map inside window
    let res_param = check(
        r#"
struct svc {
    count int
}

fn run(m map[string]svc, mutator fn(map[string]svc)) {
    s := &m["a"]
    mutator(m)
    s.count = 99
}
"#,
    );
    assert_eq!(res_param.verdict, Verdict::Reject);
    assert!(codes(&res_param).contains(&"HYB-E3102".to_string()));

    // (b) Copied closure called inside window
    let res_copy = check(
        r#"
struct svc {
    count int
}

fn main() {
    m := map[string]svc{"a": svc{count: 1}}
    f := fn() {
        m["b"] = svc{count: 2}
    }
    f2 := f
    s := &m["a"]
    f2()
    s.count = 99
}
"#,
    );
    assert_eq!(res_copy.verdict, Verdict::Reject);
    assert!(codes(&res_copy).contains(&"HYB-E3102".to_string()));

    // (c) Inline closure IIFE called inside window
    let res_iife = check(
        r#"
struct svc {
    count int
}

fn main() {
    m := map[string]svc{"a": svc{count: 1}}
    s := &m["a"]
    _ = (fn() int {
        m["b"] = svc{count: 2}
        return 0
    })()
    s.count = 99
}
"#,
    );
    assert_eq!(res_iife.verdict, Verdict::Reject);
    assert!(codes(&res_iife).contains(&"HYB-E3102".to_string()));

    // (d) Slot loan opened inside a closure body
    let res_inner = check(
        r#"
struct svc {
    count int
}

fn main() {
    m := map[string]svc{"a": svc{count: 1}}
    f := fn() {
        s := &m["a"]
        m["b"] = svc{count: 2}
        s.count = 99
    }
    f()
}
"#,
    );
    assert_eq!(res_inner.verdict, Verdict::Reject);
    assert!(codes(&res_inner).contains(&"HYB-E3102".to_string()));
}

#[test]
fn parenthesized_addr_of_enforces_a3_and_a6() {
    let res_a3 = check(
        r#"
struct svc {
    count int
}

fn main() {
    m := map[string]svc{"a": svc{count: 1}}
    s := (&m["a"])
    m["b"] = svc{count: 2}
    s.count = 99
}
"#,
    );
    assert_eq!(res_a3.verdict, Verdict::Reject);
    assert!(codes(&res_a3).contains(&"HYB-E3102".to_string()));

    let res_a6 = check(
        r#"
struct holder {
    p *int
}

fn main() {
    x := 1
    h := holder{p: (&x)}
    _ = h
    h.p = &(x)
}
"#,
    );
    assert_eq!(res_a6.verdict, Verdict::Reject);
    assert!(codes(&res_a6).contains(&"HYB-E3103".to_string()));
}

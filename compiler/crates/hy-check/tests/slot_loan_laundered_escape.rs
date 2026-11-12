use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("slot_loan_laundered_escape.hy", src)
        .unwrap_or_else(|e| panic!("must parse: {:?}", e));
    check_file(&ast)
}

fn assert_rejects_code(src: &str, code: &str) {
    let res = check(src);
    let codes: Vec<&str> = res
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.as_str())
        .collect();
    assert_eq!(
        res.verdict,
        Verdict::Reject,
        "expected Reject with {}, got Accept; diags={:?}",
        code,
        res.diagnostics
    );
    assert!(
        codes.contains(&code),
        "expected diagnostic {}, got {:?}: {:?}",
        code,
        codes,
        res.diagnostics
    );
}

fn assert_accepts(src: &str) {
    let res = check(src);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "expected Accept, got Reject: {:?}",
        res.diagnostics
    );
}

#[test]
fn slice_map_and_array_laundered_slot_loan_move_rejects_e3104() {
    // Reproducer 1: laundering a map-slot loan into `[]*item{slot}` and moving
    // the slice into a concurrent task while `slot`'s O8 store-back flushes
    // after the `if/else` previously passed with 0 diagnostics and crashed at
    // runtime with `fatal error: concurrent map writes`.
    assert_rejects_code(
        r#"struct item {
    peer map[string]item
    n int
}

fn worker(xs []*item, ready chan int, done chan int) {
    ready <- 1
    for i in 0..10 {
        xs[0].n = i
        xs[0].peer["b"] = item{n: i}
    }
    done <- 1
}

fn run_once(cond bool) {
    ready := make(chan int, 1)
    done := make(chan int, 1)
    m := map[string]item{}
    m["a"] = item{peer: m, n: 1}
    var slot *item
    slot = &m["a"]
    xs := []*item{slot}
    if !cond {
        slot.n = 2
    } else {
        task worker(move xs, ready, done)
        <-ready
    }
    <-done
}

fn main() {
    run_once(true)
}
"#,
        "HYB-E3104",
    );

    // Map literal value and fixed-array literal laundering across channel send.
    assert_rejects_code(
        r#"struct acct { bal int }
fn main() {
    ch := make(chan map[int]*acct, 1)
    m := map[string]acct{"a": acct{bal: 1}}
    var s *acct
    s = &m["a"]
    bag := map[int]*acct{0: s}
    ch <- move bag
}
"#,
        "HYB-E3104",
    );

    assert_rejects_code(
        r#"struct acct { bal int }
fn main() {
    ch := make(chan [1]*acct, 1)
    m := map[string]acct{"a": acct{bal: 1}}
    var s *acct
    s = &m["a"]
    var arr [1]*acct = [1]*acct{s}
    ch <- move arr
}
"#,
        "HYB-E3104",
    );
}

#[test]
fn call_return_multireturn_append_and_index_store_slot_loan_move_reject_e3104() {
    // Reproducer 2: interprocedural helper call `escaped := id_ptr(slot)`
    // laundered the O8 slot-loan pointer past T24 and crashed at runtime.
    assert_rejects_code(
        r#"struct item {
    peer map[string]item
    n int
}

fn id_ptr(p *item) *item {
    return p
}

fn worker(p *item, ready chan int, done chan int) {
    ready <- 1
    p.n = 7
    p.peer["b"] = item{n: 7}
    done <- 1
}

fn run_once(cond bool) {
    ready := make(chan int, 1)
    done := make(chan int, 1)
    m := map[string]item{}
    m["a"] = item{peer: m, n: 1}
    var slot *item
    slot = &m["a"]
    escaped := id_ptr(slot)
    if !cond {
        slot.n = 2
    } else {
        task worker(move escaped, ready, done)
        <-ready
    }
    <-done
}

fn main() {
    run_once(true)
}
"#,
        "HYB-E3104",
    );

    // Multi-return unpack `p, _ := pair(s)` and `p, _ = pair(s)`.
    assert_rejects_code(
        r#"struct acct { bal int }
fn pair(p *acct) (*acct, int) { return p, 1 }
fn main() {
    ch := make(chan *acct, 1)
    m := map[string]acct{"a": acct{bal: 1}}
    var s *acct
    s = &m["a"]
    p, _ := pair(s)
    ch <- move p
}
"#,
        "HYB-E3104",
    );

    // Indexed container store `xs[0] = s` and `xs = append(xs, s)`.
    assert_rejects_code(
        r#"struct acct { bal int }
fn main() {
    ch := make(chan []*acct, 1)
    m := map[string]acct{"a": acct{bal: 1}}
    var s *acct
    s = &m["a"]
    xs := make([]*acct, 1)
    xs[0] = s
    ch <- move xs
}
"#,
        "HYB-E3104",
    );

    assert_rejects_code(
        r#"struct acct { bal int }
fn main() {
    ch := make(chan []*acct, 1)
    m := map[string]acct{"a": acct{bal: 1}}
    var s *acct
    s = &m["a"]
    var xs []*acct
    xs = append(xs, s)
    ch <- move xs
}
"#,
        "HYB-E3104",
    );
}

#[test]
fn match_option_closure_and_self_shadow_slot_loan_laundering_reject_e3104() {
    // `option[*acct]` match arm binding `some(p)` inherits `slot_loan`.
    assert_rejects_code(
        r#"struct acct { bal int }
fn main() {
    ch := make(chan *acct, 1)
    m := map[string]acct{"a": acct{bal: 1}}
    var s *acct
    s = &m["a"]
    match some(s) {
    some(p): {
        ch <- move p
    }
    none: {}
    }
}
"#,
        "HYB-E3104",
    );

    // Self-shadowing inner declaration `s := id_ptr(s)`.
    assert_rejects_code(
        r#"struct acct { bal int }
fn id_ptr(p *acct) *acct { return p }
fn main() {
    ch := make(chan *acct, 1)
    m := map[string]acct{"a": acct{bal: 1}}
    var s *acct
    s = &m["a"]
    if true {
        s := id_ptr(s)
        ch <- move s
    }
}
"#,
        "HYB-E3104",
    );

    // Closure body laundering via slice literal and select-send.
    assert_rejects_code(
        r#"struct acct { bal int }
fn main() {
    ch := make(chan []*acct, 1)
    f := fn() {
        m := map[string]acct{"a": acct{bal: 1}}
        var s *acct
        s = &m["a"]
        xs := []*acct{s}
        select {
        ch <- move xs:
            {}
        default:
            {}
        }
    }
    f()
}
"#,
        "HYB-E3104",
    );
}

#[test]
fn laundered_slot_loan_extends_a3_window_and_rejects_map_insert_e3102() {
    assert_rejects_code(
        r#"import "fmt"
struct acct { bal int }
fn main() {
    m := map[string]acct{"a": acct{bal: 1}}
    var s *acct
    s = &m["a"]
    xs := []*acct{s}
    m["b"] = acct{bal: 2}
    xs[0].bal = 99
    fmt.println(m["a"].bal)
}
"#,
        "HYB-E3102",
    );

    assert_rejects_code(
        r#"import "fmt"
struct acct { bal int }
fn id_ptr(p *acct) *acct { return p }
fn main() {
    m := map[string]acct{"a": acct{bal: 1}}
    var s *acct
    s = &m["a"]
    p := id_ptr(s)
    m["b"] = acct{bal: 2}
    p.bal = 99
    fmt.println(m["a"].bal)
}
"#,
        "HYB-E3102",
    );

    assert_rejects_code(
        r#"import "fmt"
struct acct { bal int }
fn main() {
    m := map[string]acct{"a": acct{bal: 1}}
    var s *acct
    s = &m["a"]
    xs := make([]*acct, 1)
    xs[0] = s
    m["b"] = acct{bal: 2}
    xs[0].bal = 99
    fmt.println(m["a"].bal)
}
"#,
        "HYB-E3102",
    );
}

#[test]
fn positive_controls_value_copies_scalar_slots_and_local_pointer_slices_accept() {
    // O9/A4 scalar field reads, value-typed slot copies, scalar multi-return
    // slots, and non-slot-loan local pointer slices must remain accepted.
    assert_accepts(
        r#"struct acct { bal int }
fn pair(p *acct) (*acct, int) { return p, p.bal }
fn main() {
    ch_ints := make(chan []int, 1)
    ch_accts := make(chan []acct, 1)
    ch_ptrs := make(chan []*acct, 1)

    m1 := map[string]acct{"a": acct{bal: 7}}
    var s *acct
    s = &m1["a"]
    _, copied_n := pair(s)
    ints := []int{s.bal, copied_n}
    ch_ints <- move ints

    m2 := map[string]acct{"a": acct{bal: 9}}
    var q acct
    q = &m2["a"]
    accts := []acct{q}
    ch_accts <- move accts

    local_a := acct{bal: 3}
    ptrs := []*acct{&local_a}
    ch_ptrs <- move ptrs
}
"#,
    );

    // Clearing a tainted slice to a fresh empty slice releases container taint.
    assert_accepts(
        r#"struct acct { bal int }
fn main() {
    ch := make(chan []*acct, 1)
    m := map[string]acct{"a": acct{bal: 1}}
    var s *acct
    s = &m["a"]
    xs := []*acct{s}
    xs[0].bal = 2
    xs = []*acct{}
    ch <- move xs
}
"#,
    );
}

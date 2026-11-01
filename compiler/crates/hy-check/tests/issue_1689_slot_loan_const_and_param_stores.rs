use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let file = parse_file("test.hy", src).expect("parse ok");
    check_file(&file)
}

#[test]
fn slot_loan_whole_value_assign_propagates_const_root_in_fn_and_closure() {
    // 1. Top-level function: storing a `const` slice into `m["a"]` via `slot = item{xs: k_xs}`
    //    must link `m` to `k_xs` so mutating `m["a"].xs[0]` rejects with HYB-E3006.
    let top = check(
        r#"
import "fmt"
const k_xs = []int{1, 2}
struct item { xs []int }
fn main() {
    m := map[string]item{"a": item{xs: []int{0}}}
    slot := &m["a"]
    slot = item{xs: k_xs}
    fmt.println(slot.xs[0])
    m["a"].xs[0] = 99
}
"#,
    );
    assert_eq!(top.verdict, Verdict::Reject, "{:?}", top.diagnostics);
    assert!(
        top.diagnostics.iter().any(|d| d.code == "HYB-E3006"),
        "expected HYB-E3006 on write through map after slot-loan whole-value assign of const, got {:?}",
        top.diagnostics
    );

    // 2. Closure body: `scan_block_const_writes` must preserve `slot`'s link to `m`
    //    across `slot = item{xs: k_xs}` (and multi-value `slot, ok = ...`) so writing
    //    through `m` inside the closure rejects with HYB-E3006.
    let closure = check(
        r#"
import "fmt"
const k_xs = []int{1, 2}
struct item { xs []int }
fn main() {
    f := fn() {
        m := map[string]item{"a": item{xs: []int{0}}}
        slot := &m["a"]
        slot = item{xs: k_xs}
        fmt.println(slot.xs[0])
        m["a"].xs[0] = 99
    }
    f()
}
"#,
    );
    assert_eq!(closure.verdict, Verdict::Reject, "{:?}", closure.diagnostics);
    assert!(
        closure.diagnostics.iter().any(|d| d.code == "HYB-E3006"),
        "expected HYB-E3006 inside closure on write through map after slot-loan whole-value assign of const, got {:?}",
        closure.diagnostics
    );
}

#[test]
fn callee_param_store_through_slot_loan_whole_value_assign_links_params() {
    // 1. Parameter store summary (`walk_stmt_param_stores`): helper `install(dst, src)`
    //    writes `src` into `dst["a"]` via `slot := &dst["a"]; slot = item{xs: src}`.
    //    Calling `install(m, fresh)` where `m` is a parameter must link `fresh` to `m`
    //    and reject `move fresh` with HYB-E3004.
    let param_peer = check(
        r#"
import "fmt"
struct item { xs []int }
fn install(dst map[string]item, src []int) {
    slot := &dst["a"]
    slot = item{xs: src}
    fmt.println(slot.xs[0])
}
fn worker(xs []int) { fmt.println(len(xs)) }
fn run(m map[string]item) {
    fresh := []int{1, 2}
    install(m, fresh)
    task worker(move fresh)
}
"#,
    );
    assert_eq!(param_peer.verdict, Verdict::Reject, "{:?}", param_peer.diagnostics);
    assert!(
        param_peer.diagnostics.iter().any(|d| d.code == "HYB-E3004"),
        "expected HYB-E3004 when moving slice stored into parameter map via helper slot-loan assign, got {:?}",
        param_peer.diagnostics
    );

    // 2. Const propagation through helper parameter store via slot-loan whole-value assign:
    //    `install(m, k_xs)` links `m` to `const k_xs`, so `m["a"].xs[0] = 7` rejects with HYB-E3006.
    let const_store = check(
        r#"
import "fmt"
const k_xs = []int{1, 2}
struct item { xs []int }
fn install(dst map[string]item, src []int) {
    slot := &dst["a"]
    slot = item{xs: src}
    fmt.println(slot.xs[0])
}
fn main() {
    m := map[string]item{"a": item{xs: []int{0}}}
    install(m, k_xs)
    m["a"].xs[0] = 7
}
"#,
    );
    assert_eq!(const_store.verdict, Verdict::Reject, "{:?}", const_store.diagnostics);
    assert!(
        const_store.diagnostics.iter().any(|d| d.code == "HYB-E3006"),
        "expected HYB-E3006 when writing to map after helper slot-loan store of const, got {:?}",
        const_store.diagnostics
    );
}

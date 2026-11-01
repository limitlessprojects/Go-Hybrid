use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let file = parse_file("test.hy", src).expect("parse ok");
    check_file(&file)
}

#[test]
fn whole_value_assign_to_slot_loan_preserves_parameter_non_owned_origin() {
    let r = check(
        r#"
import "fmt"
struct item { xs []int }
fn worker(x item) { fmt.println(len(x.xs)) }
fn update(m map[string]item) {
    slot := &m["a"]
    slot = item{xs: []int{1, 2}}
    task worker(move slot)
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3004"),
        "expected HYB-E3004 when moving slot loan after whole-value update on parameter map, got {:?}",
        r.diagnostics
    );
}

#[test]
fn whole_value_assign_to_slot_loan_links_assigned_storage_to_container() {
    // 1. Parameter map `m`: assigning `slot = item{xs: fresh}` links `fresh` to `m`,
    //    so moving `fresh` afterwards must reject with HYB-E3004 (may-share peer of `m`).
    let param_map = check(
        r#"
import "fmt"
struct item { xs []int }
fn worker(xs []int) { fmt.println(len(xs)) }
fn update(m map[string]item) {
    fresh := []int{1, 2}
    slot := &m["a"]
    slot = item{xs: fresh}
    fmt.println(slot.xs[0])
    task worker(move fresh)
}
"#,
    );
    assert_eq!(param_map.verdict, Verdict::Reject, "{:?}", param_map.diagnostics);
    assert!(
        param_map.diagnostics.iter().any(|d| d.code == "HYB-E3004"),
        "expected HYB-E3004 when moving slice stored into parameter map via slot loan, got {:?}",
        param_map.diagnostics
    );

    // 2. Local map `m`: assigning `slot = item{xs: fresh}` links `fresh` and `m`,
    //    so moving `m` freezes `fresh` (HYB-E3003 on subsequent read of `fresh`).
    let local_map = check(
        r#"
import "fmt"
struct item { xs []int }
fn worker(m map[string]item) { fmt.println(len(m)) }
fn main() {
    m := map[string]item{"a": item{xs: []int{0}}}
    fresh := []int{1, 2}
    slot := &m["a"]
    slot = item{xs: fresh}
    fmt.println(slot.xs[0])
    task worker(move m)
    fmt.println(fresh[0])
}
"#,
    );
    assert_eq!(local_map.verdict, Verdict::Reject, "{:?}", local_map.diagnostics);
    assert!(
        local_map.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 when reading slice stored into moved map via slot loan, got {:?}",
        local_map.diagnostics
    );
}

#[test]
fn multi_return_whole_value_assign_to_slot_loan_preserves_links() {
    let r = check(
        r#"
import "fmt"
struct item { xs []int }
fn wrap_pair(xs []int) (item, bool) {
    return item{xs: xs}, true
}
fn worker(xs []int) { fmt.println(len(xs)) }
fn update(m map[string]item) {
    fresh := []int{1, 2}
    slot := &m["a"]
    var ok bool
    slot, ok = wrap_pair(fresh)
    fmt.println(slot.xs[0], ok)
    task worker(move fresh)
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3004"),
        "expected HYB-E3004 after multi-return whole-value assign into slot loan of parameter map, got {:?}",
        r.diagnostics
    );
}

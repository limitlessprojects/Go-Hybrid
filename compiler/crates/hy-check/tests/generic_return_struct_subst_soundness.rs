//! Regression tests for generic function and method return substitution into
//! folded generic struct / prelude `set[t]` types and `ty_of_expr` / `multi_value_tys`.

use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("fixture must parse");
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

#[test]
fn generic_struct_return_preserves_slice_elem_for_copy_alias_freeze_e3003() {
    let r = check(
        r#"
struct box[t] {
    v []t
}

fn wrap[t](xs []t) box[t] {
    return box[t]{v: xs}
}

fn worker(dst []map[string]int, done chan bool) {
    dst[0]["k"] = 1
    done <- true
}

fn main() {
    src := []map[string]int{map[string]int{"k": 0}}
    dst := []map[string]int{map[string]int{"k": 0}}
    b_src := wrap(src)
    b_dst := wrap(dst)
    copy(b_dst.v, b_src.v)
    done := make(chan bool, 1)
    task worker(move dst, done)
    src[0]["k"] = 2
    <-done
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        codes(&r).iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 on post-move mutation of copy-shared map via generic struct return, got {:?}",
        codes(&r)
    );
}

#[test]
fn generic_struct_and_set_returns_enforce_closure_and_opaque_boundaries_under_shadowing() {
    let closure_box = check(
        r#"
type t = int

struct box[t] {
    f t
}

fn wrap[t](x t) box[t] {
    return box[t]{f: x}
}

fn worker[u](b box[u], done chan bool) {
    _ = b.f
    done <- true
}

fn main() {
    n := 0
    b := wrap(fn() { n += 1 })
    done := make(chan bool, 1)
    task worker(move b, done)
    n += 1
    <-done
}
"#,
    );
    assert_eq!(closure_box.verdict, Verdict::Reject);
    assert!(
        codes(&closure_box).iter().any(|c| c == "HYB-E3001"),
        "expected HYB-E3001 when moving generic-returned closure struct, got {:?}",
        codes(&closure_box)
    );

    let mutex_set = check(
        r#"
import "sync"

type t = int

fn singleton[t comparable](x t) set[t] {
    var s set[t]
    _ = s.insert(x)
    return s
}

fn main() {
    var mu sync.mutex
    s := singleton(&mu)
    ch := make(chan set[*sync.mutex], 1)
    ch <- move s
}
"#,
    );
    assert_eq!(mutex_set.verdict, Verdict::Reject);
    assert!(
        codes(&mutex_set).iter().any(|c| c == "HYB-E3002"),
        "expected HYB-E3002 when sending generic-returned set[*sync.mutex], got {:?}",
        codes(&mutex_set)
    );
}

#[test]
fn generic_struct_return_extends_borrow_handle_view_loan_e3101() {
    let r = check(
        r#"
import "fmt"
import "rust/serde_json"

struct holder[t] {
    v t
}

fn pack[t](x t) holder[t] {
    return holder[t]{v: x}
}

fn main() {
    doc := serde_json.parse("[]")?
    view := pack(doc.array("users")?).v
    doc.set("x", true)?
    fmt.println(view.int("n")?)
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        codes(&r).iter().any(|c| c == "HYB-E3101"),
        "expected HYB-E3101 on mutating handle during live projected view loan, got {:?}",
        codes(&r)
    );
}

#[test]
fn generic_caller_substitutes_return_type_params_and_preserves_incomparable_and_chan_dir() {
    let relay = check(
        r#"
fn wrap_arr[t](v t) ([1]t, bool) {
    return [1]t{v}, true
}

fn relay[u](x u, ch chan [1]u) {
    a, ok := wrap_arr(x)
    if ok {
        ch <- move a
    }
}

fn main() {}
"#,
    );
    assert_eq!(relay.verdict, Verdict::Reject);
    assert!(
        codes(&relay).iter().any(|c| c == "HYB-E3005"),
        "expected HYB-E3005 when forwarding caller type param u through generic return, got {:?}",
        codes(&relay)
    );

    let incomp_and_dir = check(
        r#"
struct box[t] {
    v t
}

fn wrap[t](x t) box[t] {
    return box[t]{v: x}
}

fn main() {
    b1 := wrap(map[int]int{1: 1})
    b2 := wrap(map[int]int{1: 1})
    if b1 == b2 {
        var so chan<- int = make(chan int, 1)
        bc := wrap(so)
        _ = <-bc.v
    }
}
"#,
    );
    assert_eq!(incomp_and_dir.verdict, Verdict::Reject);
    let cs = codes(&incomp_and_dir);
    assert!(cs.iter().any(|c| c == "HYB-E2006"), "expected HYB-E2006, got {:?}", cs);
    assert!(cs.iter().any(|c| c == "HYB-E3008"), "expected HYB-E3008, got {:?}", cs);
}

#[test]
fn generic_primitive_struct_and_set_returns_stay_movable_and_iterable() {
    let r = check(
        r#"
struct box[t] {
    v t
}

fn wrap[t](x t) box[t] {
    return box[t]{v: x}
}

fn (b box[t]) clone_box() box[t] {
    return box[t]{v: b.v}
}

fn singleton[t comparable](x t) set[t] {
    var s set[t]
    _ = s.insert(x)
    return s
}

fn worker[u](b box[u], done chan bool) {
    _ = b.v
    done <- true
}

fn main() {
    b := wrap(42).clone_box()
    done := make(chan bool, 1)
    task worker(move b, done)
    s := singleton(7)
    ch := make(chan int, 1)
    for x in s {
        ch <- x
    }
    ch_set := make(chan set[int], 1)
    ch_set <- move s
    <-done
}
"#,
    );
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected primitive generic struct and set returns to accept, got {:?}",
        r.diagnostics
    );
}

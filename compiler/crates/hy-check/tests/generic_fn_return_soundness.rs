//! Regression tests for issue #2490: generic function and method return
//! type-parameter substitution across O5.2 alias-freeze, A5 closure-capture,
//! and O4.9 unbound generic container boundary checks.

use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("fixture must parse");
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

#[test]
fn inferred_generic_array_of_map_return_freezes_source_map_e3003() {
    let r = check(
        r#"
fn wrap_arr[t](x t) [1]t {
    return [1]t{x}
}

fn worker(arr [1]map[int]int, done chan bool) {
    arr[0][1] = 1
    done <- true
}

fn main() {
    m := map[int]int{0: 0}
    a := wrap_arr(m)
    done := make(chan bool, 1)
    task worker(move a, done)
    m[2] = 2
    <-done
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        codes(&r).iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 on post-move mutation of aliased map `m`, got {:?}",
        codes(&r)
    );
}

#[test]
fn explicit_type_arg_generic_array_of_map_return_freezes_shallow_copy_e3003() {
    let r = check(
        r#"
fn wrap_arr[t](x t) [1]t {
    return [1]t{x}
}

fn worker(arr [1]map[int]int, done chan bool) {
    arr[0][1] = 1
    done <- true
}

fn main() {
    a := wrap_arr[map[int]int](map[int]int{0: 0})
    alias := a
    done := make(chan bool, 1)
    task worker(move a, done)
    alias[0][2] = 2
    <-done
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        codes(&r).iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 on post-move mutation through `alias`, got {:?}",
        codes(&r)
    );
}

#[test]
fn generic_slice_of_closure_return_rejected_at_channel_move_e3002() {
    let r = check(
        r#"
fn wrap_slice[t](x t) []t {
    return []t{x}
}

fn worker(ch chan []fn(), done chan bool) {
    cbs := <-ch
    f := cbs[0]
    f()
    done <- true
}

fn main() {
    count := 0
    cbs := wrap_slice(fn() {
        count += 1
    })
    ch := make(chan []fn(), 1)
    done := make(chan bool, 1)
    task worker(ch, done)
    ch <- move cbs
    count += 1
    <-done
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        codes(&r).iter().any(|c| c == "HYB-E3002"),
        "expected HYB-E3002 when moving generic-returned slice of closures over channel, got {:?}",
        codes(&r)
    );
}

#[test]
fn generic_method_slice_of_closure_return_rejected_at_channel_move_e3002() {
    let r = check(
        r#"
type cb = fn()

struct box[t] {
    items []t
}

fn (b *box[t]) all() []t {
    return b.items
}

fn main() {
    count := 0
    b := box[cb]{
        items: []cb{fn() { count += 1 }},
    }
    cbs := b.all()
    ch := make(chan []cb, 1)
    ch <- move cbs
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        codes(&r).iter().any(|c| c == "HYB-E3002"),
        "expected HYB-E3002 on generic method returning slice of closures, got {:?}",
        codes(&r)
    );
}

#[test]
fn multi_return_generic_container_freezes_source_map_e3003() {
    let r = check(
        r#"
fn wrap_pair[t](x t) ([1]t, bool) {
    return [1]t{x}, true
}

fn worker(arr [1]map[int]int, done chan bool) {
    arr[0][1] = 1
    done <- true
}

fn main() {
    m := map[int]int{0: 0}
    a, ok := wrap_pair(m)
    if ok {
        done := make(chan bool, 1)
        task worker(move a, done)
        m[2] = 2
        <-done
    }
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        codes(&r).iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 on multi-return generic container alias freeze, got {:?}",
        codes(&r)
    );
}

#[test]
fn unbound_generic_slice_param_move_fails_closed_e3005() {
    let r = check(
        r#"
fn relay[t]( ch chan []t) {
    xs := []t{}
    ch <- move xs
}

fn main() {}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        codes(&r).iter().any(|c| c == "HYB-E3005"),
        "expected HYB-E3005 (O4.9) when moving uninstantiated `[]t` across channel boundary, got {:?}",
        codes(&r)
    );
}

#[test]
fn generic_primitive_array_return_stays_movable_and_unaliased() {
    let r = check(
        r#"
fn wrap_arr[t](x t) [1]t {
    return [1]t{x}
}

fn worker(arr [1]int, done chan bool) {
    _ = arr[0]
    done <- true
}

fn main() {
    x := 42
    a := wrap_arr(x)
    alias := a
    done := make(chan bool, 1)
    task worker(move a, done)
    x += alias[0]
    _ = x
    <-done
}
"#,
    );
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected primitive `[1]int` generic return to stay sendable and copy-independent, got {:?}",
        r.diagnostics
    );
}

//! generic_struct_boundary_soundness.rs — regression tests for issue #2483.
//!
//! `Checker::named_shape_depth` previously collapsed every generic struct
//! instantiation (`holder[map[int]int]`, `fn_box[cb]`, `holder[t]`) to its
//! bare head (`TypeShape::Struct(Some("holder"))`), discarding concrete type
//! arguments. Field walkers (`shape_may_share`, `shape_holds_closure`,
//! `shape_holds_unmovable_leaf`, `shape_carries_handle`, `shape_is_const_sendable`,
//! and `incomparable_rec`) then inspected the raw parametric field (`t` ->
//! `Unknown`), allowing:
//!   1. `O5.2` / `A1b` alias-freeze bypass on map/slice-carrying generic structs
//!      (crashing at runtime with `fatal error: concurrent map writes`);
//!   2. `A5` closure-capture task-boundary bypass via `fn_box[cb]`;
//!   3. `O4` / `O7` unmovable `ForeignOpaque` (`sync.mutex`) move bypass;
//!   4. `O4.9` fail-closed unproven type-parameter laundering via `holder[t]`;
//!   5. `HYB-E2006` incomparable `==` bypass on generic structs carrying slices/maps.

use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src).unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn generic_struct_map_alias_mutation_after_move_rejected_e3003() {
    let src = r#"
struct holder[t] {
    m t
}

fn worker(h holder[map[int]int], done chan int) {
    h.m[0] = 1
    done <- 1
}

fn main() {
    h1 := holder[map[int]int]{ m: map[int]int{0: 0} }
    h2 := h1
    done := make(chan int, 1)
    task worker(move h1, done)
    h2.m[0] = 2
    _ = <-done
}
"#;
    let r = check(src);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 on live alias mutation of moved generic map holder, got {:?}",
        r.diagnostics
    );
}

#[test]
fn generic_struct_aliased_slice_mutation_after_move_rejected_e3003() {
    let src = r#"
struct holder[t] {
    xs t
}
type slice_holder = holder[[]int]

fn worker(h slice_holder, done chan int) {
    h.xs[0] = 1
    done <- 1
}

fn main() {
    h1 := slice_holder{ xs: []int{0} }
    h2 := h1
    done := make(chan int, 1)
    task worker(move h1, done)
    h2.xs[0] = 2
    _ = <-done
}
"#;
    let r = check(src);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 on type-aliased generic slice holder, got {:?}",
        r.diagnostics
    );
}

#[test]
fn generic_struct_holding_closure_move_across_task_rejected_e3001() {
    let src = r#"
type cb = fn()

struct fn_box[t] {
    f t
}

fn run_task(b fn_box[cb], done chan int) {
    f := b.f
    f()
    done <- 1
}

fn main() {
    var shared int = 0
    b := fn_box[cb]{
        f: fn() {
            shared = shared + 1
        },
    }
    done := make(chan int, 1)
    task run_task(move b, done)
    shared = shared + 1
    _ = <-done
    _ = shared
}
"#;
    let r = check(src);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3001"),
        "expected HYB-E3001 when moving closure-carrying generic struct across task, got {:?}",
        r.diagnostics
    );
}

#[test]
fn generic_struct_holding_foreign_opaque_mutex_move_rejected_e3001() {
    let src = r#"
import "sync"

struct lock_box[t] {
    mu t
}

fn worker(b lock_box[sync.mutex], done chan int) {
    _ = b
    done <- 1
}

fn main() {
    var b lock_box[sync.mutex]
    done := make(chan int, 1)
    task worker(move b, done)
    _ = <-done
}
"#;
    let r = check(src);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3001"),
        "expected HYB-E3001 when moving sync.mutex inside generic struct across task, got {:?}",
        r.diagnostics
    );
}

#[test]
fn generic_struct_with_unbound_type_param_move_across_chan_rejected_e3005() {
    let src = r#"
struct holder[t] {
    v t
}

fn launder[t](b holder[t], ch chan holder[t]) {
    ch <- move b
}

fn main() {
    ch := make(chan holder[int], 1)
    launder[int](holder[int]{ v: 1 }, ch)
    _ = <-ch
}
"#;
    let r = check(src);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3005"),
        "expected HYB-E3005 on moving generic struct with unbound type param t across channel, got {:?}",
        r.diagnostics
    );
}

#[test]
fn generic_struct_incomparable_equality_rejected_e2006() {
    let src = r#"
struct holder[t] {
    xs t
}

fn main() {
    a := holder[[]int]{ xs: []int{1} }
    b := holder[[]int]{ xs: []int{1} }
    if a == b {
        _ = 1
    }
}
"#;
    let r = check(src);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E2006"),
        "expected HYB-E2006 comparing slice-instantiated generic structs with ==, got {:?}",
        r.diagnostics
    );
}

#[test]
fn generic_struct_primitive_instantiation_allows_copy_and_move() {
    let src = r#"
struct holder[t] {
    v t
}

fn worker(h holder[int], done chan int) {
    done <- h.v
}

fn main() {
    h1 := holder[int]{ v: 42 }
    h2 := h1
    done := make(chan int, 1)
    task worker(move h1, done)
    got := <-done
    _ = h2.v + got
}
"#;
    let r = check(src);
    assert!(
        codes(&r).is_empty(),
        "primitive-instantiated generic struct copy + move must stay accepted, got {:?}",
        r.diagnostics
    );
}

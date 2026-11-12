//! chan_scalar_width_inferred_and_exprs.rs — regression tests for channel
//! numeric element width resolution across inferred `make(chan T)` bindings,
//! struct field channels, call/method return channels, channel type aliases,
//! receive expressions (`<-ch`, `v, ok := <-ch`, `select` receive arms, and
//! `for v in ch`), and inner-scope shadowing.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("test program must parse");
    check_file(&ast)
}

fn assert_rejects_single_e1002(src: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "expected Reject, got {:?}: {:?}",
        r.verdict,
        r.diagnostics
    );
    let errs: Vec<_> = r
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert_eq!(
        errs.len(),
        1,
        "expected exactly 1 error diagnostic, got {:?}",
        errs
    );
    assert_eq!(errs[0].code, "HYB-E1002", "expected HYB-E1002, got {:?}", errs[0]);
}

fn assert_accepts(src: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected Accept, got {:?}: {:?}",
        r.verdict,
        r.diagnostics
    );
}

#[test]
fn send_to_short_decl_make_chan_mismatched_width_rejects() {
    assert_rejects_single_e1002(
        r#"
fn main() {
    ch := make(chan int64, 1)
    var x int32 = 1
    ch <- x
}
"#,
    );
}

#[test]
fn select_send_to_inferred_make_chan_mismatched_width_rejects() {
    assert_rejects_single_e1002(
        r#"
fn main() {
    ch := make(chan float32, 1)
    var f float64 = 1.5
    select {
    ch <- f:
        _ = 0
    default:
        _ = 1
    }
}
"#,
    );
}

#[test]
fn send_to_struct_field_chan_mismatched_width_rejects() {
    assert_rejects_single_e1002(
        r#"
struct pipe {
    ch chan int64
}

fn main() {
    p := pipe{ch: make(chan int64, 1)}
    var x int32 = 7
    p.ch <- x
}
"#,
    );
}

#[test]
fn send_to_fn_and_method_returned_chan_mismatched_width_rejects() {
    assert_rejects_single_e1002(
        r#"
fn make_i64_chan() chan int64 {
    return make(chan int64, 1)
}

fn main() {
    var x int32 = 3
    make_i64_chan() <- x
}
"#,
    );

    assert_rejects_single_e1002(
        r#"
struct factory {
    cap int
}

fn (f factory) open() chan uint32 {
    return make(chan uint32, f.cap)
}

fn main() {
    f := factory{cap: 1}
    var x int32 = 3
    f.open() <- x
}
"#,
    );
}

#[test]
fn send_to_type_alias_chan_mismatched_width_rejects() {
    assert_rejects_single_e1002(
        r#"
type i64_stream = chan int64

fn main() {
    var ch i64_stream = make(i64_stream, 1)
    var x int32 = 9
    ch <- x
}
"#,
    );

    assert_rejects_single_e1002(
        r#"
type i64_stream = chan int64

fn main() {
    ch := make(i64_stream, 1)
    var x int32 = 9
    ch <- x
}
"#,
    );
}

#[test]
fn receive_from_chan_propagates_scalar_width_and_rejects_mismatch() {
    // Direct receive in typed var declaration
    assert_rejects_single_e1002(
        r#"
fn main() {
    ch := make(chan int64, 1)
    var x int32 = <-ch
    _ = x
}
"#,
    );

    // Short declaration receive propagated into mismatched assignment
    assert_rejects_single_e1002(
        r#"
fn main() {
    ch := make(chan int64, 1)
    v := <-ch
    var x int32 = v
    _ = x
}
"#,
    );

    // Comma-ok receive propagated into mismatched assignment
    assert_rejects_single_e1002(
        r#"
fn main() {
    ch := make(chan int64, 1)
    v, ok := <-ch
    var x int32 = v
    _ = x
    _ = ok
}
"#,
    );

    // Select receive arm binding propagated into mismatched assignment
    assert_rejects_single_e1002(
        r#"
fn main() {
    ch := make(chan int64, 1)
    select {
    v := <-ch:
        var x int32 = v
        _ = x
    default:
        _ = 0
    }
}
"#,
    );

    // For-range over channel propagated into mismatched assignment
    assert_rejects_single_e1002(
        r#"
fn main() {
    ch := make(chan int64, 1)
    for v in ch {
        var x int32 = v
        _ = x
    }
}
"#,
    );
}

#[test]
fn matching_widths_untyped_consts_and_inner_shadowing_accept() {
    assert_accepts(
        r#"
type i64_stream = chan int64

struct pipe {
    ch chan int64
}

fn main() {
    ch := make(chan int64, 2)
    var x int64 = 10
    ch <- x
    ch <- 42
    v := <-ch
    var y int64 = v
    _ = y

    v2, ok := <-ch
    var y2 int64 = v2
    _ = y2
    _ = ok

    p := pipe{ch: ch}
    p.ch <- x

    var aliased i64_stream = make(i64_stream, 1)
    aliased <- x
}
"#,
    );

    // Inner-scope shadowing of `ch` with a different element width must not leak outer width
    assert_accepts(
        r#"
fn main() {
    var ch chan int64 = make(chan int64, 1)
    if true {
        ch := make(chan int32, 1)
        var x int32 = 5
        ch <- x
        v := <-ch
        var y int32 = v
        _ = y
    }
    var outer_x int64 = 9
    ch <- outer_x
}
"#,
    );
}

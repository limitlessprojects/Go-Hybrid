use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_has(r: &hy_check::CheckResult, code: &str) {
    let cs = error_codes(r);
    assert!(
        cs.iter().any(|c| c == code),
        "expected {}, got {:?}",
        code,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

fn assert_accepts(src: &str) {
    let r = check(src);
    let errs = error_codes(&r);
    assert!(
        errs.is_empty(),
        "expected clean accept, got {:?}\nsrc: {}",
        errs,
        src
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn match_guard_resolves_some_binding_channel_and_shadowing() {
    // 1. Valid `option[chan int]` payload received in guard must be accepted.
    assert_accepts(
        r#"package main

fn probe(opt option[chan int]) {
    match opt {
        some(ch) if (<-ch) > 0: {}
        some(_): {}
        none: {}
    }
}
"#,
    );

    // 2. `option[int]` payload `v` shadowing an outer `v: chan int` must be
    //    rejected with HYB-E2008 when received from inside the guard.
    let r_shadow_int = check(
        r#"package main

fn probe(v chan int, opt option[int]) {
    match opt {
        some(v) if (<-v) > 0: {}
        some(_): {}
        none: {}
    }
}
"#,
    );
    assert_has(&r_shadow_int, "HYB-E2008");

    // 3. `option[chan<- int]` payload shadowing an outer bidirectional `ch`
    //    must be rejected in the guard. The extended channel-direction walker
    //    (HYB-E3012 work) now classifies the receive-from-send-only first as
    //    HYB-E3008 (`chan-direction-mismatch`, the precise code for this
    //    error) before the coarser receive-non-channel fallthrough.
    let r_shadow_send = check(
        r#"package main

fn probe(ch chan int, opt option[chan<- int]) {
    match opt {
        some(ch) if (<-ch) > 0: {}
        some(_): {}
        none: {}
    }
}
"#,
    );
    assert_has(&r_shadow_send, "HYB-E3008");
}

#[test]
fn closure_if_init_multi_value_and_parallel_shadowing_checked_in_cond() {
    // 1. Valid 2-value receive in closure `if` init (`if inner, ok := <-outer; (<-inner) > 0`)
    //    must resolve `inner` as `chan int` and accept.
    assert_accepts(
        r#"package main

fn main() {
    f := fn(outer chan chan int) {
        if inner, ok := <-outer; ok && (<-inner) > 0 {
        }
    }
    _ = f
}
"#,
    );

    // 2. 2-value receive in closure `if` init shadowing an outer `v: chan int`
    //    with an `int` payload (`if v, ok := <-int_ch; (<-v) > 0`) must reject `<-v`.
    let r_multi_shadow = check(
        r#"package main

fn main() {
    f := fn(v chan int, int_ch chan int) {
        if v, ok := <-int_ch; ok && (<-v) > 0 {
        }
    }
    _ = f
}
"#,
    );
    assert_has(&r_multi_shadow, "HYB-E2008");

    // 3. Parallel `if` init (`if x, y := 1, x; (<-y) > 0`) must evaluate RHS `x`
    //    against the pre-statement scope (`x: chan int`), not the newly bound `x := 1`.
    assert_accepts(
        r#"package main

fn main() {
    f := fn(x chan int) {
        if x, y := 1, x; x > 0 && (<-y) > 0 {
        }
    }
    _ = f
}
"#,
    );
}

#[test]
fn local_array_length_non_channel_receive_rejected() {
    let r = check(
        r#"package main

fn main() {
    bad := 42
    var a [<-bad]int
    _ = a
}
"#,
    );
    assert_has(&r, "HYB-E2008");
}

#[test]
fn nested_receive_from_match_bound_channel_of_send_only_channel_rejected() {
    let r_body = check(
        r#"package main

fn drain(opt option[chan chan<- int]) {
    match opt {
        some(outer): {
            v := <-(<-outer)
            _ = v
        }
        none: {}
    }
}
"#,
    );
    // The extended channel-direction walker classifies the outer receive
    // (operand is the send-only element type) as HYB-E3008 first; the program
    // is equally rejected, with the more precise direction diagnostic.
    assert_has(&r_body, "HYB-E3008");

    let r_select = check(
        r#"package main

fn drain(opt option[chan chan<- int]) {
    match opt {
        some(outer): {
            select {
                v := <-(<-outer): {
                    _ = v
                }
                default: {}
            }
        }
        none: {}
    }
}
"#,
    );
    assert_has(&r_select, "HYB-E3008");
}

#[test]
fn addr_of_option_chan_or_atomic_across_task_and_chan_rejected() {
    let r_task_chan = check(
        r#"package main

fn worker(p *option[chan int]) {
    _ = p
}

fn main() {
    ch := make(chan int, 1)
    opt := some(ch)
    task worker(&opt)
}
"#,
    );
    assert_has(&r_task_chan, "HYB-E3001");

    let r_send_atomic = check(
        r#"package main
import "sync/atomic"

fn main() {
    var a atomic.int64
    opt := some(a)
    out := make(chan *option[atomic.int64], 1)
    out <- &opt
}
"#,
    );
    assert_has(&r_send_atomic, "HYB-E3002");
}

#[test]
fn pointer_to_option_atomic_in_struct_field_and_return_rejected() {
    let r = check(
        r#"package main
import "sync/atomic"

struct holder {
    p *option[atomic.int64]
}

fn escape() *option[atomic.int64] {
    var a atomic.int64
    opt := some(a)
    p := &opt
    return p
}
"#,
    );
    assert_has(&r, "HYB-E3103");
    assert_has(&r, "HYB-E3104");
}

#[test]
fn send_recv_and_range_directly_on_option_rejected() {
    let r_send = check(
        r#"package main

fn bad_send(opt option[chan int]) {
    opt <- 1
}
"#,
    );
    assert_has(&r_send, "HYB-E3007");

    let r_recv = check(
        r#"package main

fn bad_recv(opt option[chan int]) {
    x := <-opt
    _ = x
}
"#,
    );
    assert_has(&r_recv, "HYB-E2008");

    let r_range = check(
        r#"package main

fn bad_range(opt_xs option[[]int], opt_arr option[[2]int]) {
    for _, x in opt_xs {
        _ = x
    }
    for _, y in &opt_arr {
        _ = y
    }
}
"#,
    );
    let codes = error_codes(&r_range);
    assert!(
        codes.iter().filter(|c| *c == "HYB-E2009").count() >= 2,
        "expected HYB-E2009 for both opt_xs and &opt_arr, got {:?}",
        r_range.diagnostics
    );
}

#[test]
fn option_chan_by_value_and_match_unwrapped_ops_accepted() {
    assert_accepts(
        r#"package main

type opt_ch_t = option[chan int]

fn worker(opt opt_ch_t) {
    match opt {
        some(ch): {
            ch <- 1
            v := <-ch
            _ = v
        }
        none: {}
    }
}

fn main() {
    ch := make(chan int, 1)
    var opt opt_ch_t = some(ch)
    task worker(opt)
}
"#,
    );
}

#[test]
fn option_from_chan_receive_select_and_range_preserves_option_wrapper() {
    let r_recv = check(
        r#"package main

fn worker(p *option[chan int]) {
    _ = p
}

fn bad_recv_local(ch chan option[chan int]) {
    opt := <-ch
    opt <- 1
    x := <-opt
    _ = x
    task worker(&opt)
    opt2, ok := <-ch
    _ = ok
    opt2 <- 2
}
"#,
    );
    assert_has(&r_recv, "HYB-E3007");
    assert_has(&r_recv, "HYB-E2008");
    assert_has(&r_recv, "HYB-E3001");

    let r_select = check(
        r#"package main

fn worker(p *option[chan int]) {
    _ = p
}

fn bad_select_recv(ch chan option[chan int]) {
    select {
        opt := <-ch: {
            opt <- 1
            x := <-opt
            _ = x
            task worker(&opt)
        }
        default: {}
    }
}
"#,
    );
    assert_has(&r_select, "HYB-E3007");
    assert_has(&r_select, "HYB-E2008");
    assert_has(&r_select, "HYB-E3001");

    let r_for = check(
        r#"package main

fn worker(p *option[chan int]) {
    _ = p
}

fn bad_for_range(xs []option[chan int]) {
    for _, opt in xs {
        opt <- 1
        x := <-opt
        _ = x
        task worker(&opt)
    }
}
"#,
    );
    assert_has(&r_for, "HYB-E3007");
    assert_has(&r_for, "HYB-E2008");
    assert_has(&r_for, "HYB-E3001");
}

#[test]
fn nested_option_reassigned_to_none_and_new_option_alias_rejected() {
    let r_nested = check(
        r#"package main

fn bad_nested() {
    var opt option[option[chan int]] = some(some(make(chan int, 1)))
    opt = none
    match opt {
        some(inner): {
            inner <- 1
        }
        none: {}
    }
}
"#,
    );
    assert_has(&r_nested, "HYB-E3007");

    let r_new_alias = check(
        r#"package main
import "sync/atomic"

type opt_atomic = option[atomic.int64]

fn worker(p *opt_atomic) {
    _ = p
}

fn bad_new_alias() {
    p := new(opt_atomic)
    task worker(p)
}
"#,
    );
    assert_has(&r_new_alias, "HYB-E3001");
}


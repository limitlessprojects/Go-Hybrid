//! Regression tests for #3756: `recv.rs` must enforce `HYB-E2008` (and avoid
//! false `HYB-E3005` on bidirectional channels) across:
//! 1. `(&arr)[0]` / `(&b).ch` (`Expr::AddrOf` / `UnOp::Addr`) and composite
//!    calls (`some(chs[0:1]).unwrap()`, `none.or(chs[0:1])`, `.pop()`, `(<-fn_ch)()`).
//! 2. `Stmt::If` single-value `composite_type_expr` and two-value comma-ok
//!    (`<-ch_of_send`, `maps[0:1][0]["k"]`) fallbacks.
//! 3. `Stmt::Match` guard `option.some(x)` binder and `composite_type_expr`
//!    subject fallback (`match opts[0:1][0]`, `match <-opt_ch`).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).expect("must parse");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_has(src: &str, code: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == code),
        "expected {code}, got {cs:?} for:\n{src}"
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

fn assert_accepts(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(cs.is_empty(), "expected accept, got {cs:?} for:\n{src}");
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn addr_of_array_and_struct_field_send_only_recv_rejected_3756() {
    assert_has(
        r#"package main
fn bad() {
    var arr [1]chan<- int
    var a [<-(&arr)[0]]int
    _ = a
}
"#,
        "HYB-E2008",
    );
    assert_has(
        r#"package main
struct box_ch { ch chan<- int }
fn bad(b box_ch) {
    var a [<-(&b).ch]int
    _ = a
}
"#,
        "HYB-E2008",
    );
}

#[test]
fn addr_of_array_and_struct_field_bidirectional_accepts_3756() {
    assert_accepts(
        r#"package main
fn ok(arr [1]chan int) int {
    return <-(&arr)[0]
}
"#,
    );
    assert_accepts(
        r#"package main
struct box_ch { ch chan int }
fn ok(b box_ch) int {
    return <-(&b).ch
}
"#,
    );
}

#[test]
fn composite_some_unwrap_none_or_pop_and_callable_recv_rejected_3756() {
    assert_has(
        r#"package main
fn bad(chs []chan<- int) {
    var b [<-some(chs[0:1]).unwrap()[0]]int
    _ = b
}
"#,
        "HYB-E2008",
    );
    assert_has(
        r#"package main
fn bad(chs []chan<- int) {
    var b [<-none.or(chs[0:1])[0]]int
    _ = b
}
"#,
        "HYB-E2008",
    );
    assert_has(
        r#"package main
fn bad(chs []chan<- int) {
    var b [<-chs[0:1].pop()]int
    _ = b
}
"#,
        "HYB-E2008",
    );
    assert_has(
        r#"package main
fn bad(fn_ch chan fn() chan<- int) {
    var b [<-(<-fn_ch)()]int
    _ = b
}
"#,
        "HYB-E2008",
    );
}

#[test]
fn if_init_composite_and_comma_ok_send_only_recv_rejected_3756() {
    assert_has(
        r#"package main
fn bad(chs []chan<- int, ch chan<- int) {
    if sub := append(chs, ch); len([<-sub[0]]int{}) > 0 {}
}
"#,
        "HYB-E2008",
    );
    assert_has(
        r#"package main
fn bad(chs []chan<- int) {
    if sub := chs[0:1]; len([<-sub[0]]int{}) > 0 {}
}
"#,
        "HYB-E2008",
    );
    assert_has(
        r#"package main
fn bad(ch_of_send chan chan<- int) {
    if ch, ok := <-ch_of_send; ok && len([<-ch]int{}) > 0 {}
}
"#,
        "HYB-E2008",
    );
    assert_has(
        r#"package main
fn bad(maps []map[string]chan<- int) {
    if ch, ok := maps[0:1][0]["k"]; ok && len([<-ch]int{}) > 0 {}
}
"#,
        "HYB-E2008",
    );
}

#[test]
fn match_guard_option_some_shadow_and_composite_subject_rejected_3756() {
    assert_has(
        r#"package main
fn bad(opt option[int]) {
    x := make(chan int, 1)
    match opt {
        option.some(x) if (<-x) > 0 => {}
        _ => {}
    }
    _ = x
}
"#,
        "HYB-E2008",
    );
    assert_has(
        r#"package main
fn bad(opts []option[chan<- int]) {
    match opts[0:1][0] {
        some(ch) if len([<-ch]int{}) > 0 => {}
        _ => {}
    }
}
"#,
        "HYB-E2008",
    );
    assert_has(
        r#"package main
fn bad(opt_ch chan option[chan<- int]) {
    match <-opt_ch {
        option.some(ch) if len([<-ch]int{}) > 0 => {}
        _ => {}
    }
}
"#,
        "HYB-E2008",
    );
}

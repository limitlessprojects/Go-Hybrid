//! Regression tests for `HYB-E2009` (`range_iter.rs` / rule LX13):
//! 1. `for a, b, c in xs` (3+ iteration variables on slices, arrays, strings,
//!    maps, and inside closures) must reject with `HYB-E2009` instead of
//!    passing `hyb check` and failing in `hy-emit` / `go build`.
//! 2. `for i in lo..hi` / `for i in lo..=hi` (`ForKind::Range`) with
//!    non-integer bounds (`"a".."z"`, `true..false`, `1.5..3.5`, `0..xs`)
//!    must reject with `HYB-E2009`.
//! 3. Ranging over an alias of a send-only channel (`ch2 := ch` where `ch`
//!    is `chan<- int`) must reject with `HYB-E2009`.

use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let f = parse_file("test.hy", src).expect("parse ok");
    check_file(&f)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn for_in_with_three_or_more_variables_rejects_e2009() {
    for iter_decl in [
        "xs := []int{1, 2, 3}",
        "var xs [3]int",
        "xs := \"abc\"",
        "xs := map[string]int{\"a\": 1}",
    ] {
        let src = format!(
            r#"
fn main() {{
    {iter_decl}
    for a, b, c in xs {{
        _ = a
        _ = b
        _ = c
    }}
}}
"#
        );
        let c = codes(&src);
        assert!(
            c.contains(&"HYB-E2009".to_string()),
            "expected HYB-E2009 for 3 loop variables over `{iter_decl}`, got: {c:?}"
        );
    }

    // Also inside a closure body (A5 perimeter)
    let closure_src = r#"
fn main() {
    f := fn() {
        xs := []int{1, 2}
        for a, b, c in xs {
            _ = a
            _ = b
            _ = c
        }
    }
    f()
}
"#;
    assert!(
        codes(closure_src).contains(&"HYB-E2009".to_string()),
        "expected HYB-E2009 for 3 loop variables inside a closure"
    );
}

#[test]
fn for_range_with_non_integer_bounds_rejects_e2009() {
    let cases = [
        "for i in \"a\"..\"z\" { _ = i }",
        "for i in true..false { _ = i }",
        "for i in 1.5..3.5 { _ = i }",
        "for i in 0..=2.5 { _ = i }",
        "xs := []int{1, 2}\n    for i in 0..xs { _ = i }",
        "var f float64 = 10.0\n    for i in 0..f { _ = i }",
        "var b bool = true\n    for i in b..10 { _ = i }",
    ];
    for body in cases {
        let src = format!(
            r#"
fn main() {{
    {body}
}}
"#
        );
        let c = codes(&src);
        assert!(
            c.contains(&"HYB-E2009".to_string()),
            "expected HYB-E2009 for non-integer range bound `{body}`, got: {c:?}"
        );
    }
}

#[test]
fn for_in_over_send_only_channel_alias_rejects_e2009() {
    let src = r#"
fn drain(ch chan<- int) {
    var ch2 = ch
    for v in (ch2) {
        _ = v
    }
}

fn main() {}
"#;
    let c = codes(src);
    assert!(
        c.contains(&"HYB-E2009".to_string()),
        "expected HYB-E2009 when ranging over parenthesized/aliased send-only channel, got: {c:?}"
    );
}

#[test]
fn valid_integer_ranges_and_two_var_iterations_stay_accepted() {
    let src = r#"
fn main() {
    xs := []int{10, 20, 30}
    for i in 0..len(xs) {
        _ = i
    }
    for i in 0..=2 {
        _ = i
    }
    for i, v in xs {
        _ = i
        _ = v
    }
}
"#;
    let c = codes(src);
    assert!(
        c.is_empty(),
        "expected 0 diagnostics for valid integer ranges and 2-var iteration, got: {c:?}"
    );
}

#[test]
fn indirect_send_only_channel_iterations_reject_e2009_issue_1829() {
    let src_receive = r#"
type tx_chan = chan<- int

fn repro(ch_of_tx <-chan tx_chan) {
    for a in <-ch_of_tx {
        _ = a
    }
}
"#;
    let c = codes(src_receive);
    assert!(
        c.contains(&"HYB-E2009".to_string()),
        "expected HYB-E2009 for `for a in <-ch_of_tx`, got: {c:?}"
    );

    let src_loop_var = r#"
fn repro(txs []chan<- int) {
    for ch in txs {
        for b in ch {
            _ = b
        }
    }
}
"#;
    let c = codes(src_loop_var);
    assert!(
        c.contains(&"HYB-E2009".to_string()),
        "expected HYB-E2009 for iterating loop-bound send-only channel `for b in ch`, got: {c:?}"
    );

    let src_aliased_struct = r#"
struct holder {
    ch chan<- int
}

type holder_alias = holder

fn repro(s *holder_alias) {
    for c in s.ch {
        _ = c
    }
}
"#;
    let c = codes(src_aliased_struct);
    assert!(
        c.contains(&"HYB-E2009".to_string()),
        "expected HYB-E2009 for `for c in s.ch` on aliased pointer receiver, got: {c:?}"
    );
}

#[test]
fn compound_and_chained_non_int_consts_and_typed_bounds_reject_e2009_issue_1829() {
    let src_compound_const = r#"
const b_not = !true
const f_neg = -1.5
const f_chain = f_neg

fn repro() {
    for i in 0..b_not {
        _ = i
    }
    for j in 0..f_chain {
        _ = j
    }
}
"#;
    let c = codes(src_compound_const);
    assert!(
        c.contains(&"HYB-E2009".to_string()),
        "expected HYB-E2009 for compound/chained non-int consts in range bounds, got: {c:?}"
    );

    let src_typed_bounds = r#"
fn repro(x int8, u u32) {
    for k in 0..(-x) {
        _ = k
    }
    for m in 0..(u << 1) {
        _ = m
    }
    for n in (-x)..300 {
        _ = n
    }
}
"#;
    let c = codes(src_typed_bounds);
    assert!(
        c.contains(&"HYB-E2009".to_string()),
        "expected HYB-E2009 for fixed-width int typed unary/shift range bound mismatches, got: {c:?}"
    );
}

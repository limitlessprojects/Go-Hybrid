//! Regression tests for #3699:
//! 1. `len(s)` on a prelude `set[T]` returns `int`, so `for i in lo..len(s)`
//!    with a fixed-width non-`int` bound (`var lo i64 = 0`) must reject with
//!    `HYB-E2009` (`mismatched types int64 and int`).
//! 2. `move`-wrapped non-integer range bounds (`const f = 2.5; for i in 0..(move f)`),
//!    `move`-wrapped typed integer bounds (`var n i64 = 5; for i in 0..(move n)`),
//!    and `move`-wrapped send-only channel iterables (`ch2 := ch; for v in move ch2`)
//!    must reject with `HYB-E2009`.

use hy_check::{check_file, CheckResult, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("test.hy", src).expect("test program must parse");
    check_file(&ast)
}

fn codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

#[test]
fn range_bounds_reject_set_len_mismatch_and_move_wrapped_bounds_3699() {
    let cases = [
        "fn main() {\n    var lo i64 = 0\n    s := set[int]{}\n    for i in lo..len(s) {\n        _ = i\n    }\n    _ = s\n}\n",
        "const f = 2.5\nfn main() {\n    for i in 0..(move f) {\n        _ = i\n    }\n}\n",
        "const k i64 = 5\nfn main() {\n    for i in 0..(move k) {\n        _ = i\n    }\n}\n",
        "fn main() {\n    var n i64 = 5\n    for i in 0..(move n) {\n        _ = i\n    }\n}\n",
        "fn drain(ch chan<- int) {\n    ch2 := ch\n    for v in move ch2 {\n        _ = v\n    }\n}\nfn main() {}\n",
    ];
    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected Reject for:\n{src}");
        assert!(
            codes(&r).iter().any(|c| c == "HYB-E2009"),
            "expected HYB-E2009 for:\n{src}\ngot: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn range_bounds_accept_valid_set_len_and_move_wrapped_int_bounds_3699() {
    let src = "\
const u = 5

fn main() {
    s := set[int]{}
    for i in 0..len(s) {
        _ = i
    }
    for i in 0..(move u) {
        _ = i
    }
    var a i64 = 0
    var b i64 = 3
    for i in (move a)..(move b) {
        _ = i
        break
    }
    _ = s
}
";
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected Accept, got diagnostics: {:?}",
        r.diagnostics
    );
}

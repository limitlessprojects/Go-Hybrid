//! Regression tests for `range_iter.rs` (`HYB-E2009` / SPEC-LANGUAGE LX13):
//! 1. Non-integer range bounds in `for i in lo..hi` / `for i in lo..=hi`:
//!    - package-level typed non-integer `const`s initialized from integer
//!      literals (`const f float64 = 2`, `type my_float = float64; const k my_float = 2`),
//!    - `option[T]` bounds (`some(5)`, `none`, `var o option[int] = some(5)`),
//!    - `any` bounds (`fn f(x any) { for i in 0..x {} }`),
//!    - `complex(...)`, `real(...)`, and `imag(...)` builtin calls.
//! 2. Typed integer bound mismatches and overflows in `for i in lo..hi`:
//!    - unary negation (`-n`) and shift (`n << 1`, `n >> 1`) over typed integer
//!      bounds,
//!    - binary arithmetic/bitwise/shift expressions containing package-level
//!      typed integer `const`s (`const k i64 = 5; for i in 0..(k + 1)`),
//!    - runtime `len(slice_or_map_or_chan)`, `cap(slice_or_chan)`, and
//!      `copy(dst, src)` calls against non-`int` counter bounds.
//! 3. `for x in e` (`ForKind::Iterate`) iterable checks:
//!    - user-declared `struct set` / `struct set[t]` must not spoof the prelude
//!      `set[T]` iterable,
//!    - composite send-only channel expressions (`<-outer`, `append(chs, ch)[0]`,
//!      `chs[:1][0]`) must reject with `HYB-E2009`.

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
fn range_bounds_reject_typed_float_consts_options_any_and_complex_builtins_e2009() {
    let cases = [
        "const f float64 = 2\nfn main() {\n    for i in 0..f {\n        _ = i\n    }\n}\n",
        "const f f32 = 2\nfn main() {\n    for i in 0..(f + 1) {\n        _ = i\n    }\n}\n",
        "type my_float = float64\nconst k my_float = 2\nfn main() {\n    for i in 0..k {\n        _ = i\n    }\n}\n",
        "fn main() {\n    for i in 0..some(5) {\n        _ = i\n    }\n}\n",
        "fn main() {\n    for i in 0..none {\n        _ = i\n    }\n}\n",
        "fn main() {\n    var o option[int] = some(5)\n    for i in 0..o {\n        _ = i\n    }\n}\n",
        "fn run(x any) {\n    for i in 0..x {\n        _ = i\n    }\n}\nfn main() {\n    run(3)\n}\n",
        "fn main() {\n    for i in 0..complex(1.0, 2.0) {\n        _ = i\n    }\n}\n",
        "const c = complex(1.0, 2.0)\nfn main() {\n    for i in 0..real(c) {\n        _ = i\n    }\n}\n",
        "const c = complex(1.0, 2.0)\nfn main() {\n    for i in 0..imag(c) {\n        _ = i\n    }\n}\n",
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
fn range_bounds_reject_compound_typed_int_mismatches_and_overflows_e2009() {
    let cases = [
        "fn main() {\n    var n i64 = 5\n    for i in 0..-n {\n        _ = i\n    }\n}\n",
        "fn main() {\n    var n i8 = 5\n    for i in -n..300 {\n        _ = i\n    }\n}\n",
        "fn main() {\n    var n i64 = 1\n    for i in 0..(n << 2) {\n        _ = i\n    }\n}\n",
        "fn main() {\n    var n i8 = 1\n    for i in (n << 1)..300 {\n        _ = i\n    }\n}\n",
        "const k i64 = 5\nfn main() {\n    for i in 0..(k + 1) {\n        _ = i\n    }\n}\n",
        "const k i8 = 1\nfn main() {\n    for i in (k + 1)..300 {\n        _ = i\n    }\n}\n",
        "fn main() {\n    var lo i64 = 0\n    xs := []int{1, 2}\n    for i in lo..len(xs) {\n        _ = i\n    }\n}\n",
        "fn main() {\n    var lo i64 = 0\n    xs := []int{1, 2}\n    for i in lo..cap(xs) {\n        _ = i\n    }\n}\n",
        "fn main() {\n    var lo i64 = 0\n    a := []int{1}\n    b := []int{2}\n    for i in lo..copy(a, b) {\n        _ = i\n    }\n}\n",
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
fn range_iterate_rejects_user_struct_set_and_composite_send_only_channels_e2009() {
    let cases = [
        "struct set {\n    x int\n}\nfn main() {\n    s := set{x: 1}\n    for v in s {\n        _ = v\n    }\n}\n",
        "struct set[t] {\n    x t\n}\nfn main() {\n    s := set[int]{x: 1}\n    for v in s {\n        _ = v\n    }\n}\n",
        "fn drain(outer chan chan<- int) {\n    for v in <-outer {\n        _ = v\n    }\n}\nfn main() {}\n",
        "fn drain(outer chan []chan<- int) {\n    for v in (<-outer)[0] {\n        _ = v\n    }\n}\nfn main() {}\n",
        "fn drain(chs []chan<- int, ch chan<- int) {\n    for v in append(chs, ch)[0] {\n        _ = v\n    }\n}\nfn main() {}\n",
        "fn drain(chs []chan<- int) {\n    for v in chs[:1][0] {\n        _ = v\n    }\n}\nfn main() {}\n",
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
fn range_bounds_and_iterables_valid_controls_accepted() {
    let src = "\
const u = 5
const s = \"abc\"

fn u32(x int) int {
    return x
}

fn main() {
    xs := []int{1, 2, 3}
    for i in 0..len(xs) {
        _ = i
    }
    for i in 0..(u + 1) {
        _ = i
    }
    for i in 0..u32(4) {
        _ = i
    }
    var lo i64 = 0
    for i in lo..len(s) {
        _ = i
    }
    var n i64 = 5
    for i in -n..n {
        _ = i
    }
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

//! Regression tests for `range_iter.rs` (`HYB-E2009` / `HYB-E3005`) across
//! composite `option[...]` and send-only `chan<- T` iterables, non-integer
//! `option[T]` / composite / complex-builtin range bounds, and unary/shift/
//! const-binary/composite typed integer range bounds.

use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).unwrap_or_else(|d| panic!("must parse: {d:?}"));
    check_file(&ast)
}

fn range_errors(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error && matches!(d.code.as_str(), "HYB-E2009" | "HYB-E3005"))
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect()
}

#[test]
fn composite_option_and_send_only_channel_iterables_reject_with_e2009() {
    for (label, needle, src) in [
        (
            "receive of `option[[]int]` is not iterable",
            "is not iterable",
            "fn f(ch chan option[[]int]) {\n for x in <-ch {\n fmt.println(\"{x}\")\n }\n}\nfn main() {}\n",
        ),
        (
            "`append(opts, o)[0]` of `option[[]int]` is not iterable",
            "is not iterable",
            "fn f(opts []option[[]int], o option[[]int]) {\n for x in append(opts, o)[0] {\n fmt.println(\"{x}\")\n }\n}\nfn main() {}\n",
        ),
        (
            "`opts[0..1][0]` of `option[string]` is not iterable",
            "is not iterable",
            "fn f(opts []option[string]) {\n for x in opts[0..1][0] {\n fmt.println(\"{x}\")\n }\n}\nfn main() {}\n",
        ),
        (
            "receive of send-only channel `<-outer`",
            "channel is send-only",
            "fn f(outer chan chan<- int) {\n for x in <-outer {\n fmt.println(\"{x}\")\n }\n}\nfn main() {}\n",
        ),
        (
            "`append(chs, ch)[0]` of send-only channel",
            "channel is send-only",
            "fn f(chs []chan<- int, ch chan<- int) {\n for x in append(chs, ch)[0] {\n fmt.println(\"{x}\")\n }\n}\nfn main() {}\n",
        ),
        (
            "struct field projection `(<-outer).ch` of send-only channel",
            "channel is send-only",
            "struct holder { ch chan<- int }\nfn f(outer chan holder) {\n for x in (<-outer).ch {\n fmt.println(\"{x}\")\n }\n}\nfn main() {}\n",
        ),
        (
            "option `.unwrap()` on composite `(<-outer).unwrap()` yielding send-only channel",
            "channel is send-only",
            "fn f(outer chan option[chan<- int]) {\n for x in (<-outer).unwrap() {\n fmt.println(\"{x}\")\n }\n}\nfn main() {}\n",
        ),
        (
            "2-variable range over `(<-outer).unwrap()` yielding bidirectional channel",
            "ranging over a channel yields only 1 value",
            "fn f(outer chan option[chan int]) {\n for a, b in (<-outer).unwrap() {\n fmt.println(\"{a} {b}\")\n }\n}\nfn main() {}\n",
        ),
    ] {
        let r = check(src);
        let errs = range_errors(&r);
        assert!(
            errs.iter().any(|e| e.starts_with("HYB-E2009") && e.contains(needle)),
            "{label} must emit HYB-E2009 mentioning `{needle}` for:\n{src}\ngot: {errs:?}"
        );
    }

    // Valid `.unwrap()` on composite `chan option[[]int]` must not false-reject with HYB-E3005.
    let valid = "fn f(outer chan option[[]int]) {\n for x in (<-outer).unwrap() {\n fmt.println(\"{x}\")\n }\n}\nfn main() {}\n";
    let r = check(valid);
    let errs = range_errors(&r);
    assert!(
        errs.is_empty(),
        "valid `(<-outer).unwrap()` yielding `[]int` must not emit HYB-E2009/E3005, got: {errs:?}"
    );
}

#[test]
fn non_integer_option_composite_and_complex_bounds_reject_with_e2009() {
    for (label, needle, src) in [
        (
            "`some(1)` bound in integer range",
            "(option[any]) is not an integer",
            "fn main() {\n for i in 0..some(1) {\n fmt.println(\"{i}\")\n }\n}\n",
        ),
        (
            "`option[int]` local bound in integer range",
            "(option[int]) is not an integer",
            "fn f(opt option[int]) {\n for i in 0..opt {\n fmt.println(\"{i}\")\n }\n}\nfn main() {}\n",
        ),
        (
            "receive from `chan float64` bound in integer range",
            "(float64) is not an integer",
            "fn f(ch chan float64) {\n for i in 0..(<-ch) {\n fmt.println(\"{i}\")\n }\n}\nfn main() {}\n",
        ),
        (
            "`append(fs, 1.0)[0]` bound in integer range",
            "(float64) is not an integer",
            "fn f(fs []float64) {\n for i in 0..append(fs, 1.0)[0] {\n fmt.println(\"{i}\")\n }\n}\nfn main() {}\n",
        ),
        (
            "`real(c)` bound in integer range",
            "(float64) is not an integer",
            "fn f(c complex128) {\n for i in 0..real(c) {\n fmt.println(\"{i}\")\n }\n}\nfn main() {}\n",
        ),
        (
            "`complex(1.0, 2.0)` bound in integer range",
            "(complex128) is not an integer",
            "fn main() {\n for i in 0..complex(1.0, 2.0) {\n fmt.println(\"{i}\")\n }\n}\n",
        ),
        (
            "`(<-outer).unwrap()` yielding `bool` bound in integer range",
            "(bool) is not an integer",
            "fn f(outer chan option[bool]) {\n for i in 0..(<-outer).unwrap() {\n fmt.println(\"{i}\")\n }\n}\nfn main() {}\n",
        ),
    ] {
        let r = check(src);
        let errs = range_errors(&r);
        assert!(
            errs.iter().any(|e| e.starts_with("HYB-E2009") && e.contains(needle)),
            "{label} must emit HYB-E2009 mentioning `{needle}` for:\n{src}\ngot: {errs:?}"
        );
    }
}

#[test]
fn unary_shift_const_binary_and_composite_typed_int_bounds_enforce_e2009() {
    for (label, needle, src) in [
        (
            "unary `-x` (`i8`) lower bound overflows constant `300` upper bound",
            "overflows the range counter's type `i8`",
            "fn main() {\n var x i8 = 5\n for i in (-x)..300 {\n fmt.println(\"{i}\")\n }\n}\n",
        ),
        (
            "unary `-x` (`i8`) upper bound mismatches untyped `0` lower bound",
            "lower to mismatched integer types",
            "fn main() {\n var x i8 = 5\n for i in 0..(-x) {\n fmt.println(\"{i}\")\n }\n}\n",
        ),
        (
            "shift `x << 1` (`i8`) lower bound overflows constant `300` upper bound",
            "overflows the range counter's type `i8`",
            "fn main() {\n var x i8 = 1\n for i in (x << 1)..300 {\n fmt.println(\"{i}\")\n }\n}\n",
        ),
        (
            "typed file-level `const c i8 = 5` in `c + 1` overflows constant `300`",
            "overflows the range counter's type `i8`",
            "const c i8 = 5\nfn main() {\n for i in (c + 1)..300 {\n fmt.println(\"{i}\")\n }\n}\n",
        ),
        (
            "composite receive `<-ch` (`chan i8`) lower bound overflows constant `300`",
            "overflows the range counter's type `i8`",
            "fn f(ch chan i8) {\n for i in (<-ch)..300 {\n fmt.println(\"{i}\")\n }\n}\nfn main() {}\n",
        ),
        (
            "composite receive `<-ch` (`chan i8`) upper bound mismatches untyped `0`",
            "lower to mismatched integer types",
            "fn f(ch chan i8) {\n for i in 0..(<-ch) {\n fmt.println(\"{i}\")\n }\n}\nfn main() {}\n",
        ),
    ] {
        let r = check(src);
        let errs = range_errors(&r);
        assert!(
            errs.iter().any(|e| e.starts_with("HYB-E2009") && e.contains(needle)),
            "{label} must emit HYB-E2009 mentioning `{needle}` for:\n{src}\ngot: {errs:?}"
        );
    }
}

//! channel_send_type_mismatch_2817.rs — issue #2817 (Row 13 BUILDABLE).
//!
//! Go §Send statements: "the type of the value to be sent must be assignable
//! to the channel's element type". `hy-check` ran only the #887 single-value
//! check on `Stmt::Send`, so a typed `float64` into a `chan int`, an `int32`
//! into a `chan int64`, or a `float64` into a `chan float32` produced
//! `Verdict::Accept` with 0 diagnostics — the emitted Go then failed
//! `go build` with `cannot use v (variable of type float64) as int value in
//! send`. Each mismatch (plain statement and `select` arm) must now be
//! rejected with `HYB-E1002`, while untyped-constant sends, exact-width
//! matches, and `chan any` stay accepted.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_rejects_e1002(src: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "{:?}",
        r.diagnostics
    );
}

// ---------------------------------------------------------------------------
// Reject: the issue's three minimal reproducers, verbatim
// ---------------------------------------------------------------------------

#[test]
fn float64_into_chan_int_rejects_e1002() {
    assert_rejects_e1002(
        "fn main() {\n\
         var ch chan int = make(chan int, 1)\n\
         var f float64 = 3.14\n\
         ch <- f\n\
         }\n",
    );
}

#[test]
fn int32_into_chan_int64_rejects_e1002() {
    assert_rejects_e1002(
        "fn main() {\n\
         var ch64 chan int64 = make(chan int64, 1)\n\
         var i32 int32 = 1\n\
         ch64 <- i32\n\
         }\n",
    );
}

#[test]
fn float64_into_chan_float32_rejects_e1002() {
    assert_rejects_e1002(
        "fn main() {\n\
         var chf32 chan float32 = make(chan float32, 1)\n\
         var f64 float64 = 1.0\n\
         chf32 <- f64\n\
         }\n",
    );
}

// ---------------------------------------------------------------------------
// Reject: other proven scalar conflicts (coarse kinds and signedness)
// ---------------------------------------------------------------------------

#[test]
fn string_into_chan_int_rejects_e1002() {
    assert_rejects_e1002(
        "fn main() {\n\
         var ch chan int = make(chan int, 1)\n\
         var s string = \"x\"\n\
         ch <- s\n\
         }\n",
    );
}

#[test]
fn bool_into_chan_int_rejects_e1002() {
    assert_rejects_e1002(
        "fn main() {\n\
         var ch chan int = make(chan int, 1)\n\
         var b bool = true\n\
         ch <- b\n\
         }\n",
    );
}

#[test]
fn typed_int_into_chan_float64_rejects_e1002() {
    // The (Float, Int) arm of the kind check stays open for untyped int
    // constants; a TYPED int variable is not one (#682 U5 disambiguation).
    assert_rejects_e1002(
        "fn main() {\n\
         var chf chan float64 = make(chan float64, 1)\n\
         var n int = 7\n\
         chf <- n\n\
         }\n",
    );
}

#[test]
fn signed_into_unsigned_chan_rejects_e1002() {
    assert_rejects_e1002(
        "fn main() {\n\
         var chu chan uint32 = make(chan uint32, 1)\n\
         var i int32 = 1\n\
         chu <- i\n\
         }\n",
    );
}

#[test]
fn select_send_mismatch_rejects_e1002() {
    assert_rejects_e1002(
        "fn main() {\n\
         var ch chan int64 = make(chan int64, 1)\n\
         var i32 int32 = 1\n\
         select {\n\
         ch <- i32:\n\
           _ = 0\n\
         default:\n\
           _ = 1\n\
         }\n\
         }\n",
    );
}

// ---------------------------------------------------------------------------
// Accept: controls that must not regress
// ---------------------------------------------------------------------------

#[test]
fn untyped_int_const_into_chan_float64_accepts() {
    // `ch <- 1` into `chan float64` builds (Go §Assignability: untyped
    // constants), so the check must not fire.
    let r = check(
        "fn main() {\n\
         var chf chan float64 = make(chan float64, 1)\n\
         chf <- 1\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn untyped_int_const_into_chan_int_accepts() {
    let r = check(
        "fn main() {\n\
         var ch chan int = make(chan int, 1)\n\
         ch <- 1\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn exact_width_match_accepts() {
    let r = check(
        "fn main() {\n\
         var ch64 chan int64 = make(chan int64, 1)\n\
         var i64 int64 = 1\n\
         ch64 <- i64\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn byte_rune_alias_spellings_accept() {
    // `byte` == `uint8` and `rune` == `int32` — identical types, not widths.
    let r = check(
        "fn main() {\n\
         var chb chan uint8 = make(chan uint8, 1)\n\
         var b byte = 1\n\
         chb <- b\n\
         var chr chan int32 = make(chan int32, 1)\n\
         var ru rune = 'x'\n\
         chr <- ru\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn any_chan_accepts_numeric_value() {
    let r = check(
        "fn main() {\n\
         var cha chan any = make(chan any, 1)\n\
         var i int = 1\n\
         cha <- i\n\
         cha <- \"str\"\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn string_chan_match_accepts() {
    let r = check(
        "fn main() {\n\
         var chs chan string = make(chan string, 1)\n\
         var s string = \"x\"\n\
         chs <- s\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn managed_hatch_suppresses_send_assignability() {
    // O6.1: the hatch's documented contract is "`managed` suppresses the
    // sendability check" — #2817's rule is a sendability check, so it stays
    // silent inside the hatch (the HYB-E8001 audit is the record). A
    // mismatched send OUTSIDE the hatch still rejects.
    let r = check(
        "fn main() {\n\
         var ch chan int = make(chan int, 1)\n\
         var m map[string]int = map[string]int{}\n\
         managed(\"audited share\", \"mutex\") { ch <- m }\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    assert!(
        !error_codes(&r).contains(&"HYB-E1002".to_string()),
        "the hatch must not be reopened: {:?}",
        r.diagnostics
    );
}

#[test]
fn same_mismatch_outside_hatch_still_rejects() {
    assert_rejects_e1002(
        "fn main() {\n\
         var ch chan int = make(chan int, 1)\n\
         var m map[string]int = map[string]int{}\n\
         ch <- m\n\
         }\n",
    );
}

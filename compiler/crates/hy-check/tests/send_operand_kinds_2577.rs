//! send_operand_kinds_2577.rs — regression for issue #2577: a channel send did
//! not compare the sent value's kind with the channel's element kind, so
//! `var ch chan float64; var i int = 1; ch <- i` passed `hyb check` and failed
//! `go build` (`cannot use i (variable of type int) as float64 value in send`).
//! The report described the `select` arm; the plain `Stmt::Send` had the same
//! hole, and both sites are covered here.
//!
//! Two properties matter and they pull in opposite directions:
//!
//! 1. a *typed* `int` operand in a `chan float64` is refused (HYB-E1002);
//! 2. a legal untyped constant conversion is NOT refused — `ch <- 1`,
//!    `ch <- 1 + 2` and `ch <- k` for `const k = 2` all build, and rejecting a
//!    buildable program is the regression this checker must never make.
//!
//! `kinds_conflict()` — the #682 assignment helper — deliberately fails open on
//! `(Float, Int)` for the untyped-constant reason, so the send sites narrow it:
//! `is_untyped_int_const` covers literal arithmetic, and the declaration's type
//! ascription separates an untyped `const k = 2` (convertible, accepted) from a
//! typed `const k int = 2` (refused by Go, rejected here). Standing down inside
//! a `managed(...)` block keeps the hatch's parity with the boundary rules.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src).unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn rejects_with(src: &str, code: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == code),
        "expected {} for {:?}, got {:?}",
        code,
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {:?}", src);
}

/// No HYB-E1002 at all — the shape of a program that must stay accepted.
fn accepts(src: &str) {
    let r = check(src);
    assert!(
        !error_codes(&r).iter().any(|c| c == "HYB-E1002"),
        "expected an accept for {:?}, got {:?}",
        src,
        error_codes(&r)
    );
}

const TYPED_INT_TO_FLOAT_CHAN: &str =
    "fn main() {\n    var ch chan float64\n    var i int = 1\n    ch <- i\n    _ = <-ch\n}\n";

const SELECT_TYPED_INT_TO_FLOAT_CHAN: &str =
    "fn main() {\n    var ch chan float64\n    var i int = 1\n    select {\n    ch <- i:\n    default:\n    }\n}\n";

#[test]
fn a_plain_send_of_a_typed_int_to_a_float_chan_is_rejected() {
    rejects_with(TYPED_INT_TO_FLOAT_CHAN, "HYB-E1002");
}

#[test]
fn a_select_send_of_a_typed_int_to_a_float_chan_is_rejected() {
    // The arm the report described.
    rejects_with(SELECT_TYPED_INT_TO_FLOAT_CHAN, "HYB-E1002");
}

#[test]
fn a_string_sent_to_an_int_chan_is_rejected() {
    rejects_with(
        "fn main() {\n    var ch chan int\n    var s string = \"x\"\n    ch <- s\n    _ = <-ch\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn untyped_int_literals_stay_accepted_on_both_send_sites() {
    accepts("fn main() {\n    var ch chan float64\n    ch <- 1\n    _ = <-ch\n}\n");
    accepts("fn main() {\n    var ch chan float64\n    select {\n    ch <- 1:\n    default:\n    }\n}\n");
    // A constant expression is still a constant.
    accepts("fn main() {\n    var ch chan float64\n    select {\n    ch <- 1 + 2:\n    default:\n    }\n}\n");
}

#[test]
fn untyped_named_constants_stay_accepted() {
    accepts("fn main() {\n    const k = 2\n    var ch chan float64\n    ch <- k\n    _ = <-ch\n}\n");
    accepts("fn main() {\n    const k = 2\n    var ch chan float64\n    select {\n    ch <- k:\n    default:\n    }\n}\n");
}

#[test]
fn matching_and_float_operands_stay_accepted() {
    accepts("fn main() {\n    var ch chan float64\n    var f float64 = 1.5\n    ch <- f\n    _ = <-ch\n}\n");
    accepts("fn main() {\n    var ch chan float64\n    var f float64 = 1.5\n    select {\n    ch <- f:\n    default:\n    }\n}\n");
    accepts("fn main() {\n    var ch chan int\n    var i int = 1\n    ch <- i\n    _ = <-ch\n}\n");
}

#[test]
fn a_send_inside_a_managed_block_stands_down() {
    // The audited hatch is where the human takes responsibility — `--report-managed`
    // records the site (HYB-E8001), exactly as it does for the boundary rules. The
    // lib suite's `fc_closure_boundary_send_inside_managed_escapes` depends on this
    // parity, so it is pinned here too.
    accepts("fn main() {\n    var ch chan float64\n    var i int = 1\n    managed(\"the site is serialized\", \"mutex\") {\n        ch <- i\n    }\n}\n");
}

#[test]
fn a_typed_constant_is_rejected() {
    // `const k int = 2` is a *typed* constant: Go refuses `ch <- k` for a
    // `chan float64` (`cannot use k (constant of type int) as float64 value in
    // send`). The untyped `const k = 2` above is the case that must stay
    // accepted — the declaration's ascription is what separates them.
    rejects_with(
        "fn main() {\n    const k int = 2\n    var ch chan float64\n    select {\n    ch <- k:\n    default:\n    }\n}\n",
        "HYB-E1002",
    );
    rejects_with(
        "const k int = 2\n\nfn main() {\n    var ch chan float64\n    ch <- k\n    _ = <-ch\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn a_file_level_untyped_constant_stays_accepted() {
    accepts("const k = 2\n\nfn main() {\n    var ch chan float64\n    ch <- k\n    _ = <-ch\n}\n");
}

//! array_len_lencap.rs — regression for issue #1133: an array length that
//! is an inline `len`/`cap` call over a non-constant argument passed
//! `hyb check` with 0 diagnostics and then failed `go build` with
//! `invalid array length` (`[len(ch)]int`, `[cap(ch)-1]int`, ...).
//!
//! `check_array_len` (HYB-E6004) folds constant expressions at step 1 and
//! rejects non-constant bare identifiers at step 3, but section 4 ("fail
//! open: those can be legal constant expressions in Go") swallowed every
//! call — including calls Go's constant-expression rules can prove
//! non-constant. The fix keeps that fail-open except for `len`/`cap`
//! calls whose argument resolves (via `st.env_lookup` + `TypeShape`) to a
//! channel, slice, map, or non-`const` string binding: Go only treats
//! `len` of a *string constant* and `len`/`cap` of *arrays* as constant.
//!
//! Controls below pin the accept side so the fail-open survivors
//! (folded literals/arithmetic, string constants, array locals, `const`
//! bindings) cannot regress, and `len(int)` stays owned by the pre-
//! existing E2005 builtin-argument-kind rule (issue #187 family) so one
//! root never reports twice.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn rejects_with_e6004(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6004"),
        "expected HYB-E6004 for {:?}, got {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {:?}", src);
}

fn accepts_without_e6004(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E6004"),
        "must NOT fire HYB-E6004 for {:?}, got {:?}",
        src,
        cs
    );
}

// ---------------------------------------------------------------- reject:

#[test]
fn len_of_local_chan_rejects() {
    rejects_with_e6004(
        "fn main() {\n    ch := make(chan int, 4)\n    var xs [len(ch)]int\n    _ = xs\n}\n",
    );
}

#[test]
fn cap_of_local_chan_rejects() {
    rejects_with_e6004(
        "fn main() {\n    ch := make(chan int, 4)\n    var xs [cap(ch)]int\n    _ = xs\n}\n",
    );
}

#[test]
fn cap_minus_one_over_chan_rejects() {
    rejects_with_e6004(
        "fn main() {\n    ch := make(chan int, 2)\n    var xs [cap(ch)-1]int\n    _ = xs\n}\n",
    );
}

#[test]
fn len_of_local_slice_rejects() {
    rejects_with_e6004(
        "fn main() {\n    ys := []int{1, 2}\n    var xs [len(ys)]int\n    _ = xs\n}\n",
    );
}

#[test]
fn len_of_declared_string_rejects() {
    rejects_with_e6004("fn main() {\n    s := \"hello\"\n    var xs [len(s)]int\n    _ = xs\n}\n");
}

#[test]
fn len_of_let_string_rejects() {
    // `let`/`:=` both emit Go short declarations (a `var`) — see
    // hy-emit stmt.rs "checker-level contract" — so `len` is runtime.
    rejects_with_e6004("fn main() {\n    let s = \"hello\"\n    var xs [len(s)]int\n    _ = xs\n}\n");
}

#[test]
fn len_of_local_chan_in_composite_literal_rejects() {
    rejects_with_e6004(
        "fn main() {\n    ch := make(chan int, 4)\n    xs := [len(ch)]int{}\n    _ = xs\n}\n",
    );
}

// --------------------------------------------------------------- accept:

#[test]
fn folded_literal_arithmetic_accepts() {
    accepts_without_e6004("fn main() {\n    var xs [3]int\n    _ = xs\n}\n");
    accepts_without_e6004("fn main() {\n    var xs [1+2]int\n    _ = xs\n}\n");
}

#[test]
fn len_of_string_constant_accepts() {
    accepts_without_e6004("fn main() {\n    var xs [len(\"hello\")]int\n    _ = xs\n}\n");
    accepts_without_e6004("fn main() {\n    xs := [len(\"hello\")]int{}\n    _ = xs\n}\n");
    accepts_without_e6004("fn main() {\n    var xs [len(\"a\"+\"b\")]int\n    _ = xs\n}\n");
}

#[test]
fn len_of_file_const_string_accepts() {
    accepts_without_e6004(
        "const s = \"hello\"\nfn main() {\n    var xs [len(s)]int\n    _ = xs\n}\n",
    );
}

#[test]
fn len_of_fn_local_const_accepts() {
    // fn-local `const` emits Go `const`, so `len` is constant.
    accepts_without_e6004(
        "fn main() {\n    const s = \"hello\"\n    var xs [len(s)]int\n    _ = xs\n}\n",
    );
    accepts_without_e6004(
        "fn main() {\n    const n = 4\n    var xs [len(\"abcd\") + n]int\n    _ = xs\n}\n",
    );
}

#[test]
fn len_of_array_local_accepts() {
    // Go: len/cap of an array (type-determined) IS a constant expression.
    accepts_without_e6004(
        "fn main() {\n    var arr [3]int\n    var xs [len(arr)]int\n    _ = xs\n    _ = arr\n}\n",
    );
}

#[test]
fn len_of_int_stays_owned_by_e2005() {
    // The builtin-argument-kind rule (issue #187 family) already rejects
    // `len` of a primitive here — E6004 must not double-report the root.
    let r = check("fn main() {\n    n := 5\n    var xs [len(n)]int\n    _ = xs\n}\n");
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E2005"),
        "expected HYB-E2005 for len(int), got {:?}",
        cs
    );
    assert!(
        !cs.iter().any(|c| c == "HYB-E6004"),
        "E6004 must not double-report len(int) (E2005 owns it), got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

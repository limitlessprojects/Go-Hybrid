//! try_legality.rs — regression tests for issue #155:
//! '?' was never validated against its operand or enclosing scope in hy-check,
//! allowing non-error calls (option[int], int), closures without error returns,
//! and defer statements to pass check with 0 diagnostics and emit broken Go.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "try_test.hy";
    let ast = parse_file(path, src)
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

// ---------------------------------------------------------------------------
// Repro 1: '?' on option result -> HYB-E2003 (T20)
// ---------------------------------------------------------------------------
const REPRO_T20_OPTION: &str = "\
fn find(key string) option[int] {
    return some(42)
}

fn main() {
    v := find(\"a\")?
}
";

#[test]
fn repro_1_option_try_is_rejected_e2003() {
    let r = check(REPRO_T20_OPTION);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E2003"),
        "expected HYB-E2003 for '?' on option[int], got {:?}",
        codes
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

// ---------------------------------------------------------------------------
// Repro 2: '?' on non-error call -> HYB-E2003 (E4)
// ---------------------------------------------------------------------------
const REPRO_E4_INT: &str = "\
fn five() int {
    return 5
}

fn main() {
    x := five()?
}
";

#[test]
fn repro_2_non_error_try_is_rejected_e2003() {
    let r = check(REPRO_E4_INT);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E2003"),
        "expected HYB-E2003 for '?' on int return, got {:?}",
        codes
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

// ---------------------------------------------------------------------------
// Repro 3: '?' in closure without error return -> HYB-E2004 (E3b)
// ---------------------------------------------------------------------------
const REPRO_E3B_CLOSURE: &str = "\
fn fail() error {
    return nil
}

fn main() {
    cb := fn() {
        fail()?
    }
}
";

#[test]
fn repro_3_closure_without_error_return_is_rejected_e2004() {
    let r = check(REPRO_E3B_CLOSURE);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E2004"),
        "expected HYB-E2004 for '?' in non-error closure, got {:?}",
        codes
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

// ---------------------------------------------------------------------------
// Repro 4: '?' in defer statement -> HYB-E2001 (P / E3)
// ---------------------------------------------------------------------------
const REPRO_DEFER_TRY: &str = "\
fn cleanup() error {
    return nil
}

fn main() {
    defer cleanup()?
}
";

#[test]
fn repro_4_defer_try_is_rejected_e2001() {
    let r = check(REPRO_DEFER_TRY);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E2001"),
        "expected HYB-E2001 for '?' in defer, got {:?}",
        codes
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

// ---------------------------------------------------------------------------
// Positive Controls: Valid '?' usages accept with zero errors
// ---------------------------------------------------------------------------
const VALID_TRY_IN_MAIN: &str = "\
fn step() error {
    return nil
}

fn main() {
    step()?
}
";

#[test]
fn valid_try_in_main_accepts() {
    let r = check(VALID_TRY_IN_MAIN);
    let codes = error_codes(&r);
    assert!(
        codes.is_empty(),
        "valid '?' in main (E6) must accept with 0 errors, got {:?}",
        codes
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

const VALID_TRY_IN_ERROR_FN: &str = "\
fn load() (int, error) {
    return 10, nil
}

fn run() error {
    n := load()?
    return nil
}

fn main() {
    run()?
}
";

#[test]
fn valid_try_in_error_fn_accepts() {
    let r = check(VALID_TRY_IN_ERROR_FN);
    let codes = error_codes(&r);
    assert!(
        codes.is_empty(),
        "valid '?' in error-returning fn must accept, got {:?}",
        codes
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

const VALID_TRY_IN_ERROR_CLOSURE: &str = "\
fn query() error {
    return nil
}

fn main() {
    handler := fn() error {
        query()?
        return nil
    }
}
";

#[test]
fn valid_try_in_error_closure_accepts() {
    let r = check(VALID_TRY_IN_ERROR_CLOSURE);
    let codes = error_codes(&r);
    assert!(
        codes.is_empty(),
        "valid '?' in error-returning closure must accept, got {:?}",
        codes
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

const VALID_FOREIGN_OS_TRY: &str = "\
import \"os\"

fn read_cfg() error {
    data := os.read_file(\"config.json\")?
    return nil
}

fn main() {
    read_cfg()?
}
";

#[test]
fn valid_foreign_os_try_accepts() {
    let r = check(VALID_FOREIGN_OS_TRY);
    let codes = error_codes(&r);
    assert!(
        codes.is_empty(),
        "valid '?' on foreign os.read_file must accept, got {:?}",
        codes
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

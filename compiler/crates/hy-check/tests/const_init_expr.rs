//! const_init_expr.rs — regression for issue #205: a `const` whose
//! initializer is not a constant expression used to pass `hyb check`
//! (0 diagnostics) and then fail `go build` with
//! `get_val() (value of type T) is not constant`.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

const CALL_INIT: &str = "\
fn get_val() int {
    return 42
}

fn main() {
    const c = get_val()
    fmt.println(\"c: {c}\")
}
";

const FILE_LEVEL_CALL: &str = "\
fn get_val() int {
    return 42
}

const c = get_val()

fn main() {
    fmt.println(\"c: {c}\")
}
";

const VAR_REF: &str = "\
fn main() {
    x := 42
    const c = x
    fmt.println(\"c: {c}\")
}
";

const LIT_OK: &str = "\
fn main() {
    const c = 42
    fmt.println(\"c: {c}\")
}
";

const ARITH_OK: &str = "\
fn main() {
    const c = 1 + 2 * 3
    fmt.println(\"c: {c}\")
}
";

const NEG_OK: &str = "\
fn main() {
    const c = -1
    fmt.println(\"c: {c}\")
}
";

const CONV_OK: &str = "\
fn main() {
    const c = int8(42)
    fmt.println(\"c: {c}\")
}
";

const REF_OTHER_CONST_OK: &str = "\
const base = 10

fn main() {
    const c = base
    fmt.println(\"c: {c}\")
}
";

const COMPOSITE_OK: &str = "\
fn main() {
    const th = [10, 50, 250, 1000]
    fmt.println(\"{th[0]}\")
}
";

#[test]
fn call_initializer_is_rejected() {
    let r = check(CALL_INIT);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6003"),
        "expected HYB-E6003 for const c = get_val(), got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn file_level_call_initializer_is_rejected() {
    let r = check(FILE_LEVEL_CALL);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6003"),
        "expected HYB-E6003 for file-level const, got {:?}",
        cs
    );
}

#[test]
fn variable_ref_initializer_is_rejected() {
    let r = check(VAR_REF);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6003"),
        "expected HYB-E6003 for const c = x, got {:?}",
        cs
    );
}

#[test]
fn literal_initializer_still_accepted() {
    let r = check(LIT_OK);
    let cs = error_codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E6003"),
        "literal const must not fire HYB-E6003, got {:?}",
        cs
    );
}

#[test]
fn arithmetic_initializer_still_accepted() {
    let r = check(ARITH_OK);
    let cs = error_codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E6003"),
        "const arithmetic must not fire HYB-E6003, got {:?}",
        cs
    );
}

#[test]
fn negated_literal_still_accepted() {
    let r = check(NEG_OK);
    let cs = error_codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E6003"),
        "negated literal must not fire HYB-E6003, got {:?}",
        cs
    );
}

#[test]
fn typed_conversion_of_constant_still_accepted() {
    let r = check(CONV_OK);
    let cs = error_codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E6003"),
        "int8(42) const must not fire HYB-E6003, got {:?}",
        cs
    );
}

#[test]
fn reference_to_other_const_still_accepted() {
    let r = check(REF_OTHER_CONST_OK);
    let cs = error_codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E6003"),
        "const-to-const ref must not fire HYB-E6003, got {:?}",
        cs
    );
}

#[test]
fn composite_literal_const_still_accepted() {
    // Emitter rewrites composites to `var`; must not fire #205.
    let r = check(COMPOSITE_OK);
    let cs = error_codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E6003"),
        "composite const must not fire HYB-E6003, got {:?}",
        cs
    );
}

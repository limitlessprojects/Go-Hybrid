//! const_init_not_constant.rs — regression for issue #205: a `const` whose
//! initializer is not a compile-time constant expression used to pass
//! `hyb check` (0 diagnostics) and then fail `go build` with
//! `get_val() (value of type int) is not constant`, because `hy-emit` keeps
//! the Go `const` keyword for every initializer that is not a composite
//! literal.

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

const LOCAL_CONST_CALL: &str = "\
fn get_val() int {
    return 42
}

fn main() {
    const c = get_val()
    fmt.println(\"{c}\")
}
";

const FILE_CONST_CALL: &str = "\
fn get_val() int {
    return 42
}

const k = get_val()

fn main() {
    fmt.println(\"{k}\")
}
";

const LOCAL_CONST_LITERAL_AND_ARITH: &str = "\
fn main() {
    const a = 1
    const b = a * 2 + 3
    const s = \"x\"
    const f = 1.5
    const t = true
    fmt.println(\"{a} {b} {s} {f} {t}\")
}
";

const FILE_CONST_REFERS_TO_CONST: &str = "\
const base = 10
const derived = base * 3

fn main() {
    const local = derived + 1
    fmt.println(\"{base} {derived} {local}\")
}
";

const COMPOSITE_CONSTS_ACCEPT: &str = "\
struct holder {
    m map[string]int
}

fn main() {
    const xs = [1, 2, 3]
    const m = map[string]int{\"k\": 1}
    const h = holder{m: m}
    fmt.println(\"{len(xs)} {len(m)} {len(h)}\")
}
";

const LET_BOUND_TO_CALL_ACCEPTS: &str = "\
fn get_val() int {
    return 42
}

fn main() {
    let c = get_val()
    d := get_val()
    fmt.println(\"{c} {d}\")
}
";

#[test]
fn local_const_bound_to_call_is_rejected() {
    let r = check(LOCAL_CONST_CALL);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6003"),
        "expected HYB-E6003 for `const c = get_val()`, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn file_level_const_bound_to_call_is_rejected() {
    let r = check(FILE_CONST_CALL);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6003"),
        "expected HYB-E6003 for file-level `const k = get_val()`, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn const_literals_and_arithmetic_accept() {
    let r = check(LOCAL_CONST_LITERAL_AND_ARITH);
    assert!(
        error_codes(&r).is_empty(),
        "literals and arithmetic over constants must stay clean, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn const_names_are_constants() {
    let r = check(FILE_CONST_REFERS_TO_CONST);
    assert!(
        error_codes(&r).is_empty(),
        "a `const` initializer may name other `const`s, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn composite_consts_stay_accepted() {
    // hy-emit renders a composite `const` initializer as Go `var`, so this
    // must stay out of the rule (issue #188 keeps its runtime-mutable
    // semantics).
    let r = check(COMPOSITE_CONSTS_ACCEPT);
    assert!(
        error_codes(&r).is_empty(),
        "composite const initializers render as `var` and must stay clean, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn let_bound_to_call_still_accepts() {
    // Control: only `const` is a compile-time constant. A `let`/`:=` binding
    // of the same call is a runtime binding and must stay accepted — the fix
    // narrows the const rule, it does not ban calls.
    let r = check(LET_BOUND_TO_CALL_ACCEPTS);
    assert!(
        error_codes(&r).is_empty(),
        "`let c = get_val()` is a runtime binding, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

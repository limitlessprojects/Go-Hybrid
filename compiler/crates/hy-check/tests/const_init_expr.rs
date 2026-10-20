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

#[test]
fn nil_const_initializer_is_rejected() {
    let local_nil = check("fn main() {\n    const c = nil\n    fmt.println(c)\n}\n");
    let cs = error_codes(&local_nil);
    assert!(
        cs.iter().any(|c| c == "HYB-E6003"),
        "expected HYB-E6003 for const c = nil, got {:?}",
        cs
    );

    let file_nil = check("const c = nil\nfn main() {\n    fmt.println(c)\n}\n");
    let file_cs = error_codes(&file_nil);
    assert!(
        file_cs.iter().any(|c| c == "HYB-E6003"),
        "expected HYB-E6003 for file-level const c = nil, got {:?}",
        file_cs
    );
}

#[test]
fn file_level_const_forward_type_alias_and_const_overflow_checked() {
    let ok_forward_alias = check(
        "const x = my_int(42)\ntype my_int = int8\nfn main() {\n    fmt.println(x)\n}\n",
    );
    let ok_cs = error_codes(&ok_forward_alias);
    assert!(
        !ok_cs.iter().any(|c| c == "HYB-E6003" || c == "HYB-E6002"),
        "forward type alias conversion of valid constant must pass, got {:?}",
        ok_cs
    );

    let overflow_forward_alias = check(
        "const x my_int = 300\ntype my_int = int8\nfn main() {\n    fmt.println(x)\n}\n",
    );
    let overflow_cs = error_codes(&overflow_forward_alias);
    assert!(
        overflow_cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for forward-aliased int8 overflow, got {:?}",
        overflow_cs
    );

    let overflow_forward_const = check(
        "const a int8 = b\nconst b = 300\nfn main() {\n    fmt.println(a)\n}\n",
    );
    let forward_const_cs = error_codes(&overflow_forward_const);
    assert!(
        forward_const_cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for forward-referenced const overflow, got {:?}",
        forward_const_cs
    );

    let shadow_forward_fn = check(
        "const x = int8(42)\nfn int8(n int) int {\n    return n\n}\nfn main() {\n    fmt.println(x)\n}\n",
    );
    let shadow_cs = error_codes(&shadow_forward_fn);
    assert!(
        shadow_cs.iter().any(|c| c == "HYB-E6003"),
        "expected HYB-E6003 when top-level fn shadows primitive conversion in file-level const, got {:?}",
        shadow_cs
    );
}


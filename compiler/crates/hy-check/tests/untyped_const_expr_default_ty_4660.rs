//! untyped_const_expr_default_ty_4660.rs — regression for issue #4660:
//! `x := 1 + 2` / `x := c` (untyped constant expressions and named untyped
//! consts) used to bind with no Go default type, so every width check
//! skipped the binding and `hyb check` passed a program `go build`
//! rejects. Go gives the binding its default type (`int`, `rune`,
//! `float64`); the scalar must be recorded the same way bare literals
//! already are.

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

const BINOP_EXPR_INTO_INT64: &str = "\
import \"fmt\"

fn main() {
    x := 1 + 2
    var y int64 = x
    fmt.println(y)
}
";

const LOCAL_CONST_INTO_UINT8_ADD: &str = "\
import \"fmt\"

fn main() {
    const c = 5
    x := c
    z := uint8(1) + x
    fmt.println(z)
}
";

const MOD_EXPR_INTO_INT64: &str = "\
import \"fmt\"

fn main() {
    x := 7 % 3
    var y int64 = x
    fmt.println(y)
}
";

const FLOAT_EXPR_INTO_INT64: &str = "\
import \"fmt\"

fn main() {
    x := 1.5 * 2
    var y int64 = x
    fmt.println(y)
}
";

const RUNE_EXPR_INTO_INT64: &str = "\
import \"fmt\"

fn main() {
    r := 'a' + 1
    var y int64 = r
    fmt.println(y)
}
";

const FILE_CONST_EXPR_INTO_INT64: &str = "\
import \"fmt\"

const k = 5

fn main() {
    x := k + 1
    var y int64 = x
    fmt.println(y)
}
";

#[test]
fn binop_const_expr_binds_int_and_rejects_int64() {
    let r = check(BINOP_EXPR_INTO_INT64);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for `var y int64 = x` on `x := 1 + 2`, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn local_untyped_const_binds_and_rejects_mixed_add() {
    // Issue shape 2: `const c = 5; x := c` gives x Go's default `int`, so
    // `uint8(1) + x` is a mismatched-type operation Go refuses.
    let r = check(LOCAL_CONST_INTO_UINT8_ADD);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for `uint8(1) + x` on `x := c`, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn mod_expr_binds_int_and_rejects_int64() {
    let r = check(MOD_EXPR_INTO_INT64);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for `var y int64 = x` on `x := 7 % 3`, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn float_expr_binds_float64_and_rejects_int64() {
    let r = check(FLOAT_EXPR_INTO_INT64);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for `var y int64 = x` on `x := 1.5 * 2`, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn rune_expr_binds_int32_and_rejects_int64() {
    let r = check(RUNE_EXPR_INTO_INT64);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for `var y int64 = r` on `r := 'a' + 1`, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn file_untyped_const_expr_binds_int_and_rejects_int64() {
    let r = check(FILE_CONST_EXPR_INTO_INT64);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for `var y int64 = x` on `x := k + 1`, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn binop_const_expr_into_matching_int_accepts() {
    // Over-reach control: the same expression into its own default type
    // must stay accepted.
    let r = check(
        "import \"fmt\"\n\nfn main() {\n    x := 1 + 2\n    var y int = x\n    fmt.println(y)\n}\n",
    );
    assert!(
        error_codes(&r).is_empty(),
        "expected `var y int = x` on `x := 1 + 2` to accept, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn const_expr_with_explicit_conversion_accepts() {
    // `int64(x)` is a typed conversion: the binding takes int64 and the
    // declaration is legal.
    let r = check(
        "import \"fmt\"\n\nfn main() {\n    x := 7 % 3\n    var y int64 = int64(x)\n    fmt.println(y)\n}\n",
    );
    assert!(
        error_codes(&r).is_empty(),
        "expected explicit conversion to accept, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn typed_param_shadowing_file_const_stays_typed() {
    // `c` is an int64 parameter shadowing file const `c`: `x := c` must take
    // the parameter's type, not the const's default — a false E1002 here
    // would reject a program Go builds.
    let r = check(
        "import \"fmt\"\n\nconst c = 5\n\nfn take(c int64) {\n    x := c\n    var y int64 = x\n    fmt.println(y)\n}\n\nfn main() {\n    take(3)\n}\n",
    );
    assert!(
        error_codes(&r).is_empty(),
        "expected param-shadowed `x := c` to keep int64, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn local_untyped_const_into_matching_int_accepts() {
    let r = check(
        "import \"fmt\"\n\nfn main() {\n    const c = 5\n    x := c\n    var y int = x\n    fmt.println(y)\n}\n",
    );
    assert!(
        error_codes(&r).is_empty(),
        "expected `var y int = x` on `x := c` to accept, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

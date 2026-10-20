//! emit_build_guards.rs — regressions for issue #201: five families that
//! passed `hyb check` with 0 diagnostics and then failed `go vet` /
//! `go build` on the emitted Go.
//!
//! Families: A incomparable-type `==` (E2006), B var-ascription const
//! overflow/truncation (E6002) + constant negative shift (E6005),
//! C package/method/field redeclarations (E1003), D invalid receivers
//! (E2007), E invalid array lengths (E6004).

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

fn rejects_with(src: &str, code: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == code),
        "expected {}, got {:?}\nsrc:\n{}",
        code,
        cs,
        src
    );
    assert_eq!(r.verdict, Verdict::Reject, "src:\n{}", src);
}

fn accepts_without(src: &str, code: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        !cs.iter().any(|c| c == code),
        "{} must not fire, got {:?}\nsrc:\n{}",
        code,
        cs,
        src
    );
    assert_eq!(r.verdict, Verdict::Accept, "src:\n{}", src);
}

// ---------------------------------------------------------------------------
// A — incomparable equality (HYB-E2006)
// ---------------------------------------------------------------------------

const EQ_MAP: &str = "\
fn main() {
    m1 := map[string]int{\"a\": 1}
    m2 := map[string]int{\"b\": 2}
    if m1 == m2 {
        fmt.println(\"eq\")
    }
}
";

const EQ_SLICE: &str = "\
fn main() {
    s1 := []int{1, 2}
    s2 := []int{1, 2}
    if s1 == s2 {
        fmt.println(\"eq\")
    }
}
";

const EQ_FN: &str = "\
fn helper() int {
    return 1
}

fn main() {
    if helper == helper {
        fmt.println(\"eq\")
    }
}
";

const EQ_STRUCT_FN_FIELD: &str = "\
struct holder {
    cb fn () int
}

fn main() {
    var h1 holder
    var h2 holder
    if h1 == h2 {
        fmt.println(\"eq\")
    }
}
";

const EQ_INT_OK: &str = "\
fn main() {
    a := 1
    b := 2
    if a == b {
        fmt.println(\"eq\")
    }
}
";

const EQ_STRUCT_SCALARS_OK: &str = "\
struct pair {
    a int
    b string
}

fn main() {
    var p1 pair
    var p2 pair
    if p1 == p2 {
        fmt.println(\"eq\")
    }
}
";

const EQ_NIL_OK: &str = "\
fn main() {
    m := map[string]int{}
    if m == nil {
        fmt.println(\"nil\")
    }
}
";

#[test]
fn map_equality_is_rejected() {
    rejects_with(EQ_MAP, "HYB-E2006");
}

#[test]
fn slice_equality_is_rejected() {
    rejects_with(EQ_SLICE, "HYB-E2006");
}

#[test]
fn fn_equality_is_rejected() {
    rejects_with(EQ_FN, "HYB-E2006");
}

#[test]
fn struct_with_fn_field_equality_is_rejected() {
    rejects_with(EQ_STRUCT_FN_FIELD, "HYB-E2006");
}

#[test]
fn int_equality_stays_accepted() {
    accepts_without(EQ_INT_OK, "HYB-E2006");
}

#[test]
fn comparable_struct_equality_stays_accepted() {
    accepts_without(EQ_STRUCT_SCALARS_OK, "HYB-E2006");
}

#[test]
fn nil_comparison_stays_accepted() {
    accepts_without(EQ_NIL_OK, "HYB-E2006");
}

// ---------------------------------------------------------------------------
// B — const expressions / float truncation (HYB-E6002), negative shift (E6005)
// ---------------------------------------------------------------------------

const ASCR_EXPR_OVERFLOW: &str = "\
fn main() {
    var x int8 = 100 + 100
    fmt.println(\"{x}\")
}
";

const ASCR_SHIFT_OVERFLOW: &str = "\
fn main() {
    var x int8 = 1 << 10
    fmt.println(\"{x}\")
}
";

const ASCR_FLOAT_TO_INT: &str = "\
fn main() {
    var x int = 2.5
    fmt.println(\"{x}\")
}
";

const NEG_SHIFT: &str = "\
fn main() {
    x := 1 << -1
    fmt.println(\"{x}\")
}
";

const NEG_SHIFT_ASSIGN: &str = "\
fn main() {
    x := 4
    x <<= -1
    fmt.println(\"{x}\")
}
";

const ASCR_EXPR_FITS_OK: &str = "\
fn main() {
    var x int8 = 100 + 27
    fmt.println(\"{x}\")
}
";

const ASCR_FLOAT_TYPE_OK: &str = "\
fn main() {
    var x float64 = 2.5
    fmt.println(\"{x}\")
}
";

const SHIFT_FITS_OK: &str = "\
fn main() {
    x := 1 << 3
    fmt.println(\"{x}\")
}
";

#[test]
fn ascription_expr_overflow_is_rejected() {
    rejects_with(ASCR_EXPR_OVERFLOW, "HYB-E6002");
}

#[test]
fn ascription_shift_overflow_is_rejected() {
    rejects_with(ASCR_SHIFT_OVERFLOW, "HYB-E6002");
}

#[test]
fn float_constant_to_int_is_rejected() {
    rejects_with(ASCR_FLOAT_TO_INT, "HYB-E6002");
}

#[test]
fn negative_shift_count_is_rejected() {
    rejects_with(NEG_SHIFT, "HYB-E6005");
}

#[test]
fn negative_shift_assign_is_rejected() {
    rejects_with(NEG_SHIFT_ASSIGN, "HYB-E6005");
}

#[test]
fn ascription_expr_that_fits_stays_accepted() {
    accepts_without(ASCR_EXPR_FITS_OK, "HYB-E6002");
}

#[test]
fn float_constant_to_float_stays_accepted() {
    accepts_without(ASCR_FLOAT_TYPE_OK, "HYB-E6002");
}

#[test]
fn positive_shift_stays_accepted() {
    accepts_without(SHIFT_FITS_OK, "HYB-E6005");
}

// ---------------------------------------------------------------------------
// C — redeclarations (HYB-E1003)
// ---------------------------------------------------------------------------

const DUP_FN_MAIN: &str = "\
fn main() {
    fmt.println(\"a\")
}

fn main() {
    fmt.println(\"b\")
}
";

const DUP_CONST: &str = "\
const a = 1
const a = 2

fn main() {
    fmt.println(\"{a}\")
}
";

const DUP_STRUCT_FIELD: &str = "\
struct dup {
    a int
    a string
}

fn main() {
    fmt.println(\"ok\")
}
";

const DUP_METHOD: &str = "\
struct box {
    v int
}

fn (b box) get() int {
    return b.v
}

fn (b box) get() int {
    return b.v + 1
}

fn main() {
    fmt.println(\"ok\")
}
";

const SAME_METHOD_NAME_DIFFERENT_TYPES_OK: &str = "\
struct one {
    v int
}

struct two {
    v int
}

fn (a one) get() int {
    return a.v
}

fn (b two) get() int {
    return b.v
}

fn main() {
    fmt.println(\"ok\")
}
";

#[test]
fn duplicate_fn_main_is_rejected() {
    rejects_with(DUP_FN_MAIN, "HYB-E1003");
}

#[test]
fn duplicate_const_is_rejected() {
    rejects_with(DUP_CONST, "HYB-E1003");
}

#[test]
fn duplicate_struct_field_is_rejected() {
    rejects_with(DUP_STRUCT_FIELD, "HYB-E1003");
}

#[test]
fn duplicate_method_is_rejected() {
    rejects_with(DUP_METHOD, "HYB-E1003");
}

#[test]
fn same_method_name_on_different_types_stays_accepted() {
    accepts_without(SAME_METHOD_NAME_DIFFERENT_TYPES_OK, "HYB-E1003");
}

// ---------------------------------------------------------------------------
// D — invalid receivers (HYB-E2007)
// ---------------------------------------------------------------------------

const RECEIVER_INT: &str = "\
fn (c int) method() int {
    return c
}

fn main() {
    x := 1
    fmt.println(\"{x}\")
}
";

const RECEIVER_DOUBLE_PTR: &str = "\
struct box {
    v int
}

fn (b **box) get() int {
    return 0
}

fn main() {
    fmt.println(\"ok\")
}
";

const RECEIVER_STRUCT_OK: &str = "\
struct box {
    v int
}

fn (b box) get() int {
    return b.v
}

fn main() {
    fmt.println(\"ok\")
}
";

const RECEIVER_PTR_OK: &str = "\
struct box {
    v int
}

fn (b *box) get() int {
    return b.v
}

fn main() {
    fmt.println(\"ok\")
}
";

#[test]
fn primitive_receiver_is_rejected() {
    rejects_with(RECEIVER_INT, "HYB-E2007");
}

#[test]
fn double_pointer_receiver_is_rejected() {
    rejects_with(RECEIVER_DOUBLE_PTR, "HYB-E2007");
}

#[test]
fn struct_receiver_stays_accepted() {
    accepts_without(RECEIVER_STRUCT_OK, "HYB-E2007");
}

#[test]
fn pointer_receiver_stays_accepted() {
    accepts_without(RECEIVER_PTR_OK, "HYB-E2007");
}

// ---------------------------------------------------------------------------
// E — array lengths (HYB-E6004)
// ---------------------------------------------------------------------------

const ARRAY_NEG_LEN: &str = "\
fn main() {
    var a [-1]int
}
";

const ARRAY_VAR_LEN: &str = "\
fn main() {
    n := 4
    var a [n]int
}
";

const ARRAY_LITERAL_LEN_OK: &str = "\
fn main() {
    var a [4]int
    fmt.println(len(a))
}
";

const ARRAY_CONST_LEN_OK: &str = "\
const size = 8

fn main() {
    var a [size]int
    fmt.println(len(a))
}
";

const ARRAY_PARAM_NEG_LEN: &str = "\
fn f(a [-1]int) {
}

fn main() {
    fmt.println(\"ok\")
}
";

#[test]
fn negative_array_length_is_rejected() {
    rejects_with(ARRAY_NEG_LEN, "HYB-E6004");
}

#[test]
fn variable_array_length_is_rejected() {
    rejects_with(ARRAY_VAR_LEN, "HYB-E6004");
}

#[test]
fn literal_array_length_stays_accepted() {
    accepts_without(ARRAY_LITERAL_LEN_OK, "HYB-E6004");
}

#[test]
fn const_array_length_stays_accepted() {
    accepts_without(ARRAY_CONST_LEN_OK, "HYB-E6004");
}

#[test]
fn negative_array_length_in_param_is_rejected() {
    rejects_with(ARRAY_PARAM_NEG_LEN, "HYB-E6004");
}

// ---------------------------------------------------------------------------
// Method type parameters (HYB-E1002, issue #881)
// ---------------------------------------------------------------------------

const METHOD_WITH_OWN_TYPE_PARAMS: &str = "\
struct plain {
    n int
}

fn (p *plain) retype[k2 comparable](v k2) k2 {
    return v
}

fn main() {
}
";

const METHOD_GENERIC_RECEIVER_WITH_OWN_TYPE_PARAMS: &str = "\
struct cache[k comparable] {
    n k
}

fn (c *cache[k]) retype[k2 comparable](v k2) k2 {
    return v
}

fn main() {
}
";

const METHOD_USING_STRUCT_TYPE_PARAM_OK: &str = "\
struct cache[k comparable] {
    n k
}

fn (c *cache[k]) conv(u k) k {
    return u
}

fn main() {
}
";

#[test]
fn method_with_own_type_parameters_is_rejected_issue_881() {
    rejects_with(METHOD_WITH_OWN_TYPE_PARAMS, "HYB-E1002");
    rejects_with(METHOD_GENERIC_RECEIVER_WITH_OWN_TYPE_PARAMS, "HYB-E1002");
}

#[test]
fn method_using_struct_type_parameters_stays_accepted_issue_881() {
    accepts_without(METHOD_USING_STRUCT_TYPE_PARAM_OK, "HYB-E1002");
}

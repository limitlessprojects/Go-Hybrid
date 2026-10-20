//! float_const_overflow_issue_683.rs — regression test for issue #683:
//! untyped float constants past float64/float32 range pass hyb check at
//! declaration, assignment, return, literal-element, and append sites,
//! then fail go build (issue #228 family D, float leg).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "test_float_overflow.hy";
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
// Site 1: default-float64 local, both spellings and unary minus
// ---------------------------------------------------------------------------

#[test]
fn default_float64_let_overflow_rejected() {
    let src = "
fn main() {
    let y = 1e400
    fmt.println(y)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002, got {:?}",
        error_codes(&r)
    );
}

#[test]
fn default_float64_define_overflow_rejected() {
    let src = "
fn main() {
    y := 1e400
    fmt.println(y)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002, got {:?}",
        error_codes(&r)
    );
}

#[test]
fn default_float64_neg_overflow_rejected() {
    let src = "
fn main() {
    let h = -1e400
    fmt.println(h)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002, got {:?}",
        error_codes(&r)
    );
}

#[test]
fn default_float64_binary_fold_overflow_rejected() {
    let src = "
fn main() {
    let z = 1e300 * 1e300
    fmt.println(z)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002, got {:?}",
        error_codes(&r)
    );
}

// ---------------------------------------------------------------------------
// Site 2: ascribed float32 local
// ---------------------------------------------------------------------------

#[test]
fn ascribed_float32_local_overflow_rejected() {
    let src = "
fn main() {
    var f float32 = 1e40
    fmt.println(f)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002, got {:?}",
        error_codes(&r)
    );
}

#[test]
fn ascribed_float32_neg_overflow_rejected() {
    let src = "
fn main() {
    var f float32 = -1e40
    fmt.println(f)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002, got {:?}",
        error_codes(&r)
    );
}

// ---------------------------------------------------------------------------
// Site 3: reassignment to a float64 / float32 binding
// ---------------------------------------------------------------------------

#[test]
fn reassignment_to_float64_overflow_rejected() {
    let src = "
fn main() {
    var y float64 = 1.0
    y = 1e400
    fmt.println(y)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002, got {:?}",
        error_codes(&r)
    );
}

#[test]
fn reassignment_to_float32_overflow_rejected() {
    let src = "
fn main() {
    var f float32 = 1.0
    f = 1e40
    fmt.println(f)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002, got {:?}",
        error_codes(&r)
    );
}

// ---------------------------------------------------------------------------
// Site 4: closure-body local (default-float64 and assign)
// ---------------------------------------------------------------------------

#[test]
fn closure_body_local_overflow_rejected() {
    let src = "
fn f() {
    let y = 1e400
    fmt.println(y)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002, got {:?}",
        error_codes(&r)
    );
}

#[test]
fn closure_body_assign_overflow_rejected() {
    let src = "
fn f() {
    var y float64 = 1.0
    y = 1e400
    fmt.println(y)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002, got {:?}",
        error_codes(&r)
    );
}

// ---------------------------------------------------------------------------
// Site 5: return statement of a float64 / float32 fn
// ---------------------------------------------------------------------------

#[test]
fn return_statement_float64_overflow_rejected() {
    let src = "
fn f() float64 {
    return 1e400
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002, got {:?}",
        error_codes(&r)
    );
}

#[test]
fn return_statement_float32_overflow_rejected() {
    let src = "
fn f() float32 {
    return 1e40
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002, got {:?}",
        error_codes(&r)
    );
}

// ---------------------------------------------------------------------------
// Site 6: slice-literal element and append argument
// ---------------------------------------------------------------------------

#[test]
fn slice_literal_element_float64_overflow_rejected() {
    let src = "
fn main() {
    let xs = []float64{1e400}
    fmt.println(xs)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002, got {:?}",
        error_codes(&r)
    );
}

#[test]
fn append_argument_float64_overflow_rejected() {
    let src = "
fn main() {
    let xs = []float64{}
    let ys = append(xs, 1e400)
    fmt.println(ys)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002, got {:?}",
        error_codes(&r)
    );
}

// ---------------------------------------------------------------------------
// Site 7: past-i128 INTEGER literal at a float-declared site
// ---------------------------------------------------------------------------

#[test]
fn huge_integer_at_float64_site_overflow_rejected() {
    let zeros = "0".repeat(400);
    let src = format!("
fn main() {{
    var x float64 = 1{}
    fmt.println(x)
}}
", zeros);
    let r = check(&src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002, got {:?}",
        error_codes(&r)
    );
}

// ---------------------------------------------------------------------------
// Untyped const alias resolved into float variable declaration
// ---------------------------------------------------------------------------

#[test]
fn untyped_const_alias_overflow_rejected() {
    let src = "
const big = 1e400

fn main() {
    let y = big
    fmt.println(y)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002, got {:?}",
        error_codes(&r)
    );
}

// ---------------------------------------------------------------------------
// Controls: what Go accepts at the same sites MUST NOT be rejected
// ---------------------------------------------------------------------------

#[test]
fn go_accepted_controls_pass_cleanly() {
    let controls = [
        // f64::MAX, builds
        "fn main() {\n    var x float64 = 1.7976931348623157e308\n    fmt.println(x)\n}\n",
        // within half an ulp — rounds down to MAX, builds (go1.26 vet clean)
        "fn main() {\n    var x float64 = 1.7976931348623158e308\n    fmt.println(x)\n}\n",
        // within half an ulp of f32::MAX — rounds down, builds (go1.26 vet clean)
        "fn main() {\n    var x float32 = 3.4028235e38\n    fmt.println(x)\n}\n",
        // underflow rounds to 0 silently in go1.26 — builds, NOT an error
        "fn main() {\n    var x float64 = 1e-400\n    fmt.println(x)\n}\n",
        // underflow for float32
        "fn main() {\n    var x float32 = 1e-46\n    fmt.println(x)\n}\n",
        // inside range, builds
        "fn main() {\n    x := 1e308\n    fmt.println(x)\n}\n",
        "fn main() {\n    let g = 1e308\n    fmt.println(g)\n}\n",
        // normal float operations
        "fn main() {\n    var f float64 = 2.5\n    fmt.println(f)\n}\n",
        "fn main() {\n    var f float32 = 2.5\n    fmt.println(f)\n}\n",
        "fn main() {\n    xs := []float64{1.0, 2.0}\n    ys := append(xs, 3.0)\n    fmt.println(ys)\n}\n",
        "fn f() float64 {\n    return 1.5\n}\n",
    ];

    for src in controls {
        let r = check(src);
        assert!(
            error_codes(&r).is_empty(),
            "control must pass cleanly without diagnostics: {}\ngot: {:?}",
            src,
            error_codes(&r)
        );
        assert_eq!(r.verdict, Verdict::Accept);
    }
}

// ---------------------------------------------------------------------------
// f32/f64 alias spellings (carried from rickymulya's parallel take #689):
// the emitter maps both to float32/float64, so past-range constants under
// the alias spellings are the same go-build failures.
// ---------------------------------------------------------------------------

#[test]
fn f32_alias_spelling_overflow_rejected() {
    let src = "
fn main() {
    var f f32 = 1e40
    fmt.println(f)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E6002"),
        "var f f32 = 1e40 must reject HYB-E6002, got {:?}",
        error_codes(&r)
    );
}

#[test]
fn f64_alias_spelling_overflow_rejected() {
    let src = "
fn main() {
    var x f64 = 1e400
    fmt.println(x)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E6002"),
        "var x f64 = 1e400 must reject HYB-E6002, got {:?}",
        error_codes(&r)
    );
}

#[test]
fn f64_alias_spelling_assign_overflow_rejected() {
    let src = "
fn main() {
    var y f64 = 1.0
    y = 1e400
    fmt.println(y)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E6002"),
        "y = 1e400 under f64 must reject HYB-E6002, got {:?}",
        error_codes(&r)
    );
}

#[test]
fn f32_alias_in_range_control_accepts() {
    let src = "
fn main() {
    var f f32 = 3.4028235e38
    fmt.println(f)
}
";
    let r = check(src);
    assert!(
        error_codes(&r).is_empty(),
        "in-range f32-alias control must accept, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

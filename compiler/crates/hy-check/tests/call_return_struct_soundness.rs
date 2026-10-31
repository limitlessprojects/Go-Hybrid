use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> (Verdict, Vec<hy_diag::Diagnostic>) {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    let res = check_file(&ast);
    (res.verdict, res.diagnostics)
}

#[test]
fn struct_call_return_non_const_int_for_float_rejected() {
    let cases = [
        // 1. Struct literal field mismatch
        r#"
struct config {
    rate float64
}
fn main() {
    var i int = 1
    cfg := config{rate: i}
    _ = cfg
}
"#,
        // 2. Function call argument mismatch
        r#"
fn compute(x float64) {}
fn main() {
    var i int = 1
    compute(i)
}
"#,
        // 3. Method call argument mismatch
        r#"
struct runner {}
fn (r runner) run(x float64) {}
fn main() {
    var i int = 1
    r := runner{}
    r.run(i)
}
"#,
        // 4. Return statement mismatch
        r#"
fn get_rate() float64 {
    var i int = 1
    return i
}
fn main() {
    _ = get_rate()
}
"#,
    ];

    for src in cases {
        let (verdict, diags) = check(src);
        assert_eq!(
            verdict,
            Verdict::Reject,
            "expected reject for non-const int in float position:\n{src}"
        );
        assert!(
            diags.iter().any(|d| d.code == "HYB-E1002"),
            "expected HYB-E1002 in diags: {:?}",
            diags
        );
    }
}

#[test]
fn struct_call_return_untyped_const_and_matching_type_accepted() {
    let src = r#"
const c = 2

struct config {
    rate float64
}

fn (cfg config) update(x float64) {}

fn compute(x float64) {}

fn get_rate() float64 {
    return 1
}

fn get_rate_c() float64 {
    return c
}

fn get_rate_var() float64 {
    var f float64 = 3.5
    return f
}

fn main() {
    var f float64 = 1.0

    // Struct literal
    c1 := config{rate: 1}
    c2 := config{rate: c}
    c3 := config{rate: f}
    _ = c1
    _ = c2
    _ = c3

    // Function call
    compute(1)
    compute(c)
    compute(f)

    // Method call
    c1.update(1)
    c1.update(c)
    c1.update(f)

    // Return calls
    _ = get_rate()
    _ = get_rate_c()
    _ = get_rate_var()
}
"#;
    let (verdict, diags) = check(src);
    assert_eq!(
        verdict,
        Verdict::Accept,
        "expected accept for valid untyped constants and matching types: {:?}",
        diags
    );
}

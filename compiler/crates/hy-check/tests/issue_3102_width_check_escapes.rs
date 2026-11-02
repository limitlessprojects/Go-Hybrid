//! issue_3102_width_check_escapes.rs — tests for HYB-E1002 rejection
//! of escaped scalar width checks through for-in bindings, method calls,
//! fn-literal calls, and channel receives (fixes #3102).

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn for_in_typed_slice_and_array_bindings_reject_e1002() {
    let cases = [
        // 1. slice element width mismatch
        "fn main() {\n    xs := []int8{1, 2}\n    for x in xs {\n        var y int64 = x\n        _ = y\n    }\n}\n",
        // 2. array element width mismatch
        "fn main() {\n    xs := [3]int16{1, 2, 3}\n    for i, x in xs {\n        _ = i\n        var y int = x\n        _ = y\n    }\n}\n",
        // 3. float slice width mismatch
        "fn main() {\n    xs := []float32{1.5}\n    for x in xs {\n        var y float64 = x\n        _ = y\n    }\n}\n",
        // 6. call argument with for-in binding
        "fn take(y int64) { _ = y }\nfn main() {\n    for x in []int8{1, 2} {\n        take(x)\n    }\n}\n",
        // 7. binary operand with for-in binding
        "fn main() {\n    xs := []int8{1, 2}\n    var t int64 = 0\n    for x in xs {\n        t = t + x\n    }\n    _ = t\n}\n",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for:\n{}", src);
        assert!(error_codes(&r).contains(&"HYB-E1002".to_string()), "expected HYB-E1002 for:\n{}", src);
    }
}

#[test]
fn for_in_typed_map_bindings_reject_e1002() {
    let cases = [
        // 4. map value width mismatch
        "fn main() {\n    m := map[string]int8{\"a\": 1}\n    for k, v in m {\n        _ = k\n        var y int64 = v\n        _ = y\n    }\n}\n",
        // 5. map key width mismatch
        "fn main() {\n    m := map[int8]string{1: \"a\"}\n    for k, v in m {\n        _ = v\n        var y int64 = k\n        _ = y\n    }\n}\n",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for:\n{}", src);
        assert!(error_codes(&r).contains(&"HYB-E1002".to_string()), "expected HYB-E1002 for:\n{}", src);
    }
}

#[test]
fn method_and_fn_literal_calls_reject_e1002() {
    let cases = [
        // 8. method call result scalar mismatch
        "struct p { x int }\nfn (q p) g() int8 { return 1 }\nfn main() {\n    a := p{x: 1}\n    var y int64 = a.g()\n    _ = y\n}\n",
        // 9. fn-literal local call result scalar mismatch
        "fn main() {\n    f := fn() int8 { return 1 }\n    var y int64 = f()\n    _ = y\n}\n",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for:\n{}", src);
        assert!(error_codes(&r).contains(&"HYB-E1002".to_string()), "expected HYB-E1002 for:\n{}", src);
    }
}

#[test]
fn channel_receive_scalars_reject_e1002() {
    let cases = [
        // 10. channel receive in assignment
        "fn main() {\n    c := make(chan int8, 1)\n    var y int64 = <-c\n    _ = y\n}\n",
        // 11. channel receive into short-decl then assignment
        "fn main() {\n    c := make(chan uint16, 1)\n    x := <-c\n    var y int = x\n    _ = y\n}\n",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for:\n{}", src);
        assert!(error_codes(&r).contains(&"HYB-E1002".to_string()), "expected HYB-E1002 for:\n{}", src);
    }
}

#[test]
fn matching_widths_accepted() {
    let src = r#"
struct p { x int }
fn (q p) g() int8 { return 1 }
fn main() {
    xs := []int8{1, 2}
    var t int8 = 0
    for x in xs { t = t + x }
    m := map[int8]float32{1: 1.5}
    for k, v in m {
        var a int8 = k
        var b float32 = v
        _ = a
        _ = b
    }
    a := p{x: 1}
    var g int8 = a.g()
    _ = g
    f := fn() int8 { return 1 }
    var h int8 = f()
    _ = h
    c := make(chan int8, 1)
    c <- 1
    var r int8 = <-c
    _ = r
    var big int64 = int64(t) + 1
    _ = big
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected accept, got diagnostics: {:?}", r.diagnostics);
}

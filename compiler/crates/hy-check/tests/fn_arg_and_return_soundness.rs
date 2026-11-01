use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

#[test]
fn test_mismatched_fn_arg_and_return_types_rejected() {
    let src = r#"
struct accumulator {
    total int64
}

fn (a *accumulator) add(delta int64) {}

fn callee_int(x int64) {}
fn callee_float(f float64) {}

fn ret_int64() int64 {
    var x int32 = 10
    return x
}

fn ret_float64() float64 {
    var f float32 = 1.0
    return f
}

fn ret_int64_err() (int64, error) {
    var x int32 = 10
    return x
}

fn caller() {
    var a32 int32 = 10
    var f32 float32 = 1.0
    callee_int(a32)
    callee_float(f32)

    var acc accumulator
    acc.add(a32)
}
"#;
    let ast = parse_file("t.hy", src).expect("snippet parses");
    let res = check_file(&ast);
    println!("Verdict: {:?}", res.verdict);
    for d in &res.diagnostics {
        println!("Diag: {} - {}", d.code, d.message);
    }
    assert_eq!(res.verdict, Verdict::Reject);
    assert_eq!(res.diagnostics.len(), 6);
    for d in &res.diagnostics {
        assert_eq!(d.code, "HYB-E1002");
    }
}

#[test]
fn test_valid_fn_arg_and_return_types_accepted() {
    let src = r#"
struct accumulator {
    total int64
}

fn (a *accumulator) add(delta int64) {}

fn callee_int(x int64) {}
fn callee_float(f float64) {}

fn ret_int64() int64 {
    var x int64 = 10
    return x
}

fn ret_float64() float64 {
    var f float64 = 1.0
    return f
}

fn ret_const_int() int64 {
    return 100
}

fn ret_const_float() float64 {
    return 3.14
}

fn caller() {
    var a64 int64 = 10
    var f64 float64 = 1.0
    callee_int(a64)
    callee_float(f64)

    // Untyped constants adapt legally
    callee_int(42)
    callee_float(2.718)

    var acc accumulator
    acc.add(a64)
    acc.add(10)
}

fn byte_rune_aliases(b byte, r rune) (uint8, int32) {
    return b, r
}
"#;
    let ast = parse_file("t.hy", src).expect("snippet parses");
    let res = check_file(&ast);
    for d in &res.diagnostics {
        println!("Unexpected Diag: {} - {}", d.code, d.message);
    }
    assert_eq!(res.verdict, Verdict::Accept);
    assert!(res.diagnostics.is_empty());
}

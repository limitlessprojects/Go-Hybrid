use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

#[test]
fn test_invalid_binary_arith_and_bitwise_rejected() {
    let src = r#"
fn main() {
    var a64 int64 = 10
    var b32 int32 = 3

    _ = a64 + b32
    _ = a64 - b32
    _ = a64 * b32
    _ = a64 / b32
    _ = a64 % b32

    _ = a64 & b32
    _ = a64 | b32
    _ = a64 ^ b32

    a64 += b32
    a64 -= b32
    a64 *= b32
    a64 /= b32
    a64 %= b32
    a64 &= b32
    a64 |= b32
    a64 ^= b32

    var f64 float64 = 1.0
    var f32 float32 = 2.0
    _ = f64 + f32
    _ = f64 - f32
    _ = f64 * f32
    _ = f64 / f32

    f64 += f32
    f64 -= f32
    f64 *= f32
    f64 /= f32
}
"#;
    let ast = parse_file("t.hy", src).expect("snippet parses");
    let res = check_file(&ast);
    println!("Verdict: {:?}", res.verdict);
    for d in &res.diagnostics {
        println!("Diag: {} - {}", d.code, d.message);
    }
    assert_eq!(res.verdict, Verdict::Reject);
    assert_eq!(res.diagnostics.len(), 24);
    for d in &res.diagnostics {
        assert_eq!(d.code, "HYB-E1002");
    }
}

#[test]
fn test_valid_binary_arith_and_bitwise_accepted() {
    let src = r#"
type my_int = int64

fn main() {
    var a int64 = 10
    var b int64 = 20
    _ = a + b
    _ = a - b
    _ = a * b
    _ = a / b
    _ = a % b
    _ = a & b
    _ = a | b
    _ = a ^ b
    _ = a << 2
    _ = a >> 1

    a += 5
    a -= 2
    a *= 3
    a /= 2
    a %= 3
    a &= 7
    a |= 1
    a ^= 2
    a <<= 1
    a >>= 1

    var f float64 = 3.14
    _ = f + 2.0
    _ = f * 1.5
    f += 1.0

    var u uint8 = 65
    var by byte = 1
    _ = u + by
    u += by

    var i int32 = 100
    var ru rune = 2
    _ = i + ru
    i += ru
}
"#;
    let ast = parse_file("t.hy", src).expect("snippet parses");
    let res = check_file(&ast);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "valid binary and assign ops must be accepted: {:?}",
        res.diagnostics
    );
    assert!(res.diagnostics.is_empty());
}

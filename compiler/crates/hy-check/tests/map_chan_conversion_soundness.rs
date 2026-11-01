use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

#[test]
fn test_invalid_mismatched_map_and_chan_conversions_rejected() {
    let src = r#"
type map_a = map[string]int32
type map_b = map[string]int64
type map_c = map[int32]string
type map_d = map[int64]string
type map_s = map[string]string

type chan_a = chan int32
type chan_b = chan int64
type chan_s = chan string

fn main() {
    // Map value scalar mismatch
    var ma map_a
    var mb map_b = map_b(ma)

    // Map key scalar mismatch
    var mc map_c
    var md map_d = map_d(mc)

    // Map value kind mismatch
    var ms map_s
    var mb2 map_b = map_b(ms)

    // Channel element scalar mismatch
    var ca chan_a
    var cb chan_b = chan_b(ca)

    // Channel element kind mismatch
    var cs chan_s
    var cb2 chan_b = chan_b(cs)

    _ = mb
    _ = md
    _ = mb2
    _ = cb
    _ = cb2
}
"#;
    let ast = parse_file("invalid_map_chan_conv.hy", src).expect("snippet parses");
    let res = check_file(&ast);
    println!("Verdict: {:?}", res.verdict);
    for d in &res.diagnostics {
        println!("Diag: {} - {}", d.code, d.message);
    }
    assert_eq!(res.verdict, Verdict::Reject);
    assert_eq!(res.diagnostics.len(), 5);
    for d in &res.diagnostics {
        assert_eq!(d.code, "HYB-E1002");
    }
}

#[test]
fn test_valid_same_map_and_chan_conversions_accepted() {
    let src = r#"
type map_alias = map[string]int64
type chan_alias = chan int64

fn main() {
    var m map[string]int64
    var ma map_alias = map_alias(m)

    var c chan int64
    var ca chan_alias = chan_alias(c)

    _ = ma
    _ = ca
}
"#;
    let ast = parse_file("valid_map_chan_conv.hy", src).expect("snippet parses");
    let res = check_file(&ast);
    for d in &res.diagnostics {
        println!("Unexpected Diag: {} - {}", d.code, d.message);
    }
    assert_eq!(res.verdict, Verdict::Accept);
    assert!(res.diagnostics.is_empty());
}

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

#[test]
fn test_invalid_chan_send_numeric_mismatch_rejected() {
    let src = r#"
type my_chan = chan int64

fn main() {
    var ch chan int = make(chan int, 1)
    var f float64 = 3.14
    ch <- f

    var ch64 chan int64 = make(chan int64, 1)
    var i32 int32 = 1
    ch64 <- i32

    var chf32 chan float32 = make(chan float32, 1)
    var f64 float64 = 1.0
    chf32 <- f64

    var chf64 chan float64 = make(chan float64, 1)
    var i int = 10
    chf64 <- i

    var alias_ch my_chan = make(my_chan, 1)
    var i16 int16 = 2
    alias_ch <- i16

    ch <- 1.5

    select {
    ch64 <- i32: {}
    default: {}
    }
}
"#;
    let ast = parse_file("t.hy", src).expect("snippet parses");
    let res = check_file(&ast);
    println!("Verdict: {:?}", res.verdict);
    for d in &res.diagnostics {
        println!("Diag: {} - {}", d.code, d.message);
    }
    assert_eq!(res.verdict, Verdict::Reject);
    assert_eq!(res.diagnostics.len(), 7);
    let e1002_count = res.diagnostics.iter().filter(|d| d.code == "HYB-E1002").count();
    assert_eq!(e1002_count, 6);
}

#[test]
fn test_valid_chan_send_accepted() {
    let src = r#"
type byte_chan = chan byte
type rune_chan = chan rune

fn main() {
    var ch64 chan int64 = make(chan int64, 1)
    var v64 int64 = 100
    ch64 <- v64
    ch64 <- 1

    var chf64 chan float64 = make(chan float64, 1)
    var vf64 float64 = 2.5
    chf64 <- vf64
    chf64 <- 3.0

    var bch byte_chan = make(byte_chan, 1)
    var b uint8 = 65
    bch <- b

    var rch rune_chan = make(rune_chan, 1)
    var r int32 = 65
    rch <- r

    select {
    ch64 <- v64: {}
    default: {}
    }
}
"#;
    let ast = parse_file("t.hy", src).expect("snippet parses");
    let res = check_file(&ast);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "valid channel sends must be accepted: {:?}",
        res.diagnostics
    );
    assert!(res.diagnostics.is_empty());
}

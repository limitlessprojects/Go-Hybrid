use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

#[test]
fn test_repros_4537() {
    let prog = r#"
fn worker(rx hy.recv_cap[map[string]int]) {
    m := rx.recv()
    m["w"] = 1
    rx.drain()
}

fn main() {
    tx, rx := hy.pipe[map[string]int]()
    m := map[string]int{}
    task worker(move rx)
    tx.send(m)
    m["x"] = 2
    tx.close()
}
"#;
    let r = check_file(&parse_file("prog.hy", prog).unwrap());
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(r.diagnostics.iter().any(|d| d.code == "HYB-E3002"));

    let wrongtype = r#"
fn worker(rx hy.recv_cap[int]) { rx.drain() }
fn main() {
    tx, rx := hy.pipe[int]()
    task worker(move rx)
    tx.send(map[string]int{})
    tx.close()
}
"#;
    let r = check_file(&parse_file("wrongtype.hy", wrongtype).unwrap());
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(r.diagnostics.iter().any(|d| d.code == "HYB-E3011"));

    let surface = r#"
fn main() {
    tx, rx := hy.pipe[int]()
    tx.send()
    tx.send(1, 2)
    tx.bogus(1)
    _ = rx.recv(9)
    tx.close()
    rx.drain()
}
"#;
    let r = check_file(&parse_file("surface.hy", surface).unwrap());
    assert_eq!(r.verdict, Verdict::Reject);
    assert_eq!(r.diagnostics.iter().filter(|d| d.code == "HYB-E1002").count(), 4);

    // Positive controls:
    let ok_int = r#"
fn worker(rx hy.recv_cap[int]) {
    _ = rx.recv()
    rx.drain()
}

fn main() {
    tx, rx := hy.pipe[int]()
    task worker(move rx)
    tx.send(42)
    tx.close()
}
"#;
    let r = check_file(&parse_file("ok_int.hy", ok_int).unwrap());
    assert_eq!(r.verdict, Verdict::Accept, "diags: {:?}", r.diagnostics);

    let ok_str = r#"
fn worker(rx hy.recv_cap[string]) {
    _ = rx.recv()
    rx.drain()
}

fn main() {
    tx, rx := hy.pipe[string]()
    task worker(move rx)
    tx.send("hello")
    tx.close()
}
"#;
    let r = check_file(&parse_file("ok_str.hy", ok_str).unwrap());
    assert_eq!(r.verdict, Verdict::Accept, "diags: {:?}", r.diagnostics);
}

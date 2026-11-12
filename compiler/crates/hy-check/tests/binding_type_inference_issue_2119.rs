use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("test.hy", src).expect("parses");
    check_file(&ast)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn test_constant_overflow_on_receive_binding() {
    let src = r#"
fn run(ch chan int8) {
    v := <-ch
    v = 300
    _ = v
}
"#;
    let cs = codes(src);
    assert!(cs.contains(&"HYB-E6002".to_string()), "got {cs:?}");
}

#[test]
fn test_constant_overflow_on_comma_ok_receive_binding() {
    let src = r#"
fn run(ch chan int8) {
    v, ok := <-ch
    v = 300
    _ = v
    _ = ok
}
"#;
    let cs = codes(src);
    assert!(cs.contains(&"HYB-E6002".to_string()), "got {cs:?}");
}

#[test]
fn test_constant_overflow_on_aliased_channel_receive() {
    let src = r#"
type byte_chan = chan uint8

fn run() {
    ch := make(byte_chan, 1)
    v := <-ch
    v = 300
    _ = v
}
"#;
    let cs = codes(src);
    assert!(cs.contains(&"HYB-E6002".to_string()), "got {cs:?}");
}

#[test]
fn test_constant_overflow_on_aliased_map_comma_ok_lookup() {
    let src = r#"
type byte_map = map[string]uint8

fn run(m byte_map) {
    v, ok := m["k"]
    v = 300
    _ = v
    _ = ok
}
"#;
    let cs = codes(src);
    assert!(cs.contains(&"HYB-E6002".to_string()), "got {cs:?}");
}

#[test]
fn test_range_over_send_only_channel_from_receive_rejected() {
    let src = r#"
fn run_chan(outer chan chan<- int) {
    inner := <-outer
    for x in inner {
        _ = x
    }
}
"#;
    let cs = codes(src);
    assert!(cs.contains(&"HYB-E2009".to_string()), "got {cs:?}");
}

#[test]
fn test_multi_value_function_arity_on_received_fn_value() {
    let src = r#"
fn run(ch chan fn(a int, b int) int int) {
    cb := <-ch
    p := cb(3, 9)
    _ = p
}
"#;
    let cs = codes(src);
    assert!(cs.contains(&"HYB-E2002".to_string()), "got {cs:?}");
}

#[test]
fn test_constant_overflow_on_subslice_index() {
    let src = r#"
fn run(xs []int8) {
    sub := xs[0:2]
    elem := sub[0]
    elem = 300
    _ = elem
}
"#;
    let cs = codes(src);
    assert!(cs.contains(&"HYB-E6002".to_string()), "got {cs:?}");
}

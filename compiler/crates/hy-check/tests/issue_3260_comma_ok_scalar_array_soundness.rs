use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn error_codes(src: &str) -> Vec<String> {
    let ast = parse_file("test.hy", src).expect("snippet parses");
    let res = check_file(&ast);
    res.diagnostics
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code)
        .collect()
}

fn check_verdict(src: &str) -> Verdict {
    let ast = parse_file("test.hy", src).expect("snippet parses");
    check_file(&ast).verdict
}

#[test]
fn map_index_scalar_width_mismatch_in_multi_assign_rejected() {
    let src = r#"
fn check_map_scalar(m map[string]int64) {
    var v int32
    var ok bool
    v, ok = m["k"]
    _ = v
    _ = ok
}
"#;
    let codes = error_codes(src);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for assigning int64 map element to int32 target, got: {:?}",
        codes
    );
}

#[test]
fn chan_recv_scalar_width_mismatch_in_multi_assign_rejected() {
    let src = r#"
fn check_chan_scalar(ch chan int64) {
    var v int32
    var ok bool
    v, ok = <-ch
    _ = v
    _ = ok
}
"#;
    let codes = error_codes(src);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for assigning int64 chan element to int32 target, got: {:?}",
        codes
    );
}

#[test]
fn map_index_array_len_mismatch_in_multi_assign_rejected() {
    let src = r#"
fn check_map_array(m map[string][3]int) {
    var v [2]int
    var ok bool
    v, ok = m["k"]
    _ = v
    _ = ok
}
"#;
    let codes = error_codes(src);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for assigning [3]int map element to [2]int target, got: {:?}",
        codes
    );
}

#[test]
fn chan_recv_array_len_mismatch_in_multi_assign_rejected() {
    let src = r#"
fn check_chan_array(ch chan [3]int) {
    var v [2]int
    var ok bool
    v, ok = <-ch
    _ = v
    _ = ok
}
"#;
    let codes = error_codes(src);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for assigning [3]int chan element to [2]int target, got: {:?}",
        codes
    );
}

#[test]
fn map_index_short_decl_scalar_mismatch_subsequent_rejected() {
    let src = r#"
fn check_map_decl_scalar(m map[string]int64) {
    v, ok := m["k"]
    var target int32 = v
    _ = target
    _ = ok
}
"#;
    let codes = error_codes(src);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 when v initialized from map[string]int64 is assigned to int32, got: {:?}",
        codes
    );
}

#[test]
fn chan_recv_short_decl_scalar_mismatch_subsequent_rejected() {
    let src = r#"
fn check_chan_decl_scalar(ch chan int64) {
    v, ok := <-ch
    var target int32 = v
    _ = target
    _ = ok
}
"#;
    let codes = error_codes(src);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 when v initialized from chan int64 is assigned to int32, got: {:?}",
        codes
    );
}

#[test]
fn map_index_short_decl_array_mismatch_subsequent_rejected() {
    let src = r#"
fn check_map_decl_array(m map[string][3]int) {
    v, ok := m["k"]
    var target [2]int
    target = v
    _ = target
    _ = ok
}
"#;
    let codes = error_codes(src);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 when v initialized from map[string][3]int is assigned to [2]int, got: {:?}",
        codes
    );
}

#[test]
fn chan_recv_short_decl_array_mismatch_subsequent_rejected() {
    let src = r#"
fn check_chan_decl_array(ch chan [3]int) {
    v, ok := <-ch
    var target [2]int
    target = v
    _ = target
    _ = ok
}
"#;
    let codes = error_codes(src);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 when v initialized from chan [3]int is assigned to [2]int, got: {:?}",
        codes
    );
}

#[test]
fn comma_ok_matching_types_accepted() {
    let src = r#"
fn check_matching_map(m map[string]int64) {
    var v int64
    var ok bool
    v, ok = m["k"]
    _ = v
    _ = ok

    v2, ok2 := m["k"]
    var target int64 = v2
    _ = target
    _ = ok2
}

fn check_matching_chan(ch chan [3]int) {
    var v [3]int
    var ok bool
    v, ok = <-ch
    _ = v
    _ = ok

    v2, ok2 := <-ch
    var target [3]int
    target = v2
    _ = target
    _ = ok2
}
"#;
    assert_eq!(check_verdict(src), Verdict::Accept);
    assert!(error_codes(src).is_empty());
}

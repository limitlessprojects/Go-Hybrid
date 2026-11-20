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
fn direct_map_slot_read_during_live_loan_rejected_e3102() {
    let src = r#"
struct cell { n int }
fn main() {
    m := map[string]cell{"a": cell{n: 1}}
    p := &m["a"]
    p.n = 7
    _ = m["a"].n
    p.n = 8
}
"#;
    let codes = error_codes(src);
    assert!(
        codes.iter().any(|c| c == "HYB-E3102"),
        "expected HYB-E3102 for reading loaned map slot during live loan window, got: {:?}",
        codes
    );
}

#[test]
fn map_container_passed_to_call_during_live_loan_rejected_e3102() {
    let src = r#"
struct cell { n int }
fn inspect(m map[string]cell) {
    _ = m
}
fn main() {
    m := map[string]cell{"a": cell{n: 1}}
    p := &m["a"]
    p.n = 7
    inspect(m)
    p.n = 8
}
"#;
    let codes = error_codes(src);
    assert!(
        codes.iter().any(|c| c == "HYB-E3102"),
        "expected HYB-E3102 for passing map container to call during live loan window, got: {:?}",
        codes
    );
}

#[test]
fn direct_slice_slot_read_during_live_loan_rejected_e3102() {
    let src = r#"
struct cell { n int }
fn main() {
    xs := []cell{cell{n: 1}}
    p := &xs[0]
    p.n = 7
    _ = xs[0].n
    p.n = 8
}
"#;
    let codes = error_codes(src);
    assert!(
        codes.iter().any(|c| c == "HYB-E3102"),
        "expected HYB-E3102 for reading loaned slice element during live loan window, got: {:?}",
        codes
    );
}

#[test]
fn slice_container_passed_to_call_during_live_loan_rejected_e3102() {
    let src = r#"
struct cell { n int }
fn show(xs []cell) {
    _ = xs
}
fn main() {
    xs := []cell{cell{n: 1}}
    p := &xs[0]
    p.n = 7
    show(xs)
    p.n = 8
}
"#;
    let codes = error_codes(src);
    assert!(
        codes.iter().any(|c| c == "HYB-E3102"),
        "expected HYB-E3102 for passing slice container to call during live loan window, got: {:?}",
        codes
    );
}

#[test]
fn fmt_println_map_slot_during_live_loan_rejected_e3102() {
    let src = r#"
import "fmt"
struct cell { n int }
fn main() {
    m := map[string]cell{"a": cell{n: 1}}
    p := &m["a"]
    p.n = 7
    fmt.println(m["a"].n)
    p.n = 8
}
"#;
    let codes = error_codes(src);
    assert!(
        codes.iter().any(|c| c == "HYB-E3102"),
        "expected HYB-E3102 for fmt.println reading loaned map slot during live loan window, got: {:?}",
        codes
    );
}

#[test]
fn fmt_println_container_during_live_loan_rejected_e3102() {
    let src = r#"
import "fmt"
struct cell { n int }
fn main() {
    m := map[string]cell{"a": cell{n: 1}}
    p := &m["a"]
    p.n = 7
    fmt.println(m)
    p.n = 8
}
"#;
    let codes = error_codes(src);
    assert!(
        codes.iter().any(|c| c == "HYB-E3102"),
        "expected HYB-E3102 for fmt.println receiving container during live loan window, got: {:?}",
        codes
    );
}

#[test]
fn read_distinct_map_slot_during_live_loan_accepted() {
    let src = r#"
struct cell { n int }
fn main() {
    m := map[string]cell{"a": cell{n: 1}, "b": cell{n: 2}}
    p := &m["a"]
    p.n = 7
    _ = m["b"].n
    p.n = 8
}
"#;
    assert_eq!(check_verdict(src), Verdict::Accept);
    assert!(error_codes(src).is_empty());
}

#[test]
fn read_distinct_slice_slot_during_live_loan_accepted() {
    let src = r#"
struct cell { n int }
fn main() {
    xs := []cell{cell{n: 1}, cell{n: 2}}
    p := &xs[0]
    p.n = 7
    _ = xs[1].n
    p.n = 8
}
"#;
    assert_eq!(check_verdict(src), Verdict::Accept);
    assert!(error_codes(src).is_empty());
}

#[test]
fn read_after_last_use_accepted() {
    let src = r#"
struct cell { n int }
fn show(xs []cell) {
    _ = xs
}
fn main() {
    m := map[string]cell{"a": cell{n: 1}}
    p := &m["a"]
    p.n = 7
    p.n = 8
    _ = m["a"].n

    xs := []cell{cell{n: 1}}
    q := &xs[0]
    q.n = 7
    q.n = 8
    show(xs)
}
"#;
    assert_eq!(check_verdict(src), Verdict::Accept);
    assert!(error_codes(src).is_empty());
}

#[test]
fn dynamic_key_read_during_live_loan_rejected_e3102() {
    let src = r#"
struct cell { n int }
fn main() {
    m := map[string]cell{"a": cell{n: 1}}
    p := &m["a"]
    p.n = 7
    k := "a"
    _ = m[k].n
    p.n = 8
}
"#;
    let codes = error_codes(src);
    assert!(
        codes.iter().any(|c| c == "HYB-E3102"),
        "expected HYB-E3102 for dynamic key read on loaned map during live loan window, got: {:?}",
        codes
    );
}

//! Regression tests for issue #4567:
//! A payload-carrying enum was classified as Primitive, so a shared map inside it
//! reached a spawned task or channel unproven.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

const BOX_DEF: &str = "\
enum box {
    full(map[string]int)
    empty
}

fn get(m map[string]int) box {
    return box.full(m)
}

fn worker(b box, done chan<- bool) {
    match b {
        full(inner): {
            for i in 0..2000 {
                inner[\"k\"] = i
            }
        }
        empty: {}
    }
    done <- true
}
";

#[test]
fn issue_4567_enum_returned_map_spawn_is_rejected_e3001() {
    let src = format!("{BOX_DEF}\n\
fn main() {{\n\
    m := map[string]int{{\"k\": 0}}\n\
    done := make(chan bool, 1)\n\
    task worker(get(m), done)\n\
    for i in 0..2000 {{\n\
        m[\"k\"] = i\n\
    }}\n\
    <-done\n\
}}\n");
    let r = check(&src);
    assert_eq!(r.verdict, Verdict::Reject, "diagnostics: {:?}", r.diagnostics);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E3001"),
        "expected HYB-E3001 in {:?}",
        r.diagnostics
    );
}

#[test]
fn enum_with_map_payload_send_is_rejected_e3002() {
    let src = format!("{BOX_DEF}\n\
fn main() {{\n\
    m := map[string]int{{\"k\": 0}}\n\
    ch := make(chan box, 1)\n\
    ch <- box.full(m)\n\
    m[\"k\"] = 1\n\
}}\n");
    let r = check(&src);
    assert_eq!(r.verdict, Verdict::Reject, "diagnostics: {:?}", r.diagnostics);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E3002"),
        "expected HYB-E3002 in {:?}",
        r.diagnostics
    );
}

#[test]
fn enum_with_only_primitive_payloads_spawn_is_accepted() {
    let src = "\
enum shape {
    circle(int)
    square(int)
    dot
}

fn worker(s shape, done chan<- bool) {
    done <- true
}

fn main() {
    done := make(chan bool, 1)
    task worker(shape.circle(3), done)
    <-done
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "diagnostics: {:?}", r.diagnostics);
    assert!(error_codes(&r).is_empty(), "diagnostics: {:?}", r.diagnostics);
}

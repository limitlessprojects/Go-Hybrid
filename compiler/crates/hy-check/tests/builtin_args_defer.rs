//! Regression for issue #187: builtin argument validation (HYB-E2005) and
//! defer-non-call (HYB-E2001). Accepted programs used to emit Go that fails
//! `go build` / `go vet`.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_has(r: &hy_check::CheckResult, code: &str) {
    let cs = error_codes(r);
    assert!(
        cs.iter().any(|c| c == code),
        "expected {}, got {:?}",
        code,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

fn assert_lacks(r: &hy_check::CheckResult, code: &str) {
    let cs = error_codes(r);
    assert!(
        !cs.iter().any(|c| c == code),
        "did not expect {}, got {:?}",
        code,
        cs
    );
}

#[test]
fn make_negative_size_rejected() {
    let r = check("fn main() {\n    c := make(chan int, -1)\n}\n");
    assert_has(&r, "HYB-E2005");
}

#[test]
fn make_string_size_rejected() {
    let r = check("fn main() {\n    c := make(chan int, \"x\")\n}\n");
    assert_has(&r, "HYB-E2005");
}

#[test]
fn make_float_size_rejected() {
    let r = check("fn main() {\n    c := make(chan int, 2.5)\n}\n");
    assert_has(&r, "HYB-E2005");
}

#[test]
fn make_slice_negative_size_rejected() {
    let r = check("fn main() {\n    s := make([]int, -1)\n}\n");
    assert_has(&r, "HYB-E2005");
}

#[test]
fn make_variable_size_ok() {
    let r = check("fn main() {\n    n := 1\n    c := make(chan int, n)\n    close(c)\n}\n");
    assert_lacks(&r, "HYB-E2005");
    assert_lacks(&r, "HYB-E2001");
}

#[test]
fn close_literal_rejected() {
    let r = check("fn main() {\n    close(5)\n}\n");
    assert_has(&r, "HYB-E2005");
}

#[test]
fn len_int_literal_rejected() {
    let r = check("fn main() {\n    fmt.println(len(5))\n}\n");
    assert_has(&r, "HYB-E2005");
}

#[test]
fn cap_string_literal_rejected() {
    let r = check("fn main() {\n    fmt.println(cap(\"x\"))\n}\n");
    assert_has(&r, "HYB-E2005");
}

#[test]
fn len_string_literal_ok() {
    let r = check("fn main() {\n    fmt.println(len(\"abc\"))\n}\n");
    assert_lacks(&r, "HYB-E2005");
}

#[test]
fn delete_arity_one_rejected() {
    let r = check("fn main() {\n    m := map[string]int{\"a\": 1}\n    delete(m)\n}\n");
    assert_has(&r, "HYB-E2005");
}

#[test]
fn delete_arity_two_ok() {
    let r = check("fn main() {\n    m := map[string]int{\"a\": 1}\n    delete(m, \"a\")\n}\n");
    assert_lacks(&r, "HYB-E2005");
}

#[test]
fn defer_ident_send_fusion_is_non_call() {
    // Parser yields Defer{Ident(ch)} then a following receive — not a Call.
    let r = check(
        "fn main() {\n    ch := make(chan int, 1)\n    defer ch <- 1\n    fmt.println(\"ok\")\n}\n",
    );
    assert_has(&r, "HYB-E2001");
}

#[test]
fn defer_receive_is_non_call() {
    let r = check(
        "fn main() {\n    ch := make(chan int, 1)\n    defer <-ch\n    fmt.println(\"ok\")\n}\n",
    );
    assert_has(&r, "HYB-E2001");
}

#[test]
fn defer_close_call_ok() {
    let r = check(
        "fn main() {\n    ch := make(chan int, 1)\n    defer close(ch)\n    fmt.println(\"ok\")\n}\n",
    );
    assert_lacks(&r, "HYB-E2001");
    assert_lacks(&r, "HYB-E2005");
}

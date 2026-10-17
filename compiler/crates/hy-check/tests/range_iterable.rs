//! range_iterable.rs — regression tests for issue #411: the operand of
//! `for x in e` was never classified, so ranging over an `int`, `bool`,
//! a struct or a non-iterable call result passed `hyb check` with 0
//! diagnostics and emitted Go that fails `cannot range over`. A concrete
//! non-iterable operand is now HYB-E2009 (LX13); real iterables and the
//! integer-range form stay clean.

use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn rejects(src: &str) {
    let c = codes(src);
    assert_eq!(
        c.iter().filter(|c| *c == "HYB-E2009").count(),
        1,
        "expected exactly one HYB-E2009, got {:?}",
        c
    );
}

fn clean(src: &str) {
    let c = codes(src);
    assert!(!c.iter().any(|c| c == "HYB-E2009"), "unexpected HYB-E2009: {:?}", c);
}

#[test]
fn r1_int_binding_rejected() {
    rejects("fn main() {\n    x := 5\n    for v in x {\n        fmt.println(v)\n    }\n}\n");
}

#[test]
fn r2_int_call_result_rejected() {
    rejects("fn f() int {\n    return 3\n}\n\nfn main() {\n    for v in f() {\n        fmt.println(v)\n    }\n}\n");
}

#[test]
fn r3_bool_binding_rejected() {
    rejects("fn main() {\n    b := true\n    for v in b {\n        fmt.println(v)\n    }\n}\n");
}

#[test]
fn r4_struct_binding_rejected() {
    rejects("struct box {\n    n int\n}\n\nfn main() {\n    s := box{n: 1}\n    for v in s {\n        fmt.println(v)\n    }\n}\n");
}

#[test]
fn closure_body_rejected_once() {
    rejects("fn main() {\n    x := 5\n    f := fn() {\n        for v in x {\n            fmt.println(v)\n        }\n    }\n    f()\n}\n");
}

#[test]
fn iterables_stay_clean() {
    clean("fn main() {\n    m := map[string]int{}\n    m[\"a\"] = 1\n    for k in m {\n        fmt.println(k)\n    }\n    xs := []int{1, 2}\n    for i, v in xs {\n        fmt.println(i, v)\n    }\n    for c in \"hi\" {\n        fmt.println(c)\n    }\n    for i in 0..3 {\n        fmt.println(i)\n    }\n    ch := make(chan int, 1)\n    ch <- 1\n    close(ch)\n    for v in ch {\n        fmt.println(v)\n    }\n}\n");
}

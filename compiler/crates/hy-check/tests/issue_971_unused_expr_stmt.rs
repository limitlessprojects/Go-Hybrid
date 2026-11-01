//! Regression tests for Issue #971:
//! - Family 1: bare non-call, non-receive expression statements (`Stmt::Expr`) reject as `HYB-E1002`.
//! - Family 2: bare calls to pure/allocating builtins (`len`, `cap`, `make`, `new`, `append`,
//!   `min`, `max`, `complex`, `real`, `imag`) and type conversions (`int(x)`, `[]byte(s)`,
//!   `[1]int(s)`, named types) in `Stmt::Expr` reject as `HYB-E1002`.
//! - Family 3: `defer` of value-only builtins and type conversions rejects as `HYB-E2001`.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("test.hy", src).expect("must parse");
    check_file(&ast)
}

fn error_codes(src: &str) -> Vec<String> {
    check(src)
        .diagnostics
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code)
        .collect()
}

#[test]
fn bare_non_call_expression_statements_reject_e1002() {
    let cases = [
        "fn main() {\n    42\n}\n",
        "fn main() {\n    \"unused\"\n}\n",
        "fn main() {\n    x := 1\n    _ = x\n    x\n}\n",
        "fn main() {\n    1 + 2\n}\n",
        "fn main() {\n    x := 5\n    -x\n}\n",
        "fn main() {\n    b := true\n    !b\n}\n",
        "fn main() {\n    s := []int{1, 2}\n    s[0]\n}\n",
        "fn main() {\n    s := []int{1, 2}\n    s[0:1]\n}\n",
        "struct pt { x int }\nfn main() {\n    p := pt{x: 1}\n    p.x\n}\n",
        "struct pt { x int }\nfn main() {\n    pt{x: 1}\n}\n",
        "fn main() {\n    []int{1, 2}\n}\n",
        "fn main() {\n    fn() {}\n}\n",
        "fn main() {\n    f := fn() {\n        1 + 2\n    }\n    f()\n}\n",
    ];
    for (idx, src) in cases.iter().enumerate() {
        let codes = error_codes(src);
        assert_eq!(codes, vec!["HYB-E1002".to_string()], "case {idx}:\n{src}");
    }
}

#[test]
fn bare_value_only_builtins_and_type_conversions_reject_e1002() {
    let cases = [
        "fn main() {\n    s := []int{1, 2}\n    len(s)\n}\n",
        "fn main() {\n    s := []int{1, 2}\n    cap(s)\n}\n",
        "fn main() {\n    make([]int, 4)\n}\n",
        "fn main() {\n    new(int)\n}\n",
        "fn main() {\n    s := []int{1}\n    append(s, 2)\n}\n",
        "fn main() {\n    min(1, 2)\n}\n",
        "fn main() {\n    max(1, 2)\n}\n",
        "fn main() {\n    complex(1.0, 2.0)\n}\n",
        "fn main() {\n    c := complex(1.0, 2.0)\n    real(c)\n}\n",
        "fn main() {\n    c := complex(1.0, 2.0)\n    imag(c)\n}\n",
        "fn main() {\n    x := 3\n    int64(x)\n}\n",
        "fn f(s string) {\n    []byte(s)\n}\nfn main() {}\n",
        "type one_int = [1]int\nfn f(s []int) {\n    one_int(s)\n}\nfn main() {}\n",
        "type my_int = int\nfn main() {\n    x := 3\n    my_int(x)\n}\n",
    ];
    for (idx, src) in cases.iter().enumerate() {
        let codes = error_codes(src);
        assert_eq!(codes, vec!["HYB-E1002".to_string()], "case {idx}:\n{src}");
    }
}

#[test]
fn defer_of_value_only_builtins_and_type_conversions_rejects_e2001() {
    let cases = [
        "fn main() {\n    s := []int{1, 2}\n    defer len(s)\n}\n",
        "fn main() {\n    s := []int{1, 2}\n    defer cap(s)\n}\n",
        "fn main() {\n    defer make([]int, 2)\n}\n",
        "fn main() {\n    defer new(int)\n}\n",
        "fn main() {\n    s := []int{1}\n    defer append(s, 2)\n}\n",
        "fn main() {\n    defer min(1, 2)\n}\n",
        "fn main() {\n    defer max(1, 2)\n}\n",
        "fn main() {\n    defer complex(1.0, 2.0)\n}\n",
        "fn main() {\n    c := complex(1.0, 2.0)\n    defer real(c)\n}\n",
        "fn main() {\n    c := complex(1.0, 2.0)\n    defer imag(c)\n}\n",
        "fn main() {\n    x := 1\n    defer int64(x)\n}\n",
        "fn main() {\n    s := \"hi\"\n    defer []byte(s)\n}\n",
        "type one_int = [1]int\nfn main() {\n    s := []int{1}\n    defer one_int(s)\n}\n",
        "type my_int = int\nfn main() {\n    x := 1\n    defer my_int(x)\n}\n",
        "fn main() {\n    f := fn() {\n        s := []int{1}\n        defer len(s)\n    }\n    f()\n}\n",
    ];
    for (idx, src) in cases.iter().enumerate() {
        let codes = error_codes(src);
        assert_eq!(codes, vec!["HYB-E2001".to_string()], "case {idx}:\n{src}");
    }
}

#[test]
fn valid_statement_expressions_and_defers_accept() {
    let src = r#"
fn step() error {
    return nil
}

fn main() {
    ch := make(chan int, 1)
    ch <- 7
    <-ch
    close(ch)

    m := map[string]int{"a": 1}
    delete(m, "a")
    clear(m)

    dst := []int{0}
    src_s := []int{1}
    copy(dst, src_s)

    print(dst[0])
    println(src_s[0])
    recover()
    step()?

    (fn() {})()

    len := fn(xs []int) { _ = xs }
    len(dst)

    ch2 := make(chan int, 1)
    defer close(ch2)
    defer clear(dst)
    defer copy(dst, src_s)
    defer print(1)
    defer println(2)
    defer recover()
    defer step()?
}
"#;
    let r = check(src);
    assert!(r.diagnostics.is_empty(), "unexpected diagnostics: {:?}", r.diagnostics);
    assert_eq!(r.verdict, Verdict::Accept);
}

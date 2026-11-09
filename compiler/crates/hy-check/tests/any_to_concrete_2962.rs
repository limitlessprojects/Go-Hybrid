//! any_to_concrete_2962.rs: regression for #2962.
//!
//! A value of type `any` (an `any` variable or `recover()`) used where a
//! concrete type is expected passed hyb check, and go build failed with
//! "need type assertion". Hybrid has no type assertion, so these never build.
//! The reverse direction (concrete into `any`) and `any == concrete` equality
//! must keep accepting.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn rejects(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(cs.iter().any(|c| c == "HYB-E1002"), "expected HYB-E1002 for {:?}, got {:?}", src, cs);
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {:?}", src);
}

fn accepts(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(cs.is_empty(), "must accept {:?}, got {:?}", src, cs);
}

#[test]
fn any_var_into_int_var_init_rejects() {
    rejects("fn main() {\n    var a any = 1\n    var n int = a\n    fmt.println(n)\n}\n");
}

#[test]
fn recover_as_string_argument_rejects() {
    rejects("fn take(s string) { fmt.println(s) }\nfn main() {\n    defer fn() {\n        take(recover())\n    }()\n    panic(\"boom\")\n}\n");
}

#[test]
fn recover_returned_as_string_rejects() {
    rejects("fn get() string {\n    return recover()\n}\nfn main() {\n    fmt.println(get())\n}\n");
}

#[test]
fn recover_local_in_string_slice_literal_rejects() {
    rejects("fn main() {\n    defer fn() {\n        r := recover()\n        xs := []string{r}\n        fmt.println(xs)\n    }()\n    panic(\"boom\")\n}\n");
}

#[test]
fn recover_local_into_int_var_init_rejects() {
    rejects("fn main() {\n    r := recover()\n    var n int = r\n    fmt.println(n)\n}\n");
}

#[test]
fn any_param_passed_as_int_rejects() {
    rejects("fn take(n int) { fmt.println(n) }\nfn pass(a any) {\n    take(a)\n}\nfn main() {\n    pass(1)\n}\n");
}

// ---------------------------------------------------------------- accept:

#[test]
fn concrete_into_any_stays_accepted() {
    accepts("fn show(a any) { fmt.println(a) }\nfn main() {\n    var a any = 1\n    a = \"s\"\n    show(a)\n    show(3)\n    xs := []any{1, \"x\", a}\n    fmt.println(xs)\n}\n");
}

#[test]
fn any_to_any_and_recover_to_any_stay_accepted() {
    accepts("fn keep(a any) any {\n    return a\n}\nfn main() {\n    defer fn() {\n        var r any = recover()\n        fmt.println(keep(r))\n    }()\n    panic(\"boom\")\n}\n");
}

#[test]
fn any_equality_with_concrete_stays_accepted() {
    accepts("fn main() {\n    var a any = 1\n    if a == 1 {\n        fmt.println(\"one\")\n    }\n    if a != nil {\n        fmt.println(\"set\")\n    }\n}\n");
}

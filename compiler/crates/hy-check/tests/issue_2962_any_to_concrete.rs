//! issue_2962_any_to_concrete.rs — a value of type `any` (a variable declared
//! `any`, or `recover()`) used where a concrete type is expected must be
//! rejected: Hybrid has no type-assertion syntax, so the emitted Go can never
//! build (`need type assertion`, Row 13 BUILDABLE).

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("repro.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn assert_rejected(src: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject, got diagnostics: {:?}\nsrc:\n{src}", r.diagnostics);
}

fn assert_accepted(src: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept, got diagnostics: {:?}\nsrc:\n{src}", r.diagnostics);
}

#[test]
fn any_variable_in_concrete_var_init_is_rejected() {
    assert_rejected("fn main() {\n    var a any = 1\n    var n int = a\n    fmt.println(n)\n}\n");
}

#[test]
fn any_variable_in_concrete_assignment_is_rejected() {
    assert_rejected("fn main() {\n    var a any = 1\n    var n int = 0\n    n = a\n    fmt.println(n)\n}\n");
}

#[test]
fn recover_in_concrete_return_is_rejected() {
    assert_rejected("fn get() string {\n    return recover()\n}\n\nfn main() {\n    fmt.println(get())\n}\n");
}

#[test]
fn recover_in_concrete_argument_is_rejected() {
    assert_rejected(
        "fn take(s string) {\n    fmt.println(s)\n}\n\nfn main() {\n    defer fn() {\n        take(recover())\n    }()\n    panic(\"boom\")\n}\n",
    );
}

#[test]
fn recover_variable_in_concrete_slice_literal_is_rejected() {
    assert_rejected(
        "fn main() {\n    defer fn() {\n        r := recover()\n        xs := []string{r}\n        fmt.println(xs)\n    }()\n    panic(\"boom\")\n}\n",
    );
}

#[test]
fn any_to_any_and_any_targets_stay_accepted() {
    // `Interface` on the target side accepts everything; `any` -> `any` is fine;
    // assigning a concrete value to an `any` target is fine.
    assert_accepted("fn main() {\n    var a any = 1\n    var b any = a\n    fmt.println(b)\n}\n");
    assert_accepted("fn main() {\n    var a any = 1\n    a = \"str\"\n    fmt.println(a)\n}\n");
    assert_accepted("fn main() {\n    n := 1\n    var a any = n\n    fmt.println(a)\n}\n");
    assert_accepted("fn main() {\n    var a any = 1\n    fmt.println(a)\n}\n");
}

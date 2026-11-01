//! any_concrete_assign_2962.rs — tests for HYB-E1002 rejection of `any`-typed
//! values (declared `any`, or `recover()`) used where a CONCRETE type is
//! expected: var initializer, assignment, call argument, return value, and
//! composite-literal element (issue #2962).
//!
//! `kinds_conflict` fail-opened BOTH `Interface` directions:
//! `(Interface, _) | (_, Interface) => false`. The `want = Interface` half is
//! correct (an `any` target accepts every value). But `got = Interface` was
//! justified by "assigning an `any` to a concrete type needs a type assertion
//! the kind model does not track" — and Hybrid has no type-assertion syntax at
//! all (`x.(T)`, type switch). An `any` value can therefore NEVER become
//! concrete, so every such use fails `go build` with `need type assertion`.
//! This breaks the BUILDABLE guarantee (GUARANTEES.md Row 13).
//!
//! Explicit conversions `int(a)` are a DIFFERENT site (#2482 / PR #2484) and
//! stay out of scope here; equality `a == 5` stays legal in Go and is pinned
//! by a control.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn rejects_e1002(src: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject, got {:?}", r);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

fn accepts(src: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept, got {:?}", r);
    assert!(
        error_codes(&r).is_empty(),
        "expected 0 errors, got {:?}",
        r.diagnostics
    );
}

// ---------------------------------------------------------------------------
// Rejects: the issue's five reproducers, verbatim shapes
// ---------------------------------------------------------------------------

#[test]
fn any_var_into_concrete_var_init_rejects() {
    rejects_e1002(
        "\
fn main() {
    var a any = 1
    var n int = a
    fmt.println(n)
}
",
    );
}

#[test]
fn recover_into_call_arg_rejects() {
    rejects_e1002(
        "\
fn take(s string) { fmt.println(s) }
fn main() {
    defer fn() {
        take(recover())
    }()
    panic(\"boom\")
}
",
    );
}

#[test]
fn recover_into_return_rejects() {
    rejects_e1002(
        "\
fn get() string {
    return recover()
}
fn main() {
    fmt.println(get())
}
",
    );
}

#[test]
fn recover_into_slice_lit_element_rejects() {
    rejects_e1002(
        "\
fn main() {
    defer fn() {
        r := recover()
        xs := []string{r}
        fmt.println(xs)
    }()
    panic(\"boom\")
}
",
    );
}

#[test]
fn recover_var_into_concrete_var_init_rejects() {
    rejects_e1002(
        "\
fn main() {
    r := recover()
    var n int = r
    fmt.println(n)
}
",
    );
}

// ---------------------------------------------------------------------------
// Rejects: the rest of the value-position family
// ---------------------------------------------------------------------------

#[test]
fn any_into_plain_assignment_rejects() {
    rejects_e1002(
        "\
fn main() {
    var a any = \"s\"
    var n int = 0
    n = a
    fmt.println(n)
}
",
    );
}

#[test]
fn any_var_into_call_arg_rejects() {
    rejects_e1002(
        "\
fn take(s string) { fmt.println(s) }
fn main() {
    var a any = \"x\"
    take(a)
}
",
    );
}

#[test]
fn any_var_into_return_rejects() {
    rejects_e1002(
        "\
fn get() int {
    var a any = 1
    return a
}
fn main() {
    fmt.println(get())
}
",
    );
}

#[test]
fn any_into_map_lit_value_rejects() {
    rejects_e1002(
        "\
fn main() {
    var a any = 1
    m := map[string]int{\"k\": a}
    fmt.println(m)
}
",
    );
}

#[test]
fn any_into_struct_lit_field_rejects() {
    rejects_e1002(
        "\
struct packet { n int }
fn main() {
    var a any = 1
    p := packet{n: a}
    fmt.println(p)
}
",
    );
}

#[test]
fn any_into_typed_struct_target_rejects() {
    rejects_e1002(
        "\
struct packet { n int }
fn main() {
    var a any = 1
    var p packet = a
    fmt.println(p)
}
",
    );
}

#[test]
fn any_into_chan_elem_via_var_rejects() {
    // `var c chan int = a` — want = Chan(Int), got = Interface.
    rejects_e1002(
        "\
fn main() {
    var a any = 1
    var c chan int = a
    fmt.println(c)
}
",
    );
}

#[test]
fn any_into_slice_typed_target_rejects() {
    rejects_e1002(
        "\
fn main() {
    var a any = 1
    var xs []string = a
    fmt.println(xs)
}
",
    );
}

#[test]
fn any_into_option_payload_arg_rejects() {
    // `some(a)`/typed option target: an `any` cannot become the payload.
    rejects_e1002(
        "\
fn main() {
    var a any = 1
    var o option[int] = a
    fmt.println(o)
}
",
    );
}

// ---------------------------------------------------------------------------
// Accepts: the fail-open directions and legal shapes must stay silent
// ---------------------------------------------------------------------------

#[test]
fn concrete_into_any_still_accepts() {
    accepts(
        "\
fn main() {
    var a any = 1
    var s any = \"str\"
    fmt.println(a)
    fmt.println(s)
}
",
    );
}

#[test]
fn any_into_any_still_accepts() {
    accepts(
        "\
fn main() {
    var a any = 1
    var b any = a
    r := recover()
    var c any = r
    fmt.println(b)
    fmt.println(c)
}
",
    );
}

#[test]
fn any_param_call_still_accepts() {
    accepts(
        "\
fn take_any(x any) { fmt.println(x) }
fn main() {
    var a any = 1
    take_any(a)
    take_any(5)
    take_any(\"s\")
}
",
    );
}

#[test]
fn any_target_assignment_still_accepts() {
    accepts(
        "\
fn main() {
    var a any = 1
    a = \"str\"
    a = 2.5
    fmt.println(a)
}
",
    );
}

#[test]
fn equality_on_any_still_accepts() {
    // Go ALLOWS comparing an interface value with a concrete operand
    // (`a == 5` builds); the symmetric equality rule must not fire.
    accepts(
        "\
fn main() {
    var a any = 1
    if a == 5 {
        fmt.println(\"five\")
    }
    r := recover()
    if r == nil {
        fmt.println(\"clean\")
    }
}
",
    );
}

#[test]
fn any_into_any_container_lit_still_accepts() {
    accepts(
        "\
fn main() {
    var a any = 1
    xs := []any{a, 2, \"s\"}
    m := map[string]any{\"k\": a}
    fmt.println(xs)
    fmt.println(m)
}
",
    );
}

#[test]
fn any_as_return_of_any_fn_still_accepts() {
    accepts(
        "\
fn get() any {
    r := recover()
    return r
}
fn main() {
    fmt.println(get())
}
",
    );
}

//! #2901 — calls through fn values from an index, a struct field or another
//! call must not skip arity, argument and result checks.
//!
//! `hyb check` accepted every shape below while `go build` refused all of them
//! (`fs[0]("a")`, `v.cb("a")`, `mk()("a")`, `mk()(1, 2)`, `var s string =
//! mk()(1)`, `x + "a"` where `x := mk()(1)`, `var s string = m["a"](1)`),
//! because `call_result_kinds` had no arm for an `Expr::Call` / `Expr::Index`
//! callee and `check_call_operands` only compared arity and argument types for
//! `Ident` callees (plus `Field` arity when the base was a struct).
use hy_check::{check_file, CheckResult};
use hy_diag::Severity;
use hy_syntax::parse_file;

const PRELUDE: &str = "fn mk() fn(int) int {\n    return fn(x int) int {\n        return x\n    }\n}\n\nstruct h {\n    cb fn(int) int\n}\n";

fn check(body: &str) -> CheckResult {
    let src = format!("{PRELUDE}\nfn main() {{\n{body}}}\n");
    let ast = parse_file("inline.hy", &src).expect("parses");
    check_file(&ast)
}

fn codes(body: &str) -> Vec<String> {
    check(body)
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn rejects_argument_types_in_a_call_result_callee_2901() {
    // Row 1: `mk()("a")` — the callee is a call whose declared result is
    // `fn(int) int`, so the argument must be an `int`.
    let got = codes("    fmt.println(mk()(\"a\"))\n");
    assert!(
        got.contains(&"HYB-E1002".to_string()),
        "mk()(\"a\") must be rejected, got {got:?}"
    );
}

#[test]
fn rejects_arity_in_a_call_result_callee_2901() {
    // Row 2: `mk()(1, 2)` — one declared parameter, two arguments.
    let got = codes("    fmt.println(mk()(1, 2))\n");
    assert!(
        got.contains(&"HYB-E1002".to_string()),
        "mk()(1, 2) must be rejected, got {got:?}"
    );
}

#[test]
fn rejects_result_kind_of_a_call_result_callee_2901() {
    // Row 3: the call yields `int`, not `string`.
    let got = codes("    var s string = mk()(1)\n    fmt.println(s)\n");
    assert!(
        got.contains(&"HYB-E1002".to_string()),
        "var s string = mk()(1) must be rejected, got {got:?}"
    );
}

#[test]
fn rejects_operands_of_a_call_result_callee_2901() {
    // Row 4: `int` cannot be concatenated with a string.
    let got = codes("    x := mk()(1)\n    y := x + \"a\"\n    fmt.println(y)\n");
    assert!(
        got.contains(&"HYB-E1002".to_string()),
        "x + \"a\" (x int) must be rejected, got {got:?}"
    );
}

#[test]
fn rejects_argument_types_in_an_indexed_callee_2901() {
    // Row 5: the element type of `[]fn(int) int` states the parameter list.
    let got = codes(
        "    fs := []fn(int) int{fn(x int) int {\n        return x\n    }}\n    fmt.println(fs[0](\"a\"))\n",
    );
    assert!(
        got.contains(&"HYB-E1002".to_string()),
        "fs[0](\"a\") must be rejected, got {got:?}"
    );
}

#[test]
fn rejects_result_kind_of_an_indexed_callee_2901() {
    // Row 6: `m["a"](1)` yields `int`, not `string`.
    let got = codes(
        "    m := map[string]fn(int) int{\"a\": fn(x int) int {\n        return x\n    }}\n    var s string = m[\"a\"](1)\n    fmt.println(s)\n",
    );
    assert!(
        got.contains(&"HYB-E1002".to_string()),
        "var s string = m[\"a\"](1) must be rejected, got {got:?}"
    );
}

#[test]
fn rejects_argument_types_in_a_fn_typed_field_callee_2901() {
    // Row 7: `cb fn(int) int` states the parameter list.
    let got = codes(
        "    v := h{cb: fn(x int) int {\n        return x\n    }}\n    fmt.println(v.cb(\"a\"))\n",
    );
    assert!(
        got.contains(&"HYB-E1002".to_string()),
        "v.cb(\"a\") must be rejected, got {got:?}"
    );
}

#[test]
fn accepts_matching_callee_shapes_2901() {
    for body in [
        "    fmt.println(mk()(1))\n",
        "    var n int = mk()(1)\n    fmt.println(n)\n",
        "    fs := []fn(int) int{fn(x int) int {\n        return x\n    }}\n    fmt.println(fs[0](1))\n",
        "    v := h{cb: fn(x int) int {\n        return x\n    }}\n    fmt.println(v.cb(1))\n",
        "    m := map[string]fn(int) int{\"a\": fn(x int) int {\n        return x\n    }}\n    var n int = m[\"a\"](1)\n    fmt.println(n)\n",
    ] {
        let got = codes(body);
        assert!(got.is_empty(), "{body} must stay clean, got {got:?}");
    }
}

#[test]
fn accepts_fn_typed_parameters_and_locals_2901() {
    // A parameter of a `fn` type is declared through a `TypeExpr` too, so the
    // `apply` shapes below must keep building.
    let src = "fn apply(cb fn(int) int, v int) int {\n    return cb(v)\n}\n\nfn main() {\n    fmt.println(apply(fn(x int) int {\n        return x\n    }, 2))\n}\n";
    let ast = parse_file("inline.hy", src).expect("parses");
    let diags = check_file(&ast)
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect::<Vec<_>>();
    assert!(diags.is_empty(), "apply must stay clean, got {diags:?}");
}

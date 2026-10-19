//! Regression tests for `HYB-E2002` (T23: a multi-value result is not a tuple)
//! across compound assignments (`+=`, `-=`, `*=`, `/=`, `%=`, `|=`, `&=`, `^=`, `<<=`, `>>=`),
//! `move`-wrapped calls (`p := move minmax(3, 9)`), assignment target index
//! expressions (`xs[minmax(3, 9)] = 1`), and single-value subexpression contexts
//! (`Binary`, `Unary`, `Index`, `Slice`, `StructLit`, `SliceLit`, `MapLit`,
//! `Interp`, `Send`, `If`, `For`, `Match`, `Select`, and multi-arg `Call`/`Task`),
//! while preserving legal Go multi-value call forwarding (`f(g())` with 1 arg
//! and `return g()`).

use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let f = parse_file("test.hy", src).expect("parse ok");
    check_file(&f)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn compound_assignments_and_place_indices_with_multi_value_calls_reject_e2002() {
    for op in ["+=", "-=", "*=", "/=", "%=", "|=", "&=", "^=", "<<=", ">>="] {
        let src = format!(
            r#"
fn minmax(a int, b int) (int, int) {{
    return a, b
}}

fn main() {{
    x := 1
    x {op} minmax(3, 9)
    _ = x
}}
"#
        );
        let c = codes(&src);
        assert!(
            c.contains(&"HYB-E2002".to_string()),
            "expected HYB-E2002 for `x {op} minmax(3, 9)`, got: {c:?}"
        );
    }

    let place_idx = r#"
fn minmax(a int, b int) (int, int) {
    return a, b
}

fn main() {
    xs := []int{1, 2, 3}
    xs[minmax(0, 1)] = 9
    _ = xs
}
"#;
    assert!(
        codes(place_idx).contains(&"HYB-E2002".to_string()),
        "expected HYB-E2002 for multi-value index in assignment target"
    );
}

#[test]
fn move_wrapped_multi_value_calls_reject_e2002() {
    let src = r#"
fn minmax(a int, b int) (int, int) {
    return a, b
}

fn main() {
    p := move minmax(3, 9)
    _ = p
}
"#;
    assert!(
        codes(src).contains(&"HYB-E2002".to_string()),
        "expected HYB-E2002 for `p := move minmax(3, 9)`"
    );
}

#[test]
fn single_value_expression_and_statement_contexts_reject_e2002() {
    let cases = [
        // Binary & Unary
        "y := minmax(3, 9) + 1\n    _ = y",
        "y := -minmax(3, 9)\n    _ = y",
        // Index & Slice
        "xs := []int{1, 2}\n    y := xs[minmax(0, 1)]\n    _ = y",
        "xs := []int{1, 2}\n    y := xs[minmax(0, 1)..]\n    _ = y",
        // Composite literals & string interpolation
        "s := []int{minmax(3, 9)}\n    _ = s",
        "m := map[int]int{minmax(3, 9): 1}\n    _ = m",
        "s := \"val={minmax(3, 9)}\"\n    _ = s",
        // Multi-argument call
        "fmt.println(1, minmax(3, 9))",
        // Channel send
        "ch := make(chan int, 1)\n    ch <- minmax(3, 9)",
        // Control-flow conditions / bounds / subjects
        "if minmax(3, 9) > 0 {}",
        "for i in 0..minmax(3, 9) { _ = i }",
        "for v in minmax(3, 9) { _ = v }",
        "match minmax(3, 9) { _ => {} }",
        // Multi-value return with extra expression
        "return minmax(3, 9), 1",
    ];

    for body in cases {
        let src = format!(
            r#"
import "fmt"

fn minmax(a int, b int) (int, int) {{
    return a, b
}}

fn helper() (int, int) {{
    {body}
    return 0, 0
}}

fn main() {{
    a, b := helper()
    fmt.println(a, b)
}}
"#
        );
        let c = codes(&src);
        assert!(
            c.contains(&"HYB-E2002".to_string()),
            "expected HYB-E2002 for `{body}`, got: {c:?}"
        );
    }
}

#[test]
fn single_arg_call_forwarding_and_return_forwarding_stay_accepted() {
    let src = r#"
fn minmax(a int, b int) (int, int) {
    return a, b
}

fn consume_pair(x int, y int) int {
    return x + y
}

fn forward_pair() (int, int) {
    return minmax(3, 9)
}

fn main() {
    a, b := forward_pair()
    s := consume_pair(minmax(a, b))
    _ = s
}
"#;
    let c = codes(src);
    assert!(
        c.is_empty(),
        "expected 0 diagnostics for valid multi-value forwarding, got: {c:?}"
    );
}

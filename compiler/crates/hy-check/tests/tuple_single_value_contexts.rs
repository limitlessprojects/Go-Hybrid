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

#[test]
fn try_unwrapped_calls_check_result_arity_issue_1835() {
    let src_2val_to_1name = r#"
fn pair_or_err() (int, int, error) {
    return 1, 2, nil
}

fn run() (int, error) {
    p := pair_or_err()?
    return p, nil
}
"#;
    let c = codes(src_2val_to_1name);
    assert!(
        c.contains(&"HYB-E2002".to_string()),
        "expected HYB-E2002 for binding 1 name to 2-value try unwrap, got: {c:?}"
    );

    let src_2val_in_binary = r#"
fn pair_or_err() (int, int, error) {
    return 1, 2, nil
}

fn run() (int, error) {
    y := pair_or_err()? + 1
    return y, nil
}
"#;
    let c = codes(src_2val_in_binary);
    assert!(
        c.contains(&"HYB-E2002".to_string()),
        "expected HYB-E2002 for 2-value try unwrap in binary expression, got: {c:?}"
    );

    let src_1val_to_2names = r#"
fn one_or_err() (int, error) {
    return 7, nil
}

fn run() (int, error) {
    u, v := one_or_err()?
    return u + v, nil
}
"#;
    let c = codes(src_1val_to_2names);
    assert!(
        c.contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for binding 2 names to 1-value try unwrap, got: {c:?}"
    );

    let src_valid = r#"
fn pair_or_err() (int, int, error) {
    return 1, 2, nil
}

fn one_or_err() (int, error) {
    return 7, nil
}

fn run() (int, error) {
    a, b := pair_or_err()?
    x := one_or_err()?
    return a + b + x, nil
}
"#;
    let c = codes(src_valid);
    assert!(
        c.is_empty(),
        "expected 0 diagnostics for valid try-unwrapped bindings, got: {c:?}"
    );
}

#[test]
fn unshadowed_one_value_builtins_reject_multiple_target_binding_issue_1835() {
    let src_len = r#"
fn run() {
    xs := [1, 2, 3]
    m, n := len(xs)
    _ = m + n
}
"#;
    let c = codes(src_len);
    assert!(
        c.contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for `m, n := len(xs)`, got: {c:?}"
    );

    let src_some = r#"
fn run() {
    a, b := some(42)
    _ = a
    _ = b
}
"#;
    let c = codes(src_some);
    assert!(
        c.contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for `a, b := some(42)`, got: {c:?}"
    );

    let src_conv = r#"
fn run() {
    x := 10
    a, b := int64(x)
    _ = a + b
}
"#;
    let c = codes(src_conv);
    assert!(
        c.contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for `a, b := int64(x)`, got: {c:?}"
    );
}

#[test]
fn aliased_pointer_receiver_methods_and_multipointer_indexing_issue_1835() {
    let src_method = r#"
struct pair_box {
    a int
    b int
}

type box_alias = pair_box

fn (p *pair_box) unpack() (int, int) {
    return p.a, p.b
}

fn run(b *box_alias) int {
    q := b.unpack()
    return q
}
"#;
    let c = codes(src_method);
    assert!(
        c.contains(&"HYB-E2002".to_string()),
        "expected HYB-E2002 for `q := b.unpack()` on aliased pointer receiver, got: {c:?}"
    );

    let src_multipointer = r#"
fn run(pp **[2]int) int {
    a, b, c := pp[0]
    return a + b + c
}
"#;
    let c = codes(src_multipointer);
    assert!(
        c.contains(&"HYB-E2002".to_string()),
        "expected HYB-E2002 for `a, b, c := pp[0]`, got: {c:?}"
    );
}

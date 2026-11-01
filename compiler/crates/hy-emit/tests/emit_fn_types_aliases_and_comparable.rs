//! Integration tests for `hy-emit/src/lib.rs` emission fixes:
//! 1. `TypeExpr::Fn` emission in `type_text` and `zero_value`:
//!    - parenthesizes multiple return types (`func(int) (int, error)`),
//!    - omits leading spaces on unnamed parameter types (`func(int, uint8) bool`),
//!    - fills `_` for unnamed parameters when mixed with named parameters, and
//!    - emits `"nil"` as the zero value of a function type (or alias to one).
//! 2. Type-alias resolution in `binding_kind_of_ty` and `cur_option_result`:
//!    - aliases to `chan T`, `map[K]V`, and `set[T]` preserve `BindingKind`,
//!    - `return none` lowers to `<option>{}` for named option results
//!      `fn find() (res ?int)` and aliased option results `type maybe_int = ?int`.
//! 3. `collect_comparable_in_ty` / `collect_comparable_in_expr` on array keys
//!    (`map[[2]k]v`, `set[[2]k]`) and folded generic arguments (`box[map[k]v]`,
//!    `box[set[k]]`), plus `mask_go_noncode` filtering for unused `"errors"`
//!    imports and prelude injection.

use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit(src: &str) -> String {
    let file = parse_file(src).expect("source must parse");
    emit_go(&file).expect("source must emit")
}

#[test]
fn fn_types_format_multi_results_unnamed_params_and_nil_zero_value() {
    let src = r#"
type callback = fn(int, u8) (int, error)
type mixed_cb = fn(a int, string) bool

fn fail() (int, error) {
    return 0, errors.new("boom")
}

fn load_cb() (callback, error) {
    _ = fail()?
    var m mixed_cb
    _ = m
    return nil, nil
}

fn main() {
}
"#;
    let go = emit(src);
    assert!(
        go.contains("type callback = func(int, uint8) (int, error)"),
        "expected parenthesized multi-result func type and clean unnamed params, got:\n{go}"
    );
    assert!(
        go.contains("type mixed_cb = func(a int, _ string) bool"),
        "expected mixed named/unnamed func type params to fill `_`, got:\n{go}"
    );
    assert!(
        go.contains("return nil, hyErr0"),
        "expected function-type alias zero value to be `nil` on `?` propagation, got:\n{go}"
    );
}

#[test]
fn type_aliases_preserve_binding_kind_and_option_return_none() {
    let src = r#"
type jobs = chan int
type scores = map[string]int
type tags = set[string]
type maybe_int = ?int

fn find_named() (res ?int) {
    return none
}

fn find_fallible() (?int, error) {
    return none, nil
}

fn find_aliased() maybe_int {
    return none
}

fn drain(ch jobs, m scores, mut s tags) int {
    s.insert("ok")
    mut total = s.len()
    for v in ch {
        total = total + v
    }
    for k in m {
        if s.contains(k) {
            total = total + 1
        }
    }
    return total
}

fn main() {
}
"#;
    let go = emit(src);
    assert!(
        go.contains("return option[int]{}"),
        "expected named option result `return none` to emit `return option[int]{{}}`, got:\n{go}"
    );
    assert!(
        go.contains("return option[int]{}, nil"),
        "expected fallible option result `return none, nil` to emit `return option[int]{{}}, nil`, got:\n{go}"
    );
    assert!(
        go.contains("return maybe_int{}"),
        "expected aliased option result `return none` to emit `return maybe_int{{}}`, got:\n{go}"
    );
    assert!(
        go.contains("for v := range ch {"),
        "expected aliased channel single-var range `for v := range ch`, got:\n{go}"
    );
    assert!(
        go.contains("for k := range m {"),
        "expected aliased map single-var range `for k := range m`, got:\n{go}"
    );
}

#[test]
fn comparable_inference_handles_array_keys_and_folded_generic_type_args() {
    let src = r#"
struct box[t] {
    val t
}

struct holder[k, v] {
    by_pair map[[2]k]v
    nested  box[map[k]v]
    seen    box[set[[2]k]]
}

fn make_pair_map[k, v]() map[[2]k]v {
    return map[[2]k]v{}
}

fn main() {
}
"#;
    let go = emit(src);
    assert!(
        go.contains("type holder[k comparable, v any] struct"),
        "expected `k comparable` on struct holder with array/folded map keys, got:\n{go}"
    );
    assert!(
        go.contains("func make_pair_map[k comparable, v any]() map[[2]k]v"),
        "expected `k comparable` on fn make_pair_map with array map key, got:\n{go}"
    );
}

#[test]
fn string_literals_do_not_trigger_unused_errors_import_or_preludes() {
    let src = r#"
import "errors"
import "fmt"

fn wrap_it(err error) error {
    return errors.wrap(err, "errors.New failed: option[int] some(1) s.contains(x) hy_pop(s)")
}

fn main() {
    fmt.println("just text: set[int] .insert(1) .remove(1)")
}
"#;
    let go = emit(src);
    assert!(
        !go.contains("\"errors\""),
        "string literal mentioning `errors.` must not keep unused `\"errors\"` import:\n{go}"
    );
    assert!(
        !go.contains("Hybrid prelude:"),
        "string literals mentioning option/set/hy_pop must not inject preludes:\n{go}"
    );
}

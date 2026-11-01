Contributor details
Your Expensify account email: 3055073676@qq.com
Upwork Profile Link: https://www.upwork.com/freelancers/~01a6c3832b8108493d

## Proposal

### Please re-state the problem that we are trying to solve in this issue.

Under Row 13 of `GUARANTEES.md` (Soundness / rule `BUILDABLE`), the Hybrid compiler guarantees that every program accepted by `hyb check` with 0 diagnostics must emit buildable Go that compiles cleanly under standard Go toolchains (`go build` / `go vet`).

Currently, when a multi-value function call (e.g. `fn two_ints() (int, int)`) is placed as an element in a slice composite literal `[]int{ two_ints() }` or as a key/value in a map composite literal `map[string]int{ "val": two_ints() }`, `hyb check` passes cleanly with 0 diagnostics.

However, according to the Go Language Specification (§Composite literals):
> *"In composite literals of array, slice, or map type, every element or value must be a single-value expression assignable to the corresponding element type."*

Consequently, supplying multi-value calls in slice or map literals is rejected by standard Go:
```go
package main

fn two_ints() (int, int) {
    return 10, 20
}

fn main() {
    var s = []int{ two_ints() }
    var m = map[string]int{ "val": two_ints() }
    println(s[0], m["val"])
}
```
Actual behavior:
- `hyb check`: 0 errors (passes cleanly).
- `go build` / `go vet`:
  ```text
  ./main.go:8:19: multiple-value two_ints() (value of type (int, int)) in single-value context
  ./main.go:9:34: multiple-value two_ints() (value of type (int, int)) in single-value context
  ```
  Exits with code 1.

This represents a direct violation of Soundness Rule Row 13 (`BUILDABLE`).

---

### What is the root cause of that problem?

The root cause lies in `compiler/crates/hy-check/src/buildable.rs`, specifically in `check_slice_lit_items` (lines 3597–3659) and `check_map_lit_entries` (lines 3661–3719), as well as `check_array_lit_items` (lines 3536–3595).

1. **Missing `check_single_value_position` calls**:
   In `buildable.rs`, single-value expression contexts strictly guard their operands via `self.check_single_value_position`:
   - Call arguments (`buildable.rs:1399`): `"a call argument"`
   - Type conversions (`buildable.rs:2194`): `"a type conversion"`
   - Unary operations (`buildable.rs:2760`): `"a unary operation"`
   - Binary operations (`buildable.rs:2887-2888`): `"a binary operation"`
   - Index expressions (`buildable.rs:3274`): `"an index expression"`
   - Slice bounds (`buildable.rs:3408`): `"a slice bound"`

   However, neither `check_slice_lit_items` nor `check_map_lit_entries` calls `self.check_single_value_position` on elements, keys, or values.

2. **First-result kind truncation in `kind_of_expr`**:
   In `buildable.rs:1967-1971`:
   ```rust
   Expr::Call { callee, args, .. } => match self.call_result_kinds(callee, args) {
       Some(res) if res.is_empty() => OpKind::Void,
       Some(res) => res[0].clone(),
       None => OpKind::Unknown,
   },
   ```
   When `item` or `v` is an `Expr::Call` returning multiple values (e.g., `(int, int)`):
   - In `check_slice_lit_items`: `let ik = self.kind_of_expr(item);` inspects only `res[0]` (`OpKind::Int`), matching `expected` (`OpKind::Int`). As a result, `ik == expected` evaluates to `true`, and line 3637 executes `continue;`, bypassing diagnostic emission.
   - In `check_map_lit_entries`: `let vk = self.kind_of_expr(v);` evaluates to `res[0]` (`OpKind::Int`). Since `exp_val` is `OpKind::Int`, `vk != exp_val` evaluates to `false`, skipping `kinds_conflict(&exp_val, &vk)`.
   - The same omission occurs for map keys (`k`) and array literal items in `check_array_lit_items`.

Because `non_single_value_count` is never checked on these expressions, multi-value and void calls slip through without triggering `HYB-E1002`.

---

### What changes do you think we should make in order to solve the problem?

In `compiler/crates/hy-check/src/buildable.rs`:
1. In `check_slice_lit_items`, invoke `self.check_single_value_position(item, "a slice literal element", /*binding_position=*/ false);` at the top of the item loop.
2. In `check_map_lit_entries`, invoke `self.check_single_value_position(k, "a map literal key", /*binding_position=*/ false);` and `self.check_single_value_position(v, "a map literal value", /*binding_position=*/ false);` at the top of the entry loop.
3. In `check_array_lit_items`, invoke `self.check_single_value_position(item, "an array literal element", /*binding_position=*/ false);` at the top of the item loop to enforce composite literal symmetry per Go §Composite literals.

#### Diff:
```diff
--- a/compiler/crates/hy-check/src/buildable.rs
+++ b/compiler/crates/hy-check/src/buildable.rs
@@ -3540,6 +3540,7 @@ impl<'a> Checker<'a> {
         if expected == OpKind::Unknown {
             return;
         }
         for item in items {
+            self.check_single_value_position(item, "an array literal element", /*binding_position=*/ false);
             let bare = unwrap_parens(item);
             if matches!(bare, Expr::Lit { kind: LitKind::Nil, .. }) {
                 continue;
@@ -3601,6 +3602,7 @@ impl<'a> Checker<'a> {
         if expected == OpKind::Unknown {
             return;
         }
         for item in items {
+            self.check_single_value_position(item, "a slice literal element", /*binding_position=*/ false);
             let bare = unwrap_parens(item);
             if matches!(bare, Expr::Lit { kind: LitKind::Nil, .. }) {
                 continue;
@@ -3669,6 +3671,8 @@ impl<'a> Checker<'a> {
         let resolved_val = self.ck.resolve_type_alias(val_ty);
         let exp_val = self.kind_of_ty(&resolved_val, 0);
         for (k, v) in entries {
+            self.check_single_value_position(k, "a map literal key", /*binding_position=*/ false);
+            self.check_single_value_position(v, "a map literal value", /*binding_position=*/ false);
             if exp_key != OpKind::Unknown {
                 let kk = self.kind_of_expr(k);
                 if kk != OpKind::Unknown
```

#### How it works:
1. `check_single_value_position` calls `non_single_value_count(expr)`.
2. When the expression is a multi-value call (e.g. `two_ints()`), `non_single_value_count` returns `Some(n)` (where `n >= 2`). Because `binding_position` is `false`, it emits `HYB-E1002`:
   > ``"`two_ints()` produces 2 values and cannot be used as one in a slice literal element — Go refuses `multiple-value ... in single-value context` (HYB-E1002)"``
3. When the expression is a void function call (e.g. `no_val()`), `non_single_value_count` returns `Some(0)` and emits `HYB-E1002`:
   > ``"`no_val()` produces no value and cannot be used as one in a slice literal element — Go refuses `no_val() (no value) used as value` (HYB-E1002)"``
4. For single-value expressions (literals, variables, single-result calls), `non_single_value_count` returns `None`, allowing normal type and assignability checking to proceed unimpeded.

---

### What alternative solutions did you explore? (Optional)

1. **Modifying `kind_of_expr` to return an `OpKind::Tuple` / multi-value variant:**
   - *Exploration*: Changing `kind_of_expr` so multi-value calls return a composite kind rather than truncating to `res[0]`.
   - *Why rejected*: `kind_of_expr` is called in dozens of positions across `buildable.rs`. Altering its signature and return variants would cause widespread churn across the compiler, complicate existing single-value match arms, break unrelated inferences, and introduce substantial regression risk.

2. **Intercepting inside the AST visitor `expr(&mut self, e: &Expr)` at `Expr::SliceLit` and `Expr::MapLit`:**
   - *Exploration*: Calling `check_single_value_position` directly in the match arms of `expr()` when traversing AST literal nodes.
   - *Why rejected*: In `buildable.rs`, composite literal verification is systematically encapsulated inside dedicated helpers (`check_slice_lit_items`, `check_map_lit_entries`, `check_array_lit_items`). Placing the check inside these helpers keeps traversal clean, automatically covers nested/recursively-checked literals, and centralizes diagnostic reporting.

---

### What automated tests can you add to prevent this from re-occurring?

Add unit tests to `compiler/crates/hy-check/tests/multivalue_and_void_positions.rs`:

```rust
// ---------------------------------------------------------------------------
// Composite literals: slice and map element single-value position (#1849)
// ---------------------------------------------------------------------------

#[test]
fn multi_value_call_in_slice_literal_element_is_rejected() {
    // Multi-value call directly in slice literal
    assert_code(
        r#"
fn two_ints() (int, int) {
    return 10, 20
}

fn main() {
    s := []int{ two_ints() }
    fmt.println("{s}")
}
"#,
        "HYB-E1002",
    );

    // Parenthesized multi-value call in slice literal
    assert_code(
        r#"
fn two_ints() (int, int) {
    return 10, 20
}

fn main() {
    s := []int{ (two_ints()) }
    fmt.println("{s}")
}
"#,
        "HYB-E1002",
    );

    // Array literal with multi-value call
    assert_code(
        r#"
fn two_ints() (int, int) {
    return 10, 20
}

fn main() {
    a := [2]int{ two_ints() }
    fmt.println("{a}")
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn multi_value_call_in_map_literal_is_rejected() {
    // Multi-value call as map value
    assert_code(
        r#"
fn two_ints() (int, int) {
    return 10, 20
}

fn main() {
    m := map[string]int{ "val": two_ints() }
    fmt.println("{m}")
}
"#,
        "HYB-E1002",
    );

    // Multi-value call as map key
    assert_code(
        r#"
fn two_ints() (int, int) {
    return 10, 20
}

fn main() {
    m := map[int]string{ two_ints(): "val" }
    fmt.println("{m}")
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn void_call_in_composite_literals_is_rejected() {
    // Void call in slice literal
    assert_code(
        r#"
fn v() {}

fn main() {
    s := []int{ v() }
    fmt.println("{s}")
}
"#,
        "HYB-E1002",
    );

    // Void call in map value
    assert_code(
        r#"
fn v() {}

fn main() {
    m := map[string]int{ "val": v() }
    fmt.println("{m}")
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn single_value_call_in_composite_literals_stays_accepted() {
    assert_ok(
        r#"
fn one_int() int {
    return 42
}

fn one_str() string {
    return "ok"
}

fn main() {
    s := []int{ one_int(), 100 }
    m := map[string]int{ one_str(): one_int() }
    fmt.println("{s} {m}")
}
"#,
    );
}
```
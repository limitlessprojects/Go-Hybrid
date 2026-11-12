Contributor details
Your Expensify account email: 3055073676@qq.com
Upwork Profile Link: https://www.upwork.com/freelancers/~01a6c3832b8108493d

## Proposal

### Please re-state the problem that we are trying to solve in this issue.

Under Row 13 of `GUARANTEES.md` (Soundness / rule `BUILDABLE`), the Hybrid compiler guarantees that every program accepted by `hyb check` with 0 diagnostics must emit buildable Go that compiles cleanly under standard Go toolchains (`go build` / `go vet`).

Currently, when a map composite literal `map[K]V{...}` uses `nil` as a key where `K` is a non-nilable type (such as `int`, `string`, `bool`, `float64`, or a struct), `hyb check` emits 0 diagnostics and reports a clean `ACCEPT`.

However, according to the Go Language Specification (§Map types):
> *"The key type must be comparable. If the key type is a non-nilable type (e.g. int, string, bool, float64, struct, array), using nil as a key is a compile-time error."*

Consequently, standard Go toolchains reject such map composite literals:
```go
package main

fn main() {
    var m = map[int]string{ nil: "alpha" }
    println(m[0])
}
```

Actual behavior:
- `hyb check`: 0 diagnostics / passes cleanly (`Verdict::Accept`).
- `go build` / `go vet`:
  ```text
  cannot use nil as int value in map key
  ```
  Exits with code 1.

This represents a direct violation of Soundness Rule Row 13 (`BUILDABLE`).

---

### What is the root cause of that problem?

The defect stems from two interconnected omissions across `compiler/crates/hy-check/src/sendability.rs` and `compiler/crates/hy-check/src/buildable.rs`:

1. **Missing key inspection in `sendability.rs::check_composite_nils` (lines 1035–1051)**:
   In `sendability.rs`, nil literal validation in composite literals is governed under the N1 doctrine (only `error` and foreign Go types can hold nil; all Hybrid-declared types have zero values). In `check_composite_nils`:
   ```rust
   Expr::MapLit { value: vty, entries, .. } => {
       let vshape = self.shape_of_type(vty);
       let vty_text = vty.render();
       for (k, v) in entries {
           if Checker::is_nil_literal(v) && !Checker::is_nilable_shape(&vshape) {
               self.emit_nil_type_error_text(
                   &vshape,
                   vty_text.clone(),
                   v.span(),
                   &format!("the map value slot `{}`", vty_text),
               );
               continue;
           }
           self.check_composite_nils(st, v);
           self.check_composite_nils(st, k);
       }
   }
   ```
   - The pattern match destructures only `value: vty` and ignores the map `key` type expression.
   - The loop checks only the map value `v` against `vshape` with `Checker::is_nil_literal(v) && !Checker::is_nilable_shape(&vshape)`. It completely omits inspecting the map key `k`.
   - The recursive call `self.check_composite_nils(st, k)` only recurses into nested composite structures (`StructLit`, `MapLit`, `SliceLit`). When `k` is a literal `nil` (`Expr::Lit { kind: LitKind::Nil, .. }`), it hits the wildcard arm `_ => {}` and returns silently without emitting `HYB-E4001`.

2. **Unchecked `OpKind::Unknown` in `buildable.rs::check_map_lit_entries` (lines 3661–3719)**:
   In `buildable.rs`, `check_map_lit_entries` validates assignability and kind conflicts (`HYB-E1002`):
   ```rust
   for (k, v) in entries {
       if exp_key != OpKind::Unknown {
           let kk = self.kind_of_expr(k);
           if kk != OpKind::Unknown
               && kk != exp_key
               && !(exp_key == OpKind::Float
                   && kk == OpKind::Int
                   && crate::const_overflow::const_int_value(k).is_some())
               && kinds_conflict(&exp_key, &kk)
           {
               self.emit_e1002(...);
           }
       }
       ...
   }
   ```
   - In Go-Hybrid, `LitKind::Nil` evaluates to `OpKind::Unknown` in `kind_of_expr` (line 1844: `LitKind::Nil => OpKind::Unknown`).
   - Consequently, when `k` is `nil`, `kk` is `OpKind::Unknown`. The guard `if kk != OpKind::Unknown` evaluates to `false`, silently bypassing `kinds_conflict`.
   - Furthermore, unlike map values `v` (`buildable.rs:3694-3696`), slice items (`buildable.rs:3604-3606`), and array items (`buildable.rs:3543-3545`), which explicitly guard `unwrap_parens(...)` against `LitKind::Nil` to delegate nil validation cleanly to `sendability.rs`, `check_map_lit_entries` lacked symmetrical guard handling for `k`.

Because neither pass inspects `k` for non-nilable `nil`, the program passes `hyb check` cleanly and fails during `go build`.

---

### What changes do you think we should make in order to solve the problem?

1. **In `compiler/crates/hy-check/src/sendability.rs` (`check_composite_nils`)**:
   - Destructure `key` in `Expr::MapLit { key, value: vty, entries, .. }`.
   - Compute `kshape = self.shape_of_type(key)` and `kty_text = key.render()`.
   - Before recursing, inspect `k`: if `Checker::is_nil_literal(k) && !Checker::is_nilable_shape(&kshape)`, emit a diagnostic via `self.emit_nil_type_error_text` with context `"the map key slot `{}`"`.

2. **In `compiler/crates/hy-check/src/buildable.rs` (`check_map_lit_entries`)**:
   - Add `let bare_k = unwrap_parens(k); if matches!(bare_k, Expr::Lit { kind: LitKind::Nil, .. }) { continue; }` at the top of the key check.
   - This ensures symmetry with map values and slice/array items, cleanly delegating composite literal nil enforcement to the N1 pass in `sendability.rs` and preventing conflicting diagnostics.

#### Unified Diff

```diff
--- a/compiler/crates/hy-check/src/sendability.rs
+++ b/compiler/crates/hy-check/src/sendability.rs
@@ -1032,10 +1032,26 @@ impl<'a> Checker<'a> {
                     }
                 }
             }
-            Expr::MapLit { value: vty, entries, .. } => {
+            Expr::MapLit {
+                key,
+                value: vty,
+                entries,
+                ..
+            } => {
+                let kshape = self.shape_of_type(key);
+                let kty_text = key.render();
                 let vshape = self.shape_of_type(vty);
                 let vty_text = vty.render();
                 for (k, v) in entries {
+                    if Checker::is_nil_literal(k) && !Checker::is_nilable_shape(&kshape) {
+                        self.emit_nil_type_error_text(
+                            &kshape,
+                            kty_text.clone(),
+                            k.span(),
+                            &format!("the map key slot `{}`", kty_text),
+                        );
+                        continue;
+                    }
                     if Checker::is_nil_literal(v) && !Checker::is_nilable_shape(&vshape) {
                         self.emit_nil_type_error_text(
                             &vshape,
--- a/compiler/crates/hy-check/src/buildable.rs
+++ b/compiler/crates/hy-check/src/buildable.rs
@@ -3670,6 +3670,10 @@ impl<'a> Checker<'a> {
         let exp_val = self.kind_of_ty(&resolved_val, 0);
         for (k, v) in entries {
             if exp_key != OpKind::Unknown {
+                let bare_k = unwrap_parens(k);
+                if matches!(bare_k, Expr::Lit { kind: LitKind::Nil, .. }) {
+                    continue;
+                }
                 let kk = self.kind_of_expr(k);
                 if kk != OpKind::Unknown
                     && kk != exp_key
```

#### How the Fix Operates:
1. When encountering `map[int]string{ nil: "alpha" }`, `sendability.rs` resolves `kshape` to `TypeShape::Primitive`.
2. `Checker::is_nilable_shape(&kshape)` evaluates to `false` (only `ForeignOpaque` is nilable in Hybrid).
3. `Checker::is_nil_literal(k)` evaluates to `true` (unwrapping parentheses if present).
4. `self.emit_nil_type_error_text` emits `HYB-E4001`:
   > ``"`nil` is not a valid value for the map key slot `int` (int): int has no nil (N1)"``
   with notes explaining the zero-value doctrine (N3) and suggesting the type's zero value or `option[T]`.
5. `buildable.rs` skips `bare_k == nil`, avoiding duplicate/conflicting `HYB-E1002` diagnostic output while preserving sound rejection across the compiler pipeline.

---

### What alternative solutions did you explore? (Optional)

1. **Emitting `HYB-E1002` directly inside `buildable.rs::check_map_lit_entries`**:
   - *Exploration*: Checking if `bare_k` is `LitKind::Nil` inside `buildable.rs` and emitting `HYB-E1002` (`"cannot use nil as int value in map key"`).
   - *Why rejected*: Violates compiler architectural layering and diagnostic consistency. Throughout Go-Hybrid, nil-safety enforcement is strictly owned by the N1 doctrine in `sendability.rs` (`HYB-E4001`). Emitting `HYB-E1002` here would bypass the actionable N1/N3 fix notes provided by `emit_nil_type_error_text` and create diagnostic asymmetry between map keys (`HYB-E1002`) and map values / slice items (`HYB-E4001`).

2. **Modifying `kind_of_expr` to return an explicit `OpKind::Nil`**:
   - *Exploration*: Changing `LitKind::Nil => OpKind::Unknown` in `buildable.rs:1844` to return a distinct kind variant `OpKind::Nil`.
   - *Why rejected*: `kind_of_expr` is called at dozens of sites throughout `buildable.rs` (arithmetic expressions, shift operations, channel operations, switch cases). Altering `OpKind` semantics would require broad pattern matching updates across the entire checker crate and introduce substantial regression risk.

---

### What automated tests can you add to prevent this from re-occurring?

Add regression test coverage in `compiler/crates/hy-check/tests/callee_and_nil.rs` and `compiler/crates/hy-check/tests/slice_and_map_literal_item_types.rs`:

```rust
// ---------------------------------------------------------------------------
// Issue #1848: Map composite literal nil key checks (HYB-E4001 / BUILDABLE)
// ---------------------------------------------------------------------------

#[test]
fn nil_map_key_with_non_nilable_key_type_is_rejected() {
    // 1. Primitive int key type
    let r1 = check(r#"
fn main() {
    m := map[int]string{ nil: "alpha" }
    _ = m
}
"#);
    let cs1 = codes(&r1);
    assert!(
        cs1.iter().any(|c| c == "HYB-E4001"),
        "expected HYB-E4001 for `map[int]string{{ nil: \"alpha\" }}`, got {:?}",
        cs1
    );
    assert_eq!(r1.verdict, Verdict::Reject);

    // 2. Primitive string key type
    let r2 = check(r#"
fn main() {
    m := map[string]int{ nil: 42 }
    _ = m
}
"#);
    let cs2 = codes(&r2);
    assert!(
        cs2.iter().any(|c| c == "HYB-E4001"),
        "expected HYB-E4001 for `map[string]int{{ nil: 42 }}`, got {:?}",
        cs2
    );
    assert_eq!(r2.verdict, Verdict::Reject);

    // 3. Bool key type
    let r3 = check(r#"
fn main() {
    m := map[bool]string{ nil: "true_val" }
    _ = m
}
"#);
    let cs3 = codes(&r3);
    assert!(
        cs3.iter().any(|c| c == "HYB-E4001"),
        "expected HYB-E4001 for `map[bool]string{{ nil: \"true_val\" }}`, got {:?}",
        cs3
    );
    assert_eq!(r3.verdict, Verdict::Reject);
}

#[test]
fn parenthesized_nil_map_key_is_rejected() {
    let r = check(r#"
fn main() {
    m := map[int]string{ (nil): "alpha" }
    _ = m
}
"#);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E4001"),
        "expected HYB-E4001 for `(nil)` map key, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn nested_map_literal_nil_key_is_rejected() {
    let r = check(r#"
fn main() {
    outer := map[string]map[int]string{
        "inner": { nil: "val" },
    }
    _ = outer
}
"#);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E4001"),
        "expected HYB-E4001 for nested map literal nil key, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn valid_map_keys_and_values_stay_accepted() {
    let r = check(r#"
fn main() {
    m1 := map[int]string{ 0: "zero", 1: "one" }
    m2 := map[string]int{ "": 0, "foo": 42 }
    _ = m1
    _ = m2
}
"#);
    assert!(
        codes(&r).is_empty(),
        "valid map keys must stay accepted without errors, got {:?}",
        codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}
```
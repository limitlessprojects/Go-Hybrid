## Contributor Details
- **GitHub**: @fangjian-bit
- **Expensify Account**: `3055073676@qq.com`
- **Upwork Profile**: https://www.upwork.com/freelancers/~01a6c3832b8108493d
- **Web3 Wallet (Base / EVM USDC)**: `0x71C836eB3F9354087961b72e04313B014Eb3E059`

---

### Root Cause Analysis (RCA)

Under **Row 13 of GUARANTEES.md (Rule BUILDABLE)**, any program accepted by `hyb check` must emit buildable Go.

#### 1. Missing Operand Kind Checks for `min` and `max`
- In `compiler/crates/hy-check/src/builtins.rs:394–408`, the handler for `"min"` and `"max"` only validates arity:
  ```rust
  "min" | "max" => {
      if args.is_empty() {
          self.emit_e2005(span, format!("`{}` requires at least 1 argument, found 0 (HYB-E2005)", name), ...);
      }
  }
  ```
- In `compiler/crates/hy-check/src/buildable.rs:4116–4250`, `check_builtin_operand_kinds` handles `"len"`, `"cap"`, `"close"`, `"delete"`, and `"append"`, but has **no branch for `"min"` or `"max"`**.
- Under the Go language specification, `min` and `max` strictly accept operands of ordered types (integers, floats, and strings). Passing non-ordered operands—such as booleans (`min(true, false)`), struct composite literals (`min(Point{x: 1}, Point{x: 2})`), slice literals (`min([]int{1}, []int{2})`), or map literals—is accepted with 0 diagnostics by `hyb check`, but subsequently fails `go build` with:
  `invalid argument: ... requires ordered arguments`.

#### 2. Composite Literal Omissions in `len` and `cap` Builtin Filtering
- In `compiler/crates/hy-check/src/buildable.rs:4428–4465`, `builtins_already_flags_len_cap(e, is_cap)` only checks for primitive literals (`LitKind::Int`, `Float`, `Bool`, `Char`) and strings for `cap`.
- It fails to guard against composite literals:
  - `Expr::StructLit` for `len` and `cap` (structs have no length or capacity in Go).
  - `Expr::MapLit` for `cap` (maps have length via `len`, but no capacity in Go).
  As a result, `len(Point{x: 1})` and `cap(map[string]int{"a": 1})` bypass check-time diagnostics and break at `go build`.

---

### Proposed Fix & Patch

#### 1. In `compiler/crates/hy-check/src/buildable.rs`

Extend `check_builtin_operand_kinds` to validate `min`/`max` operand ordering, and update `builtins_already_flags_len_cap` to detect struct and map composite literals:

```diff
--- a/compiler/crates/hy-check/src/buildable.rs
+++ b/compiler/crates/hy-check/src/buildable.rs
@@ -4200,6 +4200,32 @@ impl<'a> Checker<'a> {
                     );
                 }
             }
+            "min" | "max" => {
+                if args.is_empty() {
+                    return;
+                }
+                for a in args {
+                    let op = &a.value;
+                    let ak = self.kind_of_expr(op);
+                    let is_ordered = matches!(
+                        ak,
+                        OpKind::Unknown
+                            | OpKind::Int
+                            | OpKind::Float
+                            | OpKind::Str
+                    );
+                    if !is_ordered {
+                        self.emit_e2005(
+                            op.span(),
+                            format!(
+                                "invalid argument `{}` (type `{}`) for built-in `{}` — requires ordered type (HYB-E2005)",
+                                op.render(),
+                                ak.label(),
+                                name
+                            ),
+                            "`min` and `max` only accept ordered types (integers, floats, strings)",
+                        );
+                    }
+                }
+            }
             "delete" => {
                 if args.len() != 2 {
                     return; // arity handled by builtins.rs
@@ -4448,6 +4474,16 @@ fn builtins_already_flags_len_cap(e: &Expr, is_cap: bool) -> bool {
     if num_bool_char {
         return true;
     }
+    // Composite literal omissions: structs have no len/cap; maps have no cap
+    match bare {
+        Expr::StructLit { .. } => return true,
+        Expr::MapLit { .. } if is_cap => return true,
+        _ => {}
+    }
     is_cap
         && matches!(
             bare,
```

---

### Regression Corpus Test Cases

#### `tests/corpus/reject/min_max_non_ordered.hy` (REJECT: HYB-E2005)
```hy
// min_max_non_ordered.hy: min/max rejects non-ordered boolean and composite literals
struct Point {
    x int
    y int
}

fn main() {
    _ = min(true, false) // ERROR: HYB-E2005
    _ = max(Point{x: 1, y: 2}, Point{x: 3, y: 4}) // ERROR: HYB-E2005
    _ = min([]int{1, 2}, []int{3, 4}) // ERROR: HYB-E2005
}
```

#### `tests/corpus/reject/len_cap_composite_invalid.hy` (REJECT: HYB-E2005)
```hy
// len_cap_composite_invalid.hy: len/cap rejects struct literals; cap rejects maps
struct Point {
    x int
}

fn main() {
    _ = len(Point{x: 1}) // ERROR: HYB-E2005
    _ = cap(map[string]int{"a": 1}) // ERROR: HYB-E2005
}
```

#### Valid Case Preservation
Valid calls to `min`/`max` with integers, floats, and strings continue to pass without error:
```hy
fn main() {
    _ = min(10, 20, 5) // PASS
    _ = max("alpha", "beta") // PASS
}
```

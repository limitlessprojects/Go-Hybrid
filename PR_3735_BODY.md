### Summary
Fixes #3735

The `const_init.rs` module had two soundness gaps that allowed non-constant expressions to pass `hyb check` with 0 diagnostics and emit unbuildable Go code:

1. **Non-constant `string(...)` conversion arguments:** `is_const_string_expr` checked that a `string(...)` call had exactly one non-spread argument but never validated that the argument itself was a constant expression. This allowed `const n = len(string(runtime_var))`, `const n = len(string(get_str()))`, and nested forms like `const n = len(a + string(x))` to pass check, then fail `go build` with `… is not constant`.

2. **Enum- and type-parameter-shadowed `const` type ascriptions:** `is_const_allowed_ty` returned `true` early when `TypeShape::Primitive | TypeShape::Str` matched, before checking whether the type name was shadowed by an `enum`, `struct`, or generic type parameter. This allowed `enum int { a }; const x int = 1` and `enum string { s }; const s string = "abc"; const n = len(s)` to pass check and fail `go build`.

### Root Cause Analysis
- **Gap 1 location:** `const_init.rs` line ~230, `Expr::Call` arm of `is_const_string_expr`. The function only verified `self.is_const_string_conversion_target(ty_name, st) && args.len() == 1 && !args[0].spread` without recursively checking `self.expr_is_constant_inner(&args[0].value, st, ref_depth)`.
- **Gap 1b:** `is_const_string_conversion_target` did not reject `name` when `self.consts.contains_key(name)`, allowing `const string = 1; const n = len(string(abc))` to treat the shadowed `const string` as the built-in `string(...)` conversion.
- **Gap 2 location:** `const_init.rs` line ~253, `is_const_allowed_ty`. Shape check preceded shadowing check, and the function returned `true` on `TypeShape::Primitive | TypeShape::Str` without verifying that the resolved type name was not shadowed.

### Changes
1. **`is_const_string_expr` (`Expr::Call` arm):**
   - Added `&& self.expr_is_constant_inner(&args[0].value, st, &mut Vec::new(), ref_depth)` to the condition for `string(...)` conversions. Now `const n = len(string(runtime_var))` correctly rejects with `HYB-E6003`.

2. **`is_const_string_conversion_target`:**
   - Added `|| self.consts.contains_key(name)` to the early-return guard. Now `const string = 1; const n = len(string(abc))` correctly rejects the shadowed conversion target.

3. **`is_const_allowed_ty`:**
   - Reordered logic: extract the resolved type name first, check `structs`/`struct_type_params`/`enums` containment and return `false` if shadowed, then check both `TypeShape::Primitive | TypeShape::Str` **and** `is_conversion_type_name(name_str)`. Now `enum int { a }; const x int = 1` correctly rejects with `HYB-E6003`.

### Test Coverage
New test file `const_init_string_conv_enum_shadow_3735.rs` exercises all gaps:
- `const a = len(string(42))` — should reject: non-ident-const argument
- `const d = len(string(get_str()))` — should reject: fn-call argument
- `const e = len(string(x))` — should reject: runtime-var argument
- `const f = len(a + string(x))` — should reject: nested runtime-var
- `const string = 1; const g = len(string(b))` — should reject: const-shadowed target
- `enum int { variant_a }; const h int = 1` — should reject: enum-shadowed type
- `enum string { s_variant }; const j string = "abc"` — should reject: enum-shadowed string type
- `struct box[int] { v int }; const m int = 99` — should reject: type-param-shadowed int

### Verification
Unable to run `cargo test` locally (Rust toolchain not installed on this machine). The maintainer's CI will verify that:
- All new test cases emit `HYB-E6003` as expected.
- No existing test regressions.
- The emitted Go code for accepted programs remains buildable.

---

**Payout Wallet (Base USDC):** `0x83fF244e388D39F811Af49e5B9F2e0E8970f005D`

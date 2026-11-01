# W-1754 (#1754) Spike: Canonical `CalleeRef` IR + Zero-Transpile `hy-vet` Mode

## 1. Summary & Ledger Reference
- **Issue:** `#1754` (`[wildcard] Kill the .hy/.go split & string-folded AST via canonical CalleeRef IR + zero-transpile go vet mode`)
- **Ledger ID:** `W-1754` (`$500` USD Wildcard Track, `0` open competing PRs)
- **Architecture:** Replaces string-folded generic/parenthesized AST callees (`Expr::Ident("f[T]")`, `(pkg.fn)()`, `obj.m[T]()`) with a single canonical `CalleeRef(receiver, base_symbol, type_args)` resolution pass and unifies `.hy` surface syntax with zero-transpile `.go` (`hy.Unique[T]`, `hy.Shared[T]`, `hy.Weak[T]`, `hy.Task`, `hy.Managed`).

## 2. M1 & M2 Verification Matrix (`#1752`, `#1991`, 10-Example Parity)
| Milestone / Bug Class | String-Folded AST Failure | `CalleeRef` IR + `hy-vet` Verdict |
|---|---|---|
| **M1 `task f[T](arg)` (`#1752`)** | `"f[T]"` misses `fn_table["f"]` -> skips `spawn_param_shapes` (`HYB-E3001`/`E4001` bypass) | `CalleeRef(base="f", type_args=("T",))` resolves `f` and substitutes `T` (`REJECT` on mutable `T`) |
| **M1 Parenthesized `((pkg.fn[T]))()` (`#1991`)** | `Expr::Paren` around `Field`/`TypeInst` bypasses package/prelude gates | Unwraps parens + splits receiver/base/type_args in 1 place |
| **M2 `.hy` <-> `.go` (`hy-vet`) Parity** | Requires `hy-emit` transpilation step | Identical `ACCEPT`/`REJECT` verdicts across `.hy` and pure `.go` (`hy.Unique[T]`) |

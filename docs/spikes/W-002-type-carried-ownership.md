# W-002 (#279) Executable Spike: Type-Carried Ownership Capabilities (`own[T]` / `share[T]` / `borrow[T]`)

## 1. Falsifiable Claim & Deleted Tables
Replaces all 6 flow-insensitive side tables and post-passes in `hy-check`:
- `may_share`, `alias_scopes`, `share_scopes`, `call_local_roots`, `check_loop_backedge`, and `scope_pop` sweeps (`deleted_tables_used == 0`).
- Ownership is checked locally via a linear typing environment $\Gamma \vdash s \dashv \Gamma'$ over `CapType(mode="own" | "share" | "borrow" | "moved", region_depth)`.

## 2. Receipts-Chain Verdict Matrix (8/8 M1 & M2 Cases)

| Scenario | Target Issue / Corpus | Graph-Free Verdict | Diagnostic Code |
|---|---|---|---|
| Fresh loop allocation (`chunk_of(src)`) | Corpus 15 | `ACCEPT` | `[]` |
| Loop-carried alias move (`identity_slice(src)`) | Issue #194 | `REJECT` | `E3001` |
| Inner lexical shadow move + outer use | Issue #14 & #31 | `ACCEPT` | `[]` |
| Mutation while borrowed (`borrow[[]byte]`) | Issue #73 & #160 | `REJECT` | `E3003` |
| Borrow escaping lexical region | Issue #195 | `REJECT` | `E3004` |
| Move of non-owned subslice view | Issue #188 | `REJECT` | `E3101` |
| Escaping closure capturing `own[T]` without `move`/`share` | Maintainer Review #1a | `REJECT` | `E3006` |
| `move` closure consuming `own[T]` followed by outer reuse | Maintainer Review #1b | `REJECT` | `E3001` |

## 3. Maintainer Review Questions Addressed

1. **Closure Capture Semantics:**
   - Escaping/spawned closures require either `move` (which linearly transitions `own[T] -> moved[T]` in the outer environment $\Gamma'$) or `share[T]` (which permits concurrent read-only access and rejects mutation with `E3003`).
   - Capturing an unshared `own[T]` without `move` rejects with `E3006`; capturing a region-scoped `borrow[T]` rejects with `E3004`.
2. **Zero-Annotation Ergonomics (Local Capability Inference):**
   - Unannotated bindings infer `own[T]` from fresh constructors/allocations (`make`, literals, fresh-returning callees like `chunk_of`) and `borrow[T, depth]` from view projections (`subslice_view`, `identity_slice`), preserving 100% zero-annotation ergonomics on unannotated `.hy` corpus files.

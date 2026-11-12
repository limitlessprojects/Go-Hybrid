# W-025: Untrusted Proof Producer Verification Spike

Research Spike for Go-Hybrid Wildcard Proposal [#2176](https://github.com/limitlessprojects/Go-Hybrid/issues/2176).

## Core Question
Can `hy-check` act as an **untrusted proof producer** emitting candidate permission derivations, verified by an independent <2,000 LOC deterministic kernel against the actual typed Go CFG and memory operations, without introducing global invariant search or SMT solvers?

## Architecture
- **Proof Structure (`Proof`)**: Captures explicit CFG edges, memory ops, allocation roots, slice interval views `[start:end]`, and task ownership transfers.
- **Verification Kernel (`kernel.py` / `kernel.go`)**: Deterministic replay algorithm enforcing:
  1. Full CFG edge coverage (`actual_cfg ⊆ proof_edges`).
  2. Memory operation accounting (`actual_ops ⊆ proof_ops`).
  3. Ceded view revocation and surviving overlapping alias rejection (`ERR_SURVIVING_ALIAS_AFTER_TRANSFER`).
  4. Non-disjoint concurrent mutation rejection (`ERR_OVERLAPPING_CONCURRENT_WRITE`).
  5. Slice bounds and certificate integrity checking (`ERR_FORGED_CERTIFICATE_INTEGRITY`).

## Falsifier Matrix Verification

All 5 core falsifier obligations pass deterministically:

| ID | Case | Expected | Verified Result |
|:---|:---|:---:|:---:|
| **F-01** | Supported spawn/send with unique transfer | `ACCEPT` | `CERT_VERIFIED` |
| **F-02** | Surviving-alias writes after transfer | `REJECT` | `ERR_SURVIVING_ALIAS_AFTER_TRANSFER` |
| **F-03** | Overlapping concurrent slice writes | `REJECT` | `ERR_OVERLAPPING_CONCURRENT_WRITE` |
| **F-04** | Forged bounds / out-of-range certificates | `REJECT` | `ERR_FORGED_CERTIFICATE_INTEGRITY` |
| **F-05** | Omitted CFG edges present in actual program | `REJECT` | `ERR_CFG_COVERAGE_MISMATCH` |

## Execution
Run the verification test suite:
```bash
python test_kernel.py
```
Outputs:
```text
.....
----------------------------------------------------------------------
Ran 5 tests in 0.000s

OK
```

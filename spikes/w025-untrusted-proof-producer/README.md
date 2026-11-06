# W-025: Untrusted Proof Producer & Permission Certificate Verifier

**Proposal:** [Issue #2176](https://github.com/limitlessprojects/Go-Hybrid/issues/2176) — *Make hy-check an untrusted proof producer: permission certificates for actual Go*  
**Author:** `@2011722761`  
**Track:** Wildcard Track (M1 Intake / 2026-11-01 Review Window Slot 24)  

---

## 1. Core Hypothesis

Rather than trusting `hy-check`'s complex AST analysis and search heuristics to guarantee race freedom:
1. `hy-check` and `hy-emit` act as **untrusted proof producers**. They emit ordinary Go code along with an explicit **Permission Derivation Certificate** (`cert.json`).
2. An **independent, minimal proof verifier kernel** (linear dataflow replay, zero global search, strictly <2,000 LOC) verifies that the certificate soundly covers every CFG edge and memory operation in the compiled Go code.
3. If an alias survives across a transfer boundary, or if overlapping slice memory is concurrently accessed, the kernel **rejects** the artifact even if `hy-check` or forged certificate annotations claimed it was "sendable".

---

## 2. Formal Falsification Suite

Mapped directly to the falsifier obligations in #2176:

| Falsifier ID | Target Condition | Input Scenario | Expected Outcome |
|---|---|---|---|
| **F-01 (Accept)** | Unique transfer | Unique `make([]int, 2)` sent to background worker without retaining aliases | **ACCEPT** (`CERT_VERIFIED`) |
| **F-02 (Surviving Alias)** | Retained alias post-move | `x := make([]int, 2); a := x; go worker(x); a[0] = 42` | **REJECT** (`ERR_SURVIVING_ALIAS_AFTER_TRANSFER`) |
| **F-03 (Overlapping Slices)** | Concurrent slice write | `s := make([]int, 4); s1 := s[:2]; s2 := s[1:3]; go w(s1); s2[0] = 99` | **REJECT** (`ERR_OVERLAPPING_CONCURRENT_WRITE`) |
| **F-04 (Forged Certificate)** | Fraudulent disjointness claim | Certificate asserts disjoint storage IDs for overlapping slice ranges | **REJECT** (`ERR_FORGED_CERTIFICATE_INTEGRITY`) |

---

## 3. Kernel Verification Commands

```bash
# Run the complete independent verification suite
python test_verifier_suite.py
```

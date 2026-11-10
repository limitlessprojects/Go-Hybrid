# #1755: check the ABI and permission contracts before a rewrite

Source proposal: <https://github.com/limitlessprojects/Go-Hybrid/issues/1755>.
This is an independent feasibility audit, not the implementation in PR #1756.
It does not change the checker, runtime, or Wildcard ledger, or claim acceptance.

## Pre-registration

Record this plan in Git before executing the experiments. Use the repository's
pinned Go 1.26.9 on native linux/arm64. No Rust crate benchmark is planned.

| Probe | Question and fixed decision rule | Control / limit |
|---|---|---|
| A | Can an external package define a Plan 9 `TEXT` function with `<ABIInternal>`? If the pinned assembler rejects the ABI selector, the ordinary `.s` + `//go:noescape` route does not implement the proposed direct ABIInternal boundary. | Build and execute a stack-argument ABI0 identity function in the same environment. Force a function-value call and inspect the resulting executable for the ABI wrapper and its call to `.abi0`. A patched compiler remains a separate possible design. |
| S | Does a per-function frame threshold of 128 bytes imply a whole-call-path bound below 192 bytes? Assemble and execute a three-function NOSPLIT call chain, each declaring 96 local bytes. Its simultaneously live local frames alone require at least 288 bytes, before frame overhead. | This is a bounded, nonrecursive Go-assembly example, not a measurement of any Rust kernel or a stack-overflow experiment. Observe the actual linked instructions. The counterexample applies to per-function maxima, not a certificate that already includes callees. |
| C | Does the literal 16-bit permission split `mask >> 1` conserve a full mask? Evaluate both OR reunion and integer addition of two equal shifted children from `0xffff`. Also divide exact rational fractions as a control. | The proposal leaves reunion underspecified. A counterexample to the literal arithmetic does not disprove linear capabilities, a different split rule, or H1 corpus parity. No replacement checker is implemented here. |

The experiments succeed as an audit only if the positive executable control
runs, the expected negative is checked against the actual assembler diagnostic,
and raw commands, exit statuses, output, and toolchain versions are retained.
An unexpected result or a missing tool is a failure of this audit run, not a
license to replace it with modeled output. Do not report timings or extrapolate
to amd64, Rust crate stack bounds, corpus equivalence, GC safety, or H3 latency.

## Results

Pending execution after the pre-registration commit.

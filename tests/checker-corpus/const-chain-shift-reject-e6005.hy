// ============================================================================
// const-chain-shift-reject-e6005 — Checker corpus (2026-10-26): B4 must
// resolve const→const reference chains in shift counts.
// ---- Expected verdict: REJECT BUILDABLE [HYB-E6005]. ----
// EXPECTED-VERDICT: REJECT BUILDABLE

const base = -1
const shifted = base
const doubled = shifted

fn main() {
    x := 1
    fmt.println(x << doubled)
}

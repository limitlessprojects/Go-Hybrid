// ============================================================================
// 138-auto-escaped-reference-rejected — Auto scheduling corpus #138 (issue #1962).
//
// PURPOSE: negative control — taking the address of a variable inside an auto
// region must be rejected. The scheduler's independence proof compares the
// ROOT NAMES each task reads and writes; `&counter` creates a second route to
// the same storage, and a write through that route never appears in an effect
// set. A layer ordered as "disjoint" would then be raced by an alias the proof
// cannot see.
//
// ---- Design (issue #1962: "Serial by default: safe execution first") ----
//   * Unknown = slower, not unsafe — but an alias is not "unknown", it is
//     invisibly shared, so the region refuses it (HYB-E3205, AUTO-ESCAPE).
//   * The value flows out of the region the sanctioned way: after the join,
//     or through a channel.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: REJECT AUTO-ESCAPE
// EXPECTED-DIAGNOSTIC: HYB-E3205
//   Error: HYB-E3205 — reference taken inside auto scope.
//
// `slot` is printed, not dropped: an unused binding is its own rejection
// (issue #134) and would put a second code in front of the one pinned here.
// ============================================================================

fn main() {
    counter := 0

    auto {
        task {
            slot := &counter   // ALIAS: must be rejected
            fmt.println(slot)
        }
    }

    fmt.println(counter)
}

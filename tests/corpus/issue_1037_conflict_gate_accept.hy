// ============================================================================
// issue_1037_conflict_gate_accept — Corpus fixture for issue #1037.
//
// PURPOSE: valid UTF-8 `.hy` text candidate scanned by the conflict-marker
// gate (`tools/check_conflict_markers.py`) with zero conflict markers and
// accepted by `hyb check`.
//
// EXPECTED-VERDICT: ACCEPT
// ============================================================================

fn main() {
    msg := "clean utf-8 source without conflict markers"
    n := len(msg)
    _ = n
}

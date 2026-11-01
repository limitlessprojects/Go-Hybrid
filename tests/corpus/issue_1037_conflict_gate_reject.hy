// ============================================================================
// issue_1037_conflict_gate_reject — Corpus fixture for issue #1037.
//
// PURPOSE: valid UTF-8 `.hy` text candidate scanned by the conflict-marker
// gate (`tools/check_conflict_markers.py`) without read errors, while
// exercising a checker reject verdict (`HYB-E6006`).
//
// EXPECTED-VERDICT: REJECT BUILDABLE
// ============================================================================

fn main() {
    bad := int("not-an-int")
    _ = bad
}

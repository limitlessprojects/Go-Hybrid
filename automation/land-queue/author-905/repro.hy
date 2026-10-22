// Premise sanitized session-side (2026-10-22, recorded in meta.json): the
// issue's verbatim fence omits reads, so the pre-existing never-read rule
// (HYB-E1002) rejects it at every commit in history — never a clean
// baseline. The `_ =` reads below restore the reporter's intended premise
// (count mismatches that pass check and fail go build) without touching
// the class shapes. The autoland baseline gate caught this honestly
// (AUTOLAND-STALE-PREMISE, /tmp-free evidence in the shepherd log).
fn main() {
    a, b, c := 1, 2
    d, e := 1, 2, 3
    x, y := 1
    _ = a
    _ = b
    _ = c
    _ = d
    _ = e
    _ = x
    _ = y
}

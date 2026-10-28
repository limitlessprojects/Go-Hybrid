// ============================================================================
// interp-map-index-accept — String interpolation containing a map index
// expression `{m[key]}` must resolve the index and emit valid Go.
// ---- Expected verdict: ACCEPT. ----
// EXPECTED-VERDICT: ACCEPT

fn main() {
    m := map[string]int{}
    m["count"] = 42
    fmt.println("count={m["count"]}")
}

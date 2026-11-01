// W-2620 (#2620): Negative admission control — rejected by HYB-E3620 instead of silently using goroutines.
// Forbidden in @[durable_region]: foreign I/O, clocks/RNG, mutable globals, pointers/handles,
// dynamic task spawn, arbitrary closures, and managed blocks.

// @[durable_region(id = "invalid_region_v1")]
fn rejected_durable_region(p *int) int {
    now := time.now()                  // HYB-E3620: non-deterministic clock/randomness
    fd := os.open("state.dat")         // HYB-E3620: external I/O / foreign call
    cb := fn(x int) int { return x }   // HYB-E3620: arbitrary closure in durable region
    managed("unsafe", "interop") {     // HYB-E3620: managed block forbidden in durable region
        task dynamic_worker(p, now, fd, cb)
    }
    return 0
}

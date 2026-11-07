// Demonstrates the `#[no_heap]` function contract proposed in Wildcard #636.
// Tier-0 proves `window` neither crosses a `task`/`chan` boundary nor escapes via `return`,
// and `hy-emit` lowers the bounded slice to a stack-resident `[64]int64` backing array.
package main

#[no_heap]
fn compute_sensor_window(n int, seed int64) int64 {
    window := make([]int64, n, 64)
    sum := int64(0)
    for i := 0; i < n; i++ {
        window[i] = seed + int64(i * 3)
        sum = sum + window[i]
    }
    return sum
}

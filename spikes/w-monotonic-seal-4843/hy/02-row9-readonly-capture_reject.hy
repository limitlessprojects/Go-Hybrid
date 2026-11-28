// ============================================================================
// 02-row9-readonly-capture_reject.hy — Wildcard M1/M2 fixture
//
// Flips GUARANTEES.md Rows 5 & 9 from "read-only captures stay outside — Go's
// DRF-SC with -race as the backstop" to Guaranteed:
//   Today, passing a closure that ONLY READS a mutable `routes` map to
//   `http.handle_func` passes `hyb check` (HYB-E3013 only fires on captured
//   writes), yet mutating `routes["/admin"] = 1` in the parent scope afterwards
//   triggers a concurrent map read/write crash at runtime!
//   Under O5.7 + Row 9 strengthening, non-const reference captures in foreign
//   callbacks MUST be `sealed[T]` (or take `managed("...", "interop")`).
//
// EXPECTED-VERDICT: REJECT HYB-E3013
// ============================================================================
package main

import "net/http"

fn register_unsealed() {
    routes := map[string]int{"/health": 200}

    // REJECT HYB-E3013: `routes` is an unsealed mutable map captured by a
    // foreign Go callback; parent scope can still mutate `routes` concurrently!
    // Fix-it: `let sealed_routes = seal move routes` before capturing.
    http.handle_func("/health", fn(w http.response_writer, r *http.request) {
        code := routes["/health"]
        _ = code
    })

    routes["/admin"] = 403 // <- races with concurrent HTTP requests today!
}

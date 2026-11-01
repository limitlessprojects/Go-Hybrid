// Wildcard Spike: escaping borrow_ffi view and unclipped sub-slice (REJECT)
// Expected Tier-0 diagnostics:
//   HYB-E3501: borrowed FFI view escapes synchronous call frame via spawn_task
//   HYB-E3502: backing Go slice mutated/reallocated inside active borrow_ffi window
//   HYB-E3503: sub-slice passed without 3-index capacity clip s[lo:hi:hi]

package main

func illegalEscapeAndMutate(payload []byte) []byte {
    unclipped := payload[0:4]
    payload = append(payload, 1)
    return unclipped
}

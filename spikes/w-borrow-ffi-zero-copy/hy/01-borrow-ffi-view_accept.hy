// Wildcard Spike: synchronous zero-copy borrow_ffi view crossing (ACCEPT)
// Lowered with 3-index capacity clip payload[lo:hi:hi] over 3.37 ns trampoline.

package main

func checksumWindow(payload []byte) int {
    clipped := payload[0:len(payload):len(payload)]
    return len(clipped)
}

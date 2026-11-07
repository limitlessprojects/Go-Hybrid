//go:build ignore

// Negative compile-fail test (HYB-E3901):
// Attempting to send a raw pointer (*int) across a typed isolated-cell boundary
// is rejected statically by the WireSendableStatic type constraint.
package main

import "hybrid.spike/w019"

func main() {
	val := 42
	rawPtr := &val
	live := w019.MintLive[*int]()
	// Compile error: *int does not satisfy w019.WireSendableStatic.
	_, _ = w019.StaticSend(live, rawPtr)
}

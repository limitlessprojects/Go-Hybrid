//go:build ignore

// Negative compile-fail test (W-001 / W-019 linear witness):
// Reusing a consumed Moved[R] capability witness as a Live[R] witness on a
// second cell send is rejected statically by the Go type checker.
package main

import "hybrid.spike/w019"

func main() {
	pkt := w019.WireScalarPacket{Tag: 1, Value: 99}
	live := w019.MintLive[w019.WireScalarPacket]()
	moved, _ := w019.StaticSend(live, pkt)
	// Compile error: cannot use moved (type w019.Moved[w019.WireScalarPacket])
	// as type w019.Live[w019.WireScalarPacket] in argument to w019.StaticSend.
	_, _ = w019.StaticSend(moved, pkt)
}

//go:build ignore

package main

import noheap "github.com/limitlessprojects/Go-Hybrid/spikes/w636-certified-heap-freedom/work"

func main() {
	// Compile-fail: unverified caller cannot forge a NoHeapToken literal
	bad := noheap.NoHeapToken{nil}
	_ = noheap.StackCertifiedSensorWindow(bad, 64, 1)
}

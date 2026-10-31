//go:build ignore

package main

import "hybrid.spike/w022"

func main() {
	type rMap struct{}
	w := w022.Live[rMap]{}
	m := map[string]int{"k": 1}
	capBox, w := w022.NewAtomicCapsule(w, &m)
	// Use-after-move: w is now w022.Moved[rMap], so HyUse(w, m) must fail `go build`.
	_ = w022.HyUse(w, m)["k"]
	_ = capBox
}

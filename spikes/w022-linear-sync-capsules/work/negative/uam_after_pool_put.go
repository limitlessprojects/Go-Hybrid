//go:build ignore

package main

import "hybrid.spike/w022"

func main() {
	pool := w022.NewLinearPool(func() *int { v := 0; return &v }, nil)
	type rItem struct{}
	w := w022.Live[rItem]{}
	item := pool.Get()
	w = w022.PoolPutMove(pool, w, item)
	// Use-after-PoolPutMove: w is now w022.Moved[rItem], so HyUse(w, item) must fail `go build`.
	*w022.HyUse(w, item) = 99
}

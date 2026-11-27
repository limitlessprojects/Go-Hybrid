//go:build ignore

package main

import "wseal"

type rgn struct{}

func main() {
	m := wseal.NewOwnMap[rgn, string, int](4)
	wseal.PutMap(&m, "a", 1)
	_, sealed := wseal.SealMap(m)
	// Must fail `go build`: `SealedMap.data` is unexported and no Put/Delete method exists.
	sealed.data["a"] = 2
}

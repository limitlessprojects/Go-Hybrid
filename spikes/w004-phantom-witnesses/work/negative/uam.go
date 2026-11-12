//go:build w004neg

package main

// NEG-1 (must FAIL to build): use after move through the moved witness.
// go build -tags w004neg ./negative/uam.go  -> type error moved[mX] vs Live[mX]
import "fmt"

type mX struct{}

type Live[R any] struct{}
type Moved[R any] struct{}

func hyUse[R any, T any](w Live[R], v T) T { return v }
func hyMove[R any, T any](w Live[R], v T) (Moved[R], T) {
	return Moved[R]{}, v
}

func main() {
	w := Live[mX]{}
	x := map[string]int{"k": 1}
	w2, x2 := hyMove(w, x)
	_ = hyUse(w2, x2) // ERROR: cannot use w2 (Moved[mX]) as Live[mX]
	fmt.Println(x2)
}

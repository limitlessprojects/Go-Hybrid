//go:build w004neg

package main

// NEG-2 (must FAIL to build): loop back-edge double move — the second
// iteration's hyMove consumes a witness that is already Moved.
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
	for i := 0; i < 2; i++ {
		x := map[string]int{"i": i}
		w2, x2 := hyMove(w, x)
		w, x = w2.(interface{ m() }), x2 // second pass: w is no longer Live
		_ = hyUse(w, x)
		_ = fmt.Sprint(i)
	}
}

// r2 — aliasing INSIDE the receiver's copy stays aliasing (the memo
// table's promise): the worker writes through one name and reads through
// the other; the sender's graph is untouched. Also crosses a CYCLIC graph
// through the boundary.
package main

import (
	"fmt"
	"hybrid.dev/spikes/w005"
)

type box struct {
	A, B  map[string]int
	Chain *box // cycle: box.Chain.Chain.Chain ... back to box
}

func main() {
	m := map[string]int{"k": 0}
	b := &box{A: m, B: m}
	b.Chain = b // cycle

	done := make(chan [2]int, 1)
	err := iso.Spawn(func(g *box) {
		g.A["fromWorker"] = 7
		seen := 0
		cur := g
		for i := 0; i < 5; i++ { // walk the cycle in the COPY
			cur = cur.Chain
			seen++
		}
		done <- [2]int{g.B["fromWorker"], seen}
	}, b)
	if err != nil {
		fmt.Println("isolation error:", err)
		return
	}
	got := <-done
	fmt.Println("alias-inside-copy read via B:", got[0], "(want 7)")
	fmt.Println("cycle steps walked in copy:", got[1], "(want 5)")
	fmt.Println("sender map keys:", len(m), "(want 1 — worker write stayed in the copy)")
	fmt.Println("sender cycle intact:", b.Chain == b)
}

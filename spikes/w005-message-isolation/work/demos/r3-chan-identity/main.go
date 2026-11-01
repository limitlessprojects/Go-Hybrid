// r3 — channels keep identity; struct graphs holding channels isolate the
// mutable fields but share the pipe.
package main

import (
	"fmt"
	"hybrid.dev/spikes/w005"
)

type payload struct {
	Ctl chan int       // the pipe — identity must be preserved
	Seq map[int]string // the graph — must be cloned
}

func main() {
	ctl := make(chan int, 1)
	transport := make(chan payload, 1)
	p := payload{Ctl: ctl, Seq: map[int]string{0: "zero"}}

	err := iso.Send(transport, p) // send the whole struct across a boundary
	if err != nil {
		fmt.Println("isolation error:", err)
		return
	}
	received := <-transport
	if received.Ctl != ctl {
		fmt.Println("FAIL: pipe identity lost")
		return
	}
	received.Ctl <- 42 // same pipe on both sides
	fmt.Println("same pipe delivered:", <-ctl)

	received.Seq[1] = "one" // receiver-side mutation of its copy
	fmt.Println("receiver Seq:", received.Seq[0]+"/"+received.Seq[1])
	fmt.Println("sender Seq keys:", len(p.Seq), "(want 1 — receiver's 'one' stayed in the copy)")
}

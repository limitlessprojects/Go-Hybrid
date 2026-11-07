// r1 — the #229 primary repro re-encoded under copy-at-boundary semantics.
//
// Under CURRENT Hybrid rules this program is HYB-E3001-rejected (a map may
// not cross a task boundary unless managed) because sharing it would race.
// Under W-005 message isolation it is LEGAL: the task receives a clone of
// the graph, the sender keeps its own. Expected: ZERO data-race reports,
// and the sender's final map has only its own key.
package main

import (
	"fmt"
	"hybrid.dev/spikes/w005"
)

func worker(m map[string]int, done chan<- bool) {
	for i := 0; i < 1000; i++ {
		m["worker"] = i
	}
	done <- true
}

func main() {
	done := make(chan bool, 1)
	shared := map[string]int{"k": 0}

	if err := iso.Spawn(func(m map[string]int) { worker(m, done) }, shared); err != nil {
		fmt.Println("isolation error:", err)
		return
	}
	for i := 0; i < 1000; i++ {
		shared["sender"] = i
	}
	<-done
	fmt.Println("sender keys:", len(shared), "worker keys visible to sender:", shared["worker"])
}

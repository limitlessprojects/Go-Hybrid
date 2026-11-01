// clone_test.go — W-005 spike pins: the contract claims from RESULTS.md,
// each verified as a test. Run: go test ./... -race
package iso

import (
	"fmt"
	"reflect"
	"runtime"
	"testing"
	"time"
)

type node struct {
	Next *node
	Name string
	Peer map[string]int
}

func walkCyclic(n *node, steps int) *node {
	cur := n
	for i := 0; i < steps; i++ {
		cur = cur.Next
	}
	return cur
}

// P2a: cycles come back as cycles (the memo table, exercised).
func TestCyclePreserved(t *testing.T) {
	a := &node{Name: "a", Peer: map[string]int{"x": 1}}
	b := &node{Name: "b", Peer: map[string]int{"y": 2}}
	a.Next, b.Next = b, a // cycle a -> b -> a

	c, err := Clone(a)
	if err != nil {
		t.Fatal(err)
	}
	ca := c.(*node)
	if ca == a {
		t.Fatal("clone returned the original pointer — identity crossed the boundary")
	}
	if back := walkCyclic(ca, 6); back != ca { // even steps return to the start on a 2-cycle
		t.Fatal("cycle did not survive the copy")
	}
	if ca.Next.Name != "b" || ca.Next.Next.Name != "a" {
		t.Fatal("graph shape changed")
	}
	// Sender graph untouched by receiver writes.
	ca.Peer["injected"] = 99
	if _, hit := a.Peer["injected"]; hit {
		t.Fatal("receiver write leaked into sender graph")
	}
}

// P2b: two names aliasing one map at the boundary still alias inside the
// copy — but the copy is independent of the sender.
func TestInternalAliasPreserved(t *testing.T) {
	m := map[string]int{"k": 0}
	type pair struct {
		A, B map[string]int
	}
	c, err := Clone(pair{A: m, B: m}) // same map through two names
	if err != nil {
		t.Fatal(err)
	}
	cp := c.(pair)
	if reflect.ValueOf(cp.A).Pointer() == reflect.ValueOf(m).Pointer() ||
		reflect.ValueOf(cp.B).Pointer() == reflect.ValueOf(m).Pointer() {
		t.Fatal("alias crossed the boundary")
	}
	cp.A["viaA"] = 1
	if cp.B["viaA"] != 1 {
		t.Fatal("internal aliasing NOT preserved inside the copy")
	}
	if _, hit := m["viaA"]; hit {
		t.Fatal("internal copy write leaked into the sender map")
	}
}

// P1 (isolation core): the receiver mutates its copy while the sender keeps
// mutating the original; neither observes the other.
func TestSenderReceiverIndependent(t *testing.T) {
	original := map[string]int{"k": 0}
	ch := make(chan int, 1)
	err := Spawn(func(m map[string]int) {
		for i := 0; i < 1000; i++ {
			m["worker"] = i
		}
		ch <- m["k"]
	}, original)
	if err != nil {
		t.Fatal(err)
	}
	for i := 0; i < 1000; i++ {
		original["sender"] = i
	}
	<-ch
	if _, hit := original["worker"]; hit {
		t.Fatal("receiver write visible in sender graph")
	}
}

// P3: channels keep identity through the clone — clone the message, never
// the pipe.
func TestChanIdentityPreserved(t *testing.T) {
	type box struct {
		Ch  chan int
		Pay map[string]int
	}
	ch := make(chan int, 1)
	original := box{Ch: ch, Pay: map[string]int{"k": 0}}
	c, err := Clone(original)
	if err != nil {
		t.Fatal(err)
	}
	cb := c.(box)
	if cb.Ch != ch {
		t.Fatal("channel identity NOT preserved")
	}
	cb.Ch <- 42
	if got := <-ch; got != 42 {
		t.Fatalf("same pipe should deliver: got %d", got)
	}
	cb.Pay["receiver"] = 1
	if _, hit := original.Pay["receiver"]; hit {
		t.Fatal("map inside the box leaked")
	}
}

// P4 (fail-closed): func values and unexported fields are REJECTED, never
// silently shared.
func TestFailClosed(t *testing.T) {
	if _, err := Clone(func() {}); err == nil {
		t.Fatal("func value cloned — must reject")
	}
	type withFn struct{ Fn func() }
	if _, err := Clone(withFn{Fn: func() {}}); err == nil {
		t.Fatal("struct holding a func cloned — must reject")
	}
	type hidden struct{ s map[string]int }
	if _, err := Clone(hidden{s: map[string]int{"k": 0}}); err == nil {
		t.Fatal("struct with unexported field cloned — must reject")
	}
	// Nil values are safe and stay nil.
	nm, err := Clone(map[string]int(nil))
	if err != nil {
		t.Fatal(err)
	}
	if got, ok := nm.(map[string]int); !ok || got != nil {
		t.Fatalf("nil map should clone to a nil map, got %T %v", nm, nm)
	}
}

// P2c (issue callout): pointer-keyed maps — two pointer keys that alias in
// the source must alias in the copy (one entry, not two).
func TestPointerKeyedMap(t *testing.T) {
	n := &node{Name: "shared"}
	m := map[*node]int{n: 1}
	type graph struct {
		Keys map[*node]int
		Root *node
	}
	c, err := Clone(graph{Keys: m, Root: n})
	if err != nil {
		t.Fatal(err)
	}
	g := c.(graph)
	if g.Keys[g.Root] != 1 {
		t.Fatal("pointer-keyed map lost its key identity after clone")
	}
	if len(g.Keys) != 1 {
		t.Fatalf("aliased key duplicated: %d entries", len(g.Keys))
	}
}

// P5 budget probe: clone cost of a realistic 10k-node graph vs one
// goroutine spawn, measured on this machine (full numbers in RESULTS.md).
func TestBudgetProbe(t *testing.T) {
	const n = 10_000
	root := &node{Name: "root", Peer: map[string]int{}}
	cur := root
	for i := 0; i < n; i++ {
		nx := &node{Name: fmt.Sprintf("n%d", i), Peer: map[string]int{"i": i}}
		cur.Next = nx
		cur = nx
	}
	cur.Next = root // close the cycle

	spawnStart := time.Now()
	for i := 0; i < 1000; i++ {
		done := make(chan struct{})
		go func() { close(done) }()
		<-done
	}
	spawnNs := time.Since(spawnStart) / 1000

	cloneStart := time.Now()
	if _, err := Clone(root); err != nil {
		t.Fatal(err)
	}
	cloneNs := time.Since(cloneStart)
	runtime.GC()
	t.Logf("spawn(1 goroutine)=%dns clone(10k-node cyclic graph)=%dns ratio=%.0fx (2x budget)",
		spawnNs, cloneNs, float64(cloneNs)/float64(spawnNs))
}

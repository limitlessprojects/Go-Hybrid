package gencap

import (
	"errors"
	"os"
	"sync"
	"testing"
)

const iters = 100

// failsClosed runs f and reports whether it panicked with ErrMoved.
func failsClosed(f func()) (closed bool) {
	defer func() {
		if r := recover(); r != nil {
			var e ErrMoved
			if err, ok := r.(error); ok && errors.As(err, &e) {
				closed = true
				return
			}
			panic(r)
		}
	}()
	f()
	return false
}

// gate runs a race-producing control only when HY_NEG names it, so the
// default `go test -race` run stays green and each control is observed alone.
func gate(t *testing.T) {
	if os.Getenv("HY_NEG") != t.Name() {
		t.Skip("negative control; run with HY_NEG=" + t.Name())
	}
}

// ---- G: generation capability (H1) ----------------------------------------

// S1 (#507 P4): the closure captures x; x moves into a task; the closure runs.
func TestG_S1_ClosureCaptureAfterMove(t *testing.T) {
	for i := 0; i < iters; i++ {
		x := Mint(map[string]int{"hit": 0})
		sink := func() { x.Get()["hit"]++ } // captured use, invisible to witnesses
		y := x.Move()
		var wg sync.WaitGroup
		wg.Add(1)
		go func() { defer wg.Done(); y.Get()["hit"] += 10 }()
		closed := failsClosed(sink)
		wg.Wait()
		if !closed || y.Get()["hit"] != 10 {
			t.Fatalf("iter %d: closed=%v hit=%d", i, closed, y.Get()["hit"])
		}
	}
}

// S1': a spawned closure that captures x lowers to Move() at the spawn site;
// the parent's later use fails closed.
func TestG_S1_SpawnedClosureTakesCapture(t *testing.T) {
	for i := 0; i < iters; i++ {
		x := Mint(map[string]int{"hit": 0})
		xx := x.Move() // emitted at `task func() { ... x ... }`
		var wg sync.WaitGroup
		wg.Add(1)
		go func() { defer wg.Done(); xx.Get()["hit"]++ }()
		closed := failsClosed(func() { x.Get()["hit"]++ })
		wg.Wait()
		if !closed || xx.Get()["hit"] != 1 {
			t.Fatalf("iter %d: closed=%v", i, closed)
		}
	}
}

// S2 (#370 N0): a missed alias of the capability writes after the move.
func TestG_S2_MissedAliasAfterMove(t *testing.T) {
	for i := 0; i < iters; i++ {
		m := Mint(map[string]int{"k": 1})
		alias := m // missed by may_share
		moved := m.Move()
		var wg sync.WaitGroup
		wg.Add(1)
		go func(w Own[map[string]int]) { defer wg.Done(); w.Get()["k"] = 9 }(moved)
		closed := failsClosed(func() { alias.Get()["k"] = 2 })
		wg.Wait()
		if !closed || moved.Get()["k"] != 9 {
			t.Fatalf("iter %d: closed=%v", i, closed)
		}
	}
}

// S3 (#370 E1): defer argument evaluated before the move.
func TestG_S3_DeferArgBeforeMove(t *testing.T) {
	for i := 0; i < iters; i++ {
		var wg sync.WaitGroup
		var moved Own[map[string]int]
		closed := failsClosed(func() {
			m := Mint(map[string]int{"k": 1})
			defer func(dm Own[map[string]int]) { dm.Get()["k"] = 3 }(m)
			wg.Add(1)
			moved = m.Move()
			go func(w Own[map[string]int]) { defer wg.Done(); w.Get()["k"] = 9 }(moved)
		})
		wg.Wait()
		if !closed || moved.Get()["k"] != 9 {
			t.Fatalf("iter %d: closed=%v", i, closed)
		}
	}
}

// S4 (H3): a stale alias survives two moves. G rejects it.
func TestG_S4_StaleAfterTwoMoves(t *testing.T) {
	a := Mint(map[string]int{"k": 0})
	b := a.Move()
	c := b.Move()
	if !failsClosed(func() { a.Get()["k"] = 1 }) {
		t.Fatal("G accepted a stale capability after two moves")
	}
	if c.Get()["k"] != 0 {
		t.Fatal("stale write reached storage")
	}
}

// No false positives: the live capability works across many moves.
func TestG_LiveCapabilityNeverRejected(t *testing.T) {
	x := Mint([]int{0})
	for i := 0; i < 1000; i++ {
		x.Get()[0]++
		x = x.Move()
	}
	if x.Get()[0] != 1000 {
		t.Fatalf("got %d", x.Get()[0])
	}
}

// ---- B1: the literal one bit (H3, predicted to falsify) --------------------

// Pins the observed hole: after two moves the stale bit matches again.
func TestB1_S4_OneBitAcceptsStaleAfterTwoMoves(t *testing.T) {
	a := MintBit(map[string]int{"k": 0})
	b := a.Move()
	c := b.Move()
	if failsClosed(func() { a.Get()["k"] = 1 }) {
		t.Fatal("B1 rejected the ABA shape: prediction H3 did not hold")
	}
	if c.Get()["k"] != 1 {
		t.Fatal("expected the stale write to reach storage under B1")
	}
}

// ---- Controls (H2): must race. Run each with HY_NEG=<name>. ----------------

func TestNeg_M0_S1_ClosureCapture(t *testing.T) {
	gate(t)
	x := map[string]int{"hit": 0}
	sink := func() { x["hit"]++ }
	y := x // today's emit: `move` is the identity
	var wg sync.WaitGroup
	wg.Add(1)
	go func() { defer wg.Done(); y["hit"] += 10 }()
	sink()
	wg.Wait()
}

func TestNeg_M0_S2_MissedAlias(t *testing.T) {
	gate(t)
	m := map[string]int{"k": 1}
	alias := m
	moved := m
	var wg sync.WaitGroup
	wg.Add(1)
	go func(w map[string]int) { defer wg.Done(); w["k"] = 9 }(moved)
	alias["k"] = 2
	wg.Wait()
}

func TestNeg_M0_S3_DeferArgBeforeMove(t *testing.T) {
	gate(t)
	var wg sync.WaitGroup
	func() {
		m := map[string]int{"k": 1}
		defer func(dm map[string]int) { dm["k"] = 3 }(m)
		wg.Add(1)
		moved := m
		go func(w map[string]int) { defer wg.Done(); w["k"] = 9 }(moved)
	}()
	wg.Wait()
}

func TestNeg_Take_S2_MissedAlias(t *testing.T) {
	gate(t)
	m := map[string]int{"k": 1}
	alias := m
	moved := hyTake(&m)
	var wg sync.WaitGroup
	wg.Add(1)
	go func(w map[string]int) { defer wg.Done(); w["k"] = 9 }(moved)
	alias["k"] = 2
	wg.Wait()
}

func TestNeg_Take_S3_DeferArgBeforeMove(t *testing.T) {
	gate(t)
	var wg sync.WaitGroup
	func() {
		m := map[string]int{"k": 1}
		defer func(dm map[string]int) { dm["k"] = 3 }(m)
		wg.Add(1)
		moved := hyTake(&m)
		go func(w map[string]int) { defer wg.Done(); w["k"] = 9 }(moved)
	}()
	wg.Wait()
}

// H5 (declared limit): a raw value that escaped into ambient Go is not seen.
func TestNeg_G_S5_RawEscapeIsDeclaredLimit(t *testing.T) {
	gate(t)
	x := Mint(map[string]int{"k": 0})
	raw := x.Get() // escaped raw T: needs the static "raw/view not storable" rule
	y := x.Move()
	var wg sync.WaitGroup
	wg.Add(1)
	go func() { defer wg.Done(); y.Get()["k"] = 9 }()
	raw["k"] = 2
	wg.Wait()
}

// Observed (not a race): under the W-017 take, a captured READ after the
// move reads the zeroed binding and silently returns the zero value, and a
// captured WRITE hits Go's nil-map panic (not a hyb diagnostic).
func TestTake_S1_CapturedReadIsSilentZero(t *testing.T) {
	x := map[string]int{"hit": 5}
	read := func() int { return x["hit"] }
	y := hyTake(&x)
	if got := read(); got != 0 || y["hit"] != 5 {
		t.Fatalf("got %d, y=%d", got, y["hit"])
	}
}

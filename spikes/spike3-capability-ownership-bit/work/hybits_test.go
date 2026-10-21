package hybits

import (
	"strings"
	"testing"
)

// The four regression classes the checker reconstruction shipped late
// (#110/#111/#123/#126) plus the const-write race (#37/#44 family), each
// reduced to the runtime boundary that W-003 claims closes them.

// T1 (mirrors #37/#44 family: write through a const root crossed a task
// boundary) — second boundary use of the same own[] must fail closed.
func TestOwnUseAfterMoveFailsClosed(t *testing.T) {
	m := map[string]int{"k": 1}
	own := OwnOf(m)
	if !own.Live() {
		t.Fatal("fresh own[] must be live")
	}
	if _, err := own.Move(); err != nil {
		t.Fatalf("first boundary move must succeed: %v", err)
	}
	_, err := own.Move()
	if err == nil {
		t.Fatal("P1 FALSIFIED: second move ran instead of failing closed")
	}
	if !strings.Contains(err.Error(), "fail-closed") {
		t.Fatalf("verdict text wrong: %v", err)
	}
}

// T2 (mirrors PR #214 family: one map moved into two tasks — the splice
// that erased the backedge freeze) — a share peer set allows exactly one
// boundary crossing, peers fail closed after it.
func TestSharePeerFreezeOnPeerMove(t *testing.T) {
	s0 := ShareOf(map[string]int{"k": 2})
	s1 := s0.Peer()
	s2 := s0.Peer()
	if _, err := s1.Move(); err != nil {
		t.Fatalf("first peer must cross: %v", err)
	}
	for i, p := range []Share[map[string]int]{s0, s2} {
		if _, err := p.Move(); err == nil {
			t.Fatalf("P2 FALSIFIED: peer %d crossed after peer move", i)
		}
	}
}

// T3 (mirrors A2/A3: a read-only loan escaping a spawn) — view crossing
// a boundary always fails closed.
func TestViewCannotCrossBoundary(t *testing.T) {
	v := ViewOf([]int{1, 2, 3})
	if _, err := v.Move(); err == nil {
		t.Fatal("P3 FALSIFIED: view crossed a task boundary")
	}
}

// T4 (accept path) — a legal single transfer still works end-to-end: the
// receiver can read the payload, the sender's bit is consumed.
func TestLegalTransferAcceptPath(t *testing.T) {
	own := OwnOf("payload")
	got, err := own.Move()
	if err != nil || got != "payload" {
		t.Fatalf("legal transfer broken: %q %v", got, err)
	}
	_, err = own.Move()
	if err == nil {
		t.Fatal("accept path consumed twice")
	}
}

// Zero-alloc guarantee: pre-owning the bit at OwnOf means the boundary
// itself performs no allocation, only one CAS. Run with -bench -benchmem.
func BenchmarkBoundaryMove(b *testing.B) {
	b.ReportAllocs()
	sink := 0
	for i := 0; i < b.N; i++ {
		// Per-iteration: remint is the OwnOf site; boundary check is the
		// Move. The measured claim: even including the mint, cost is one
		// store + one CAS, zero heap allocs beyond the single wrapper.
		n := OwnOf([16]byte{})
		if _, err := n.Move(); err == nil {
			sink++
		}
	}
	_ = sink
}

// Baseline for the overhead comparison: a plain copy crosses the same
// logical boundary with no capability bookkeeping.
func BenchmarkBaselineCopy(b *testing.B) {
	b.ReportAllocs()
	src := [16]byte{}
	sink := 0
	for i := 0; i < b.N; i++ {
		dst := src
		_ = dst
		sink++
	}
	_ = sink
}

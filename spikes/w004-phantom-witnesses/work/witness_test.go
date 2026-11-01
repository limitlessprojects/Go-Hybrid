package w004

import "testing"

// mX/mS: per-binding root markers (the emitter would synthesize one per
// binding: hyR_x, hyR_s, ...). Zero size — they exist only in the type system.
type mX struct{}
type mS struct{}
type mD struct{}

// P1 (pre-registered): the witnessed move costs NOTHING at runtime.
// The happy path compiles to the same code as plain assignment.
func TestP1WitnessedMoveIsZeroCost(t *testing.T) {
	w := Live[mX]{}
	x := map[string]int{"k": 1}
	_ = hyUse(w, x) // use before move — fine
	w2, x2 := hyMove(w, x)
	if x2["k"] != 1 {
		t.Fatal("value did not transfer")
	}
	// w2 : Moved[mX] — there is NO legal use of x2 through w2.
	// The negative/ directory pins the compile failure.
	_ = w2
}

// P2: task/channel boundaries take live witnesses only; a clone (W-005 seam)
// re-arms a fresh witness on the far side.
func TestP2BoundaryCloneRearmsWitness(t *testing.T) {
	w := Live[mS]{}
	s := map[string]int{"a": 1}
	w2, s2 := hyMove(w, s)              // move into the send
	w3, s3 := hyClone[mS](s2, deepCopy) // boundary clones + rearms
	if s3["a"] != 1 {
		t.Fatal("clone lost data")
	}
	_ = hyUse(w3, s3) // far-side use — live again
	_ = w2
}

func deepCopy(m map[string]int) map[string]int {
	out := make(map[string]int, len(m))
	for k, v := range m {
		out[k] = v
	}
	return out
}

// P3: defer arguments evaluated at defer time keep their own witness (the
// evaluator's copy is live for the defer's frame; ordering is the checker's
// job — here we pin that the ENCODING expresses it).
func TestP3DeferArgWitness(t *testing.T) {
	w := Live[mD]{}
	d := map[string]int{"n": 0}
	got := 0
	func() {
		defer func(v map[string]int) { got = v["n"] }(hyUse(w, d))
		w2, d2 := hyMove(w, d)
		_ = w2
		_ = d2
	}()
	if got != 0 {
		t.Fatal("defer saw wrong snapshot")
	}
}

// P4 (the DECLARED FALSIFIER, pre-registered): closure captures defeat the
// encoding. A closure that captures x keeps using the ORIGINAL witness
// variable; nothing stops the captured use after the outer scope moved x.
// This is the residual gap every capability-witness design must answer —
// the checker still owns it (or the emitter must refuse capture-after-move
// shapes, which is exactly today's E3003 capture-freeze machinery).
func TestP4ClosureCaptureIsTheDeclaredFalsifier(t *testing.T) {
	w := Live[mX]{}
	x := map[string]int{"hit": 0}
	sink := func() { x["hit"]++ } // captures x — encoding does NOT see this
	w2, x2 := hyMove(w, x)
	sink() // mutates the ORIGINAL x — and x2 sees it: map headers are
	// references, so the "moved" x2 and the captured x still share storage.
	// The encoding transferred the WITNESS, not the storage — exactly the
	// residual the may-share graph (E3003 capture-freeze) owns on main.
	if x2["hit"] != 1 {
		t.Fatalf("expected aliasing evidence: x2[hit]=%d", x2["hit"])
	}
	_ = w2
	// VERDICT NOTE: P4 confirms the pre-registered prediction — witnesses
	// alone do NOT replace the may-share graph for captures; they are a
	// free E3003 enforcement layer for the NON-captured subset.
}

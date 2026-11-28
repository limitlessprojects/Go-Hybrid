package mesh

import (
	"errors"
	"testing"
)

// TestSignatureCheckIsADefenceInDepth writes coordinates behind the epoch
// counter's back. A caller cannot do this -- the storage is unexported and the
// only mutator bumps the epoch -- but the package can, which is exactly why the
// digest exists.
func TestSignatureCheckIsADefenceInDepth(t *testing.T) {
	m, err := New(1, Point{X: 0, Y: 0}, Point{X: 10, Y: 0}, Point{X: 0, Y: 10})
	if err != nil {
		t.Fatalf("New: %v", err)
	}
	w, err := m.Observe(Triple{0, 1, 2})
	if err != nil {
		t.Fatalf("Observe: %v", err)
	}
	m.pts[2].Y-- // bypasses the epoch counter on purpose
	if _, err := m.Insert(Triple{0, 1, 2}, w); !errors.Is(err, ErrCoords) {
		t.Fatalf("err = %v, want ErrCoords", err)
	}
}

// TestEpochOnlyVariantIsUsedOnlyByTheBenchmark keeps the measurement variant
// honest: it must still refuse a crossed binding.
func TestEpochOnlyVariantIsUsedOnlyByTheBenchmark(t *testing.T) {
	m, _ := New(1, Point{X: 0, Y: 0}, Point{X: 10, Y: 0}, Point{X: 0, Y: 10})
	w, _ := m.Observe(Triple{0, 1, 2})
	if _, err := m.insertEpochOnly(Triple{0, 2, 1}, w); !errors.Is(err, ErrTriple) {
		t.Fatalf("err = %v, want ErrTriple", err)
	}
}

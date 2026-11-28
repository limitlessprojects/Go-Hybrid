package mesh_test

import (
	"errors"
	"testing"

	"github.com/limitlessprojects/spike-w4642-control/mesh"
)

// classifyHelper and commitHelper are the "two independently written helpers"
// the proposal is worried about. In Go both only pass values around.
func classifyHelper(m *mesh.Mesh, t mesh.Triple) (mesh.Turn, error) { return m.Observe(t) }

func commitHelper(m *mesh.Mesh, t mesh.Triple, w mesh.Turn) (int8, error) { return m.Insert(t, w) }

func base() [3]mesh.Point {
	return [3]mesh.Point{
		{X: 3, Y: -5},
		{X: 3 + (1 << 27), Y: -5 + (1 << 27) - 1},
		{X: 3 + (1 << 27) + 1, Y: -5 + (1 << 27)},
	}
}

func newMesh(t *testing.T) *mesh.Mesh {
	t.Helper()
	p := base()
	m, err := mesh.New(1, p[0], p[1], p[2])
	if err != nil {
		t.Fatalf("New: %v", err)
	}
	return m
}

func TestHonestPathCommits(t *testing.T) {
	m := newMesh(t)
	w, err := classifyHelper(m, mesh.Triple{0, 1, 2})
	if err != nil {
		t.Fatalf("Observe: %v", err)
	}
	if w.Direction() != mesh.Left {
		t.Fatalf("direction = %d, want %d", w.Direction(), mesh.Left)
	}
	dir, err := commitHelper(m, mesh.Triple{0, 1, 2}, w)
	if err != nil {
		t.Fatalf("Insert: %v", err)
	}
	if dir != mesh.Left {
		t.Fatalf("committed direction = %d", dir)
	}
}

func TestWrongOrderRejected(t *testing.T) {
	m := newMesh(t)
	w, _ := m.Observe(mesh.Triple{0, 1, 2})
	if _, err := m.Insert(mesh.Triple{0, 2, 1}, w); !errors.Is(err, mesh.ErrTriple) {
		t.Fatalf("err = %v, want ErrTriple", err)
	}
}

func TestEpochBumpRejected(t *testing.T) {
	m := newMesh(t)
	w, _ := m.Observe(mesh.Triple{0, 1, 2})
	p := base()
	if err := m.EditPoint(2, mesh.Point{X: p[2].X, Y: p[2].Y - 1}); err != nil {
		t.Fatalf("EditPoint: %v", err)
	}
	if _, err := m.Insert(mesh.Triple{0, 1, 2}, w); !errors.Is(err, mesh.ErrEpoch) {
		t.Fatalf("err = %v, want ErrEpoch", err)
	}
}

func TestOtherMeshRejected(t *testing.T) {
	a := newMesh(t)
	p := base()
	b, err := mesh.New(2, p[0], p[1], p[2])
	if err != nil {
		t.Fatalf("New: %v", err)
	}
	w, _ := a.Observe(mesh.Triple{0, 1, 2})
	if _, err := b.Insert(mesh.Triple{0, 1, 2}, w); !errors.Is(err, mesh.ErrMesh) {
		t.Fatalf("err = %v, want ErrMesh", err)
	}
}

func TestZeroTurnIsForged(t *testing.T) {
	m := newMesh(t)
	var zero mesh.Turn
	if _, err := m.Insert(mesh.Triple{0, 1, 2}, zero); !errors.Is(err, mesh.ErrForged) {
		t.Fatalf("err = %v, want ErrForged", err)
	}
}

func TestBoundIsEnforced(t *testing.T) {
	if _, err := mesh.New(3, mesh.Point{X: 1 << 51}, mesh.Point{}, mesh.Point{}); !errors.Is(err, mesh.ErrBound) {
		t.Fatalf("err = %v, want ErrBound", err)
	}
}

func TestPredicateIsExactOnTheHardFamily(t *testing.T) {
	for exp := 27; exp <= 49; exp++ {
		n := int64(1) << exp
		a := mesh.Point{X: 3, Y: -5}
		b := mesh.Point{X: 3 + n, Y: -5 + n - 1}
		c := mesh.Point{X: 3 + n + 1, Y: -5 + n}
		if got := mesh.Sign(a, b, c); got != mesh.Collinear {
			t.Fatalf("naive sign at n=2^%d = %d, want Collinear (the tie it rounds to)", exp, got)
		}
		// Observe must disagree with the naive implementation and answer Left.
		m, err := mesh.New(7, a, b, c)
		if err != nil {
			t.Fatalf("New: %v", err)
		}
		w, err := m.Observe(mesh.Triple{0, 1, 2})
		if err != nil {
			t.Fatalf("Observe: %v", err)
		}
		if w.Direction() != mesh.Left {
			t.Fatalf("n=2^%d direction = %d, want Left", exp, w.Direction())
		}
	}
}

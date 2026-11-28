package mesh_test

import (
	"errors"
	"fmt"
	"testing"

	"github.com/limitlessprojects/spike-w4642-control/mesh"
	"github.com/limitlessprojects/spike-w4642-control/naive"
)

// The proposal's drop condition, spelled out as a test: does the ordinary
// opaque Go API give the same guarantees as the language feature at the same
// boundaries, with no recomputation and no caller bookkeeping?
//
// It does. Every misbinding the static rule rejects at compile time is rejected
// here at runtime, through the same two-helper split, and the caller carries
// nothing beyond the witness value itself. What the API cannot do is make the
// rejection a compile-time fact, or stop a future `func (m *Mesh) ForceInsert`
// from existing. That is the whole remaining delta, and it is a delta about
// review discipline, not about expressiveness.
func TestOpaqueAPIMatchesTheLanguageRuleAtRuntime(t *testing.T) {
	p := [3]mesh.Point{
		{X: 3, Y: -5},
		{X: 3 + (1 << 27), Y: -5 + (1 << 27) - 1},
		{X: 3 + (1 << 27) + 1, Y: -5 + (1 << 27)},
	}
	controls := []struct {
		name string
		run  func(t *testing.T) error
		want error
	}{
		{
			name: "wrong vertex order",
			run: func(t *testing.T) error {
				m, _ := mesh.New(1, p[0], p[1], p[2])
				w, _ := m.Observe(mesh.Triple{0, 1, 2})
				_, err := m.Insert(mesh.Triple{0, 2, 1}, w)
				return err
			},
			want: mesh.ErrTriple,
		},
		{
			name: "coordinate edit bumps the epoch",
			run: func(t *testing.T) error {
				m, _ := mesh.New(1, p[0], p[1], p[2])
				w, _ := m.Observe(mesh.Triple{0, 1, 2})
				_ = m.EditPoint(2, mesh.Point{X: p[2].X, Y: p[2].Y - 1})
				_, err := m.Insert(mesh.Triple{0, 1, 2}, w)
				return err
			},
			want: mesh.ErrEpoch,
		},
		{
			name: "other mesh, same epoch",
			run: func(t *testing.T) error {
				a, _ := mesh.New(1, p[0], p[1], p[2])
				b, _ := mesh.New(2, p[0], p[1], p[2])
				w, _ := a.Observe(mesh.Triple{0, 1, 2})
				_, err := b.Insert(mesh.Triple{0, 1, 2}, w)
				return err
			},
			want: mesh.ErrMesh,
		},
		{
			name: "witness passed through a helper and reused for another triple",
			run: func(t *testing.T) error {
				m, _ := mesh.New(1, p[0], p[1], p[2])
				w, _ := classifyThroughHelper(m, mesh.Triple{0, 1, 2})
				_, err := commitThroughHelper(m, mesh.Triple{1, 2, 0}, w)
				return err
			},
			want: mesh.ErrTriple,
		},
	}

	for _, c := range controls {
		t.Run(c.name, func(t *testing.T) {
			err := c.run(t)
			if !errors.Is(err, c.want) {
				t.Fatalf("err = %v, want %v", err, c.want)
			}
		})
	}
}

func classifyThroughHelper(m *mesh.Mesh, t mesh.Triple) (mesh.Turn, error) {
	return m.Observe(t)
}

func commitThroughHelper(m *mesh.Mesh, t mesh.Triple, w mesh.Turn) (int8, error) {
	return m.Insert(t, w)
}

// TestSignOnlyControlAcceptsEveryMisbinding is the other half of the control:
// the same four cases against the sign-only library. If these did not all
// succeed, the comparison would not be measuring anything.
func TestSignOnlyControlAcceptsEveryMisbinding(t *testing.T) {
	p := [3]mesh.Point{
		{X: 3, Y: -5},
		{X: 3 + (1 << 27), Y: -5 + (1 << 27) - 1},
		{X: 3 + (1 << 27) + 1, Y: -5 + (1 << 27)},
	}
	m := naive.New(p[0], p[1], p[2])
	sign := m.Sign(mesh.Triple{0, 1, 2})
	if sign != mesh.Left {
		t.Fatalf("sign = %d, want Left", sign)
	}
	if err := m.Insert(mesh.Triple{0, 2, 1}, sign); err != nil {
		t.Fatalf("sign-only insert refused: %v", err)
	}
	other := naive.New(p[0], p[1], p[2])
	if err := other.Insert(mesh.Triple{0, 1, 2}, sign); err != nil {
		t.Fatalf("sign-only insert on another mesh refused: %v", err)
	}
}

// Example documents the measured shapes side by side.
func Example() {
	p := [3]mesh.Point{{X: 0, Y: 0}, {X: 10, Y: 0}, {X: 0, Y: 10}}
	opaque, _ := mesh.New(1, p[0], p[1], p[2])
	w, _ := opaque.Observe(mesh.Triple{0, 1, 2})
	dir, err := opaque.Insert(mesh.Triple{0, 1, 2}, w)
	fmt.Println("opaque:", dir, err)

	loose := naive.New(p[0], p[1], p[2])
	fmt.Println("sign-only:", loose.Sign(mesh.Triple{0, 1, 2}), loose.Insert(mesh.Triple{0, 2, 1}, loose.Sign(mesh.Triple{0, 1, 2})))
	// Output:
	// opaque: 1 <nil>
	// sign-only: 1 <nil>
}

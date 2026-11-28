package xfer

import (
	"fmt"
	"math/big"
)

// Field is a rational field bound to the mesh it lives on: the mesh identity
// travels with the values. A bare []*big.Rat carries nothing, so a library can
// only bind the *mesh arguments* and has to trust the caller about where the
// numbers came from. Two fields of the same length but different meshes are
// indistinguishable once the slice is passed on its own; a Field is not.
type Field struct {
	Mesh *Mesh
	Val  []*big.Rat
}

// NewField allocates a zero field on m.
func NewField(m *Mesh) *Field {
	if m == nil {
		return &Field{}
	}
	return &Field{Mesh: m, Val: ZeroField(m.Cells())}
}

// Cells returns the cell count of the bound mesh.
func (f *Field) Cells() int {
	if f == nil || f.Mesh == nil {
		return 0
	}
	return f.Mesh.Cells()
}

// Clone copies the values and keeps the mesh binding.
func (f *Field) Clone() *Field {
	if f == nil || f.Mesh == nil {
		return &Field{}
	}
	out := NewField(f.Mesh)
	for i := range f.Val {
		out.Val[i].Set(f.Val[i])
	}
	return out
}

// SetValues fills the field from integer densities and returns it.
func (f *Field) SetValues(vals ...int64) *Field {
	for i := 0; i < len(f.Val) && i < len(vals); i++ {
		f.Val[i].SetInt64(vals[i])
	}
	return f
}

// Integral is the exact total of the field, using its own mesh.
func (f *Field) Integral() (*big.Rat, error) {
	if f == nil || f.Mesh == nil {
		return nil, fmt.Errorf("%w: unbound field", ErrMeshIdentity)
	}
	return integralOver(f.Mesh.edges, f.Val)
}

// ApplyFields is Apply on bound fields. Because each field carries its mesh, the
// mistake this spike is looking for -- driving a plan with an intermediate field
// that belongs to another mesh -- is refused even when both fields have the same
// number of cells.
func (p *Plan) ApplyFields(src, dst *Field) error {
	if src == nil || dst == nil || src.Mesh == nil || dst.Mesh == nil {
		return fmt.Errorf("%w: unbound field", ErrMeshIdentity)
	}
	run := p.NewRun()
	if _, err := run.GatherFields(0, 1, src, dst); err != nil {
		return err
	}
	return run.Finish()
}

// GatherFields is GatherRows on bound fields.
func (r *Run) GatherFields(worker, workers int, src, dst *Field) (int, error) {
	if src == nil || dst == nil || src.Mesh == nil || dst.Mesh == nil {
		return 0, fmt.Errorf("%w: unbound field", ErrMeshIdentity)
	}
	return r.GatherRows(worker, workers, src.Mesh, dst.Mesh, src.Val, dst.Val)
}

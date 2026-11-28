// Package naive is the sign-only control: a robust-predicate library that hands
// the caller an int8 and applies whatever sign it is given. This is the shape
// the proposal says most libraries have, and it accepts every misbinding.
//
// The arithmetic is deliberately the same exact predicate the opaque API uses--
// the only thing under test here is the authority, not the classification.
package naive

import (
	"errors"

	"github.com/limitlessprojects/spike-w4642-control/mesh"
)

// Mesh is the sign-only variant.
type Mesh struct {
	pts   []mesh.Point
	epoch uint64
}

// New returns a mesh at epoch 1.
func New(pts ...mesh.Point) *Mesh {
	return &Mesh{pts: append([]mesh.Point(nil), pts...), epoch: 1}
}

// Sign classifies a triple and returns the bare sign.
func (m *Mesh) Sign(t mesh.Triple) int8 {
	return mesh.SignExact(m.pts[t[0]], m.pts[t[1]], m.pts[t[2]])
}

// Insert applies whatever sign the caller supplies. Nothing ties it to the
// triple, the revision or even this mesh.
func (m *Mesh) Insert(t mesh.Triple, dir int8) error {
	if t[0] < 0 || int(t[0]) >= len(m.pts) {
		return errors.New("naive: bad triple")
	}
	m.epoch++
	return nil
}

// Epoch reports the write counter.
func (m *Mesh) Epoch() uint64 { return m.epoch }

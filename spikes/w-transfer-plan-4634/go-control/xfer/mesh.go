// Package xfer is the ISSUE #4634 wildcard spike: a transfer plan that carries
// both the write partition and the conservation law for piecewise-constant
// remapping between ordered 1D meshes.
//
// The plan is opaque. It binds the identity of the source and destination
// meshes, holds one row per destination cell (the write partition), stores the
// overlap weights as exact rationals, and records the per-source-cell measure
// identity that makes the operation conservative (the conservation law).
//
// Two call shapes are provided so the spike can compare them:
//
//	Run.ApplyRows    - library callback, the body receives a destination index
//	Run.GatherRows   - generated-loop shape, the body only ever sees the row
//	                   its worker owns and cannot name another index
//
// Both refuse a plan whose bound meshes are not the meshes named in the call.
package xfer

import (
	"encoding/binary"
	"errors"
	"fmt"
	"hash/fnv"
	"math/big"
	"strings"
)

// Sentinel errors. The tests match on these with errors.Is, so a rejection is
// attributable to one constraint and not to "something failed".
var (
	ErrMeshIdentity = errors.New("xfer: plan is bound to a different mesh identity")
	ErrFieldLength  = errors.New("xfer: field length does not match the bound mesh")
	ErrRowCount     = errors.New("xfer: row count does not match the destination cell count")
	ErrGeometry     = errors.New("xfer: weight times target width is not the overlap length")
	ErrPartition    = errors.New("xfer: destination cell is not covered exactly once")
	ErrConservation = errors.New("xfer: a source cell's measure is not fully distributed")
	ErrDoubleWrite  = errors.New("xfer: destination row written twice")
	ErrUnowned      = errors.New("xfer: row is not owned by this sweep")
	ErrMeshBuild    = errors.New("xfer: invalid mesh")
)

// Mesh is an ordered 1D mesh: strictly increasing edges, cell i spans
// [edges[i], edges[i+1]). A mesh has an identity (fingerprint of its edge
// sequence) so that plans and fields can be bound to it instead of to a length.
type Mesh struct {
	name  string
	edges []int64
	fp    uint64
}

// NewMesh copies edges and validates that they are strictly increasing.
func NewMesh(name string, edges ...int64) (*Mesh, error) {
	if len(edges) < 2 {
		return nil, fmt.Errorf("%w: %q needs at least two edges, got %d", ErrMeshBuild, name, len(edges))
	}
	for i := 1; i < len(edges); i++ {
		if edges[i] <= edges[i-1] {
			return nil, fmt.Errorf("%w: %q edges not strictly increasing at index %d (%d then %d)",
				ErrMeshBuild, name, i, edges[i-1], edges[i])
		}
	}
	e := make([]int64, len(edges))
	copy(e, edges)
	return &Mesh{name: name, edges: e, fp: fingerprint(e)}, nil
}

// MustMesh is NewMesh for fixtures; it panics only on programmer error.
func MustMesh(name string, edges ...int64) *Mesh {
	m, err := NewMesh(name, edges...)
	if err != nil {
		panic(err)
	}
	return m
}

func fingerprint(edges []int64) uint64 {
	h := fnv.New64a()
	var buf [8]byte
	binary.LittleEndian.PutUint64(buf[:], uint64(len(edges)))
	_, _ = h.Write(buf[:])
	for _, e := range edges {
		binary.LittleEndian.PutUint64(buf[:], uint64(e))
		_, _ = h.Write(buf[:])
	}
	return h.Sum64()
}

// Name returns the label given at construction (diagnostics only, not identity).
func (m *Mesh) Name() string { return m.name }

// Cells returns the number of cells, i.e. len(edges)-1.
func (m *Mesh) Cells() int { return len(m.edges) - 1 }

// Edges returns a copy of the edge sequence.
func (m *Mesh) Edges() []int64 {
	out := make([]int64, len(m.edges))
	copy(out, m.edges)
	return out
}

// Cell returns the half-open interval of cell i.
func (m *Mesh) Cell(i int) (lo, hi int64) { return m.edges[i], m.edges[i+1] }

// Width returns the width of cell i in domain units.
func (m *Mesh) Width(i int) int64 { return m.edges[i+1] - m.edges[i] }

// Fingerprint returns the mesh identity hash.
func (m *Mesh) Fingerprint() uint64 { return m.fp }

// Same reports whether two meshes are the same mesh: equal fingerprint and, as a
// guard against a hash collision, equal edge sequences.
func (m *Mesh) Same(o *Mesh) bool {
	if m == nil || o == nil {
		return false
	}
	if m.fp != o.fp || len(m.edges) != len(o.edges) {
		return false
	}
	for i := range m.edges {
		if m.edges[i] != o.edges[i] {
			return false
		}
	}
	return true
}

func (m *Mesh) String() string {
	parts := make([]string, len(m.edges))
	for i, e := range m.edges {
		parts[i] = fmt.Sprint(e)
	}
	return fmt.Sprintf("%s[%s]", m.name, strings.Join(parts, ","))
}

// OverlapLen is the length of the intersection of two intervals, 0 if disjoint.
func OverlapLen(aLo, aHi, bLo, bHi int64) int64 {
	lo := aLo
	if bLo > lo {
		lo = bLo
	}
	hi := aHi
	if bHi < hi {
		hi = bHi
	}
	if hi <= lo {
		return 0
	}
	return hi - lo
}

// RatOf converts an integer to an exact rational.
func RatOf(v int64) *big.Rat { return new(big.Rat).SetInt64(v) }

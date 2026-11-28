// Package mesh is the "ordinary opaque Go API" control for the W-4642 spike.
//
// The proposal's drop condition is: "If an ordinary opaque Go API gives the
// same guarantees at the same boundaries without recomputation or caller
// bookkeeping, I would drop the language feature."  This package is that API,
// written the way a careful Go library author would write it in 2026:
//
//   - the write path is the only exported way to change a Mesh;
//   - Observe returns an unforgeable Turn (unexported fields, a package-held
//     mint capability) rather than a bare sign;
//   - Insert checks identity, epoch, ordered triple and a digest of the
//     coordinates that were actually classified.
//
// No caller bookkeeping, no recomputation of the predicate at commit time.
package mesh

import (
	"crypto/sha256"
	"errors"
	"math"
	"math/bits"
)

// CoordBound is the pre-registered coordinate bound, 2^50.
const CoordBound int64 = 1 << 50

// Point is an integer coordinate pair.
type Point struct{ X, Y int64 }

// Triple is the ordered vertex triple a witness is bound to.
type Triple [3]int32

// Direction values.
const (
	Right     int8 = -1
	Collinear int8 = 0
	Left      int8 = 1
)

var (
	ErrForged = errors.New("mesh: witness was not minted by this package")
	ErrMesh   = errors.New("mesh: witness is bound to another mesh")
	ErrEpoch  = errors.New("mesh: witness is bound to an earlier epoch")
	ErrTriple = errors.New("mesh: witness is bound to another ordered triple")
	ErrCoords = errors.New("mesh: coordinates moved under the witness")
	ErrBound  = errors.New("mesh: coordinate exceeds 2^50")
	ErrIndex  = errors.New("mesh: vertex index out of range")
)

// A mint value is the unforgeable capability. Only this package can hold it,
// so only this package can produce a Turn that Insert accepts.
type mint struct{ _ byte }

var theMint = &mint{}

// Turn is the witness. Every field is unexported: callers can hold it, pass it
// through helpers and store it, but they cannot build one.
type Turn struct {
	mesh  *Mesh
	epoch uint64
	t     Triple
	dir   int8
	sig   [16]byte
	cap   *mint
}

// Direction reports the classified sign. It is deliberately readable: the
// authority is in Insert, not in hiding the answer.
func (t Turn) Direction() int8 { return t.dir }

// Mesh owns its coordinates. The slice is unexported and never handed out.
type Mesh struct {
	id    uint64
	epoch uint64
	pts   []Point
}

// New returns a mesh at epoch 1.
func New(id uint64, pts ...Point) (*Mesh, error) {
	for _, p := range pts {
		if err := checkPoint(p); err != nil {
			return nil, err
		}
	}
	return &Mesh{id: id, epoch: 1, pts: append([]Point(nil), pts...)}, nil
}

// Epoch reports the current epoch (for tests and diagnostics).
func (m *Mesh) Epoch() uint64 { return m.epoch }

// Point returns a copy of one coordinate pair.
func (m *Mesh) Point(i int32) (Point, error) {
	if int(i) >= len(m.pts) || i < 0 {
		return Point{}, ErrIndex
	}
	return m.pts[i], nil
}

// Observe classifies the ordered triple and returns a bound witness.
func (m *Mesh) Observe(t Triple) (Turn, error) {
	p, err := m.pointsOf(t)
	if err != nil {
		return Turn{}, err
	}
	dir, _ := classify(p[0], p[1], p[2])
	return Turn{mesh: m, epoch: m.epoch, t: t, dir: dir, sig: digest(p), cap: theMint}, nil
}

// EditPoint moves a vertex and bumps the epoch.
func (m *Mesh) EditPoint(i int32, p Point) error {
	if err := checkPoint(p); err != nil {
		return err
	}
	if int(i) >= len(m.pts) || i < 0 {
		return ErrIndex
	}
	m.pts[i] = p
	m.epoch++
	return nil
}

// Insert is the only write path. It consumes a witness bound to this mesh,
// this epoch, this ordered triple and these coordinates.
func (m *Mesh) Insert(t Triple, w Turn) (int8, error) {
	if w.cap != theMint {
		return 0, ErrForged
	}
	if w.mesh != m {
		return 0, ErrMesh
	}
	if w.epoch != m.epoch {
		return 0, ErrEpoch
	}
	if w.t != t {
		return 0, ErrTriple
	}
	p, err := m.pointsOf(t)
	if err != nil {
		return 0, err
	}
	if digest(p) != w.sig {
		return 0, ErrCoords
	}
	m.epoch++
	return w.dir, nil
}

func (m *Mesh) pointsOf(t Triple) ([3]Point, error) {
	var out [3]Point
	for n, idx := range t {
		if idx < 0 || int(idx) >= len(m.pts) {
			return out, ErrIndex
		}
		out[n] = m.pts[idx]
	}
	return out, nil
}

func checkPoint(p Point) error {
	if p.X > CoordBound || p.X < -CoordBound || p.Y > CoordBound || p.Y < -CoordBound {
		return ErrBound
	}
	return nil
}

func digest(p [3]Point) [16]byte {
	var buf [48]byte
	for n, pt := range p {
		putUint64(buf[n*16:], uint64(pt.X))
		putUint64(buf[n*16+8:], uint64(pt.Y))
	}
	sum := sha256.Sum256(buf[:])
	var out [16]byte
	copy(out[:], sum[:16])
	return out
}

func putUint64(b []byte, v uint64) {
	b[7] = byte(v)
	b[6] = byte(v >> 8)
	b[5] = byte(v >> 16)
	b[4] = byte(v >> 24)
	b[3] = byte(v >> 32)
	b[2] = byte(v >> 40)
	b[1] = byte(v >> 48)
	b[0] = byte(v >> 56)
}

// ---------------------------------------------------------------------------
// The predicate: conservative binary64 filter, exact 128-bit fallback.
// ---------------------------------------------------------------------------

func diff(a, b Point) (int64, int64) { return b.X - a.X, b.Y - a.Y }

// classify returns (direction, usedExact).
func classify(a, b, c Point) (int8, bool) {
	bx, by := diff(a, b)
	cx, cy := diff(a, c)
	left := float64(bx) * float64(cy)
	right := float64(by) * float64(cx)
	if math.Nextafter(left, math.Inf(-1)) > math.Nextafter(right, math.Inf(1)) {
		return Left, false
	}
	if math.Nextafter(left, math.Inf(1)) < math.Nextafter(right, math.Inf(-1)) {
		return Right, false
	}
	return exactSign(bx, by, cx, cy), true
}

// Sign is the naive float64 implementation, exported as the control.
func Sign(a, b, c Point) int8 {
	bx, by := diff(a, b)
	cx, cy := diff(a, c)
	return signFloat(float64(bx)*float64(cy) - float64(by)*float64(cx))
}

// SignExact is the exact predicate, exported so the sign-only control package
// classifies with the same arithmetic instead of a second, divergent copy.
func SignExact(a, b, c Point) int8 {
	return exactSign(b.X-a.X, b.Y-a.Y, c.X-a.X, c.Y-a.Y)
}

func signFloat(f float64) int8 {
	switch {
	case f > 0:
		return Left
	case f < 0:
		return Right
	default:
		return Collinear
	}
}

// exactSign computes sign(bx*cy - by*cx) exactly with 128-bit products.
func exactSign(bx, by, cx, cy int64) int8 {
	hiL, loL := mul128(bx, cy)
	hiR, loR := mul128(by, cx)
	lo, borrow := bits.Sub64(loL, loR, 0)
	hi, _ := bits.Sub64(hiL, hiR, borrow)
	shi := int64(hi)
	switch {
	case shi > 0 || (shi == 0 && lo > 0):
		return Left
	case shi == 0 && lo == 0:
		return Collinear
	default:
		return Right
	}
}

// mul128 returns the two's-complement 128-bit product of two int64 as (hi, lo).
func mul128(a, b int64) (hi, lo uint64) {
	neg := (a < 0) != (b < 0)
	ua := magnitude(a)
	ub := magnitude(b)
	hi, lo = bits.Mul64(ua, ub)
	if neg {
		lo = ^lo
		hi = ^hi
		var carry uint64
		lo, carry = bits.Add64(lo, 1, 0)
		hi += carry
	}
	return hi, lo
}

func magnitude(v int64) uint64 {
	if v < 0 {
		return uint64(-v)
	}
	return uint64(v)
}

package xfer

import (
	"fmt"
	"math/big"
)

// Entry is one (source cell, overlap weight) pair inside a target row.
//
// W is exact: W * (target cell width) == overlap(source cell, target cell).
type Entry struct {
	Src int
	W   big.Rat
}

// Row is one destination cell and the weights that produce its value, i.e. the
// part of the write partition that one task is allowed to write.
type Row struct {
	Dst     int
	Width   int64
	Entries []Entry // strictly ascending by Src, positive weights only
}

// Plan is the opaque, mesh-bound transfer plan. It is created by BuildPlan, is
// read-only afterwards, and carries:
//
//   - identity: fingerprints (plus exact edge copies) of both meshes,
//   - the write partition: exactly one Row per destination cell, in index order,
//   - the conservation law: every source cell's measure is fully distributed
//     across rows, which is what makes the integral invariant.
type Plan struct {
	srcName, dstName   string
	srcFP, dstFP       uint64
	srcEdges, dstEdges []int64
	srcCells, dstCells int
	rows               []Row
	entries            int
}

// BuildPlan derives the sparse overlap plan from geometry and validates it
// before returning. A plan that fails Validate is never handed out.
func BuildPlan(src, dst *Mesh) (*Plan, error) {
	if src == nil || dst == nil {
		return nil, fmt.Errorf("%w: nil mesh", ErrMeshBuild)
	}
	p := &Plan{
		srcName: src.name, dstName: dst.name,
		srcFP: src.fp, dstFP: dst.fp,
		srcEdges: src.Edges(), dstEdges: dst.Edges(),
		srcCells: src.Cells(), dstCells: dst.Cells(),
		rows: make([]Row, dst.Cells()),
	}
	for j := range p.rows {
		dLo, dHi := dst.Cell(j)
		width := dHi - dLo
		row := Row{Dst: j, Width: width}
		for i := 0; i < src.Cells(); i++ {
			sLo, sHi := src.Cell(i)
			ov := OverlapLen(sLo, sHi, dLo, dHi)
			if ov == 0 {
				continue
			}
			// weight = overlap / target width; kept exact.
			row.Entries = append(row.Entries, Entry{Src: i, W: *new(big.Rat).SetFrac64(ov, width)})
			p.entries++
		}
		p.rows[j] = row
	}
	if err := p.Validate(); err != nil {
		return nil, err
	}
	return p, nil
}

// AdmitPlan is the entry point for a plan computed by something other than
// BuildPlan -- a planner, a cached artifact, a hand-written table. It binds the
// meshes, then validates; a plan that fails Validate is never handed out, so a
// caller can not obtain an unvalidated plan through this path either.
func AdmitPlan(src, dst *Mesh, rows []Row) (*Plan, error) {
	if src == nil || dst == nil {
		return nil, fmt.Errorf("%w: nil mesh", ErrMeshBuild)
	}
	p := &Plan{
		srcName: src.name, dstName: dst.name,
		srcFP: src.fp, dstFP: dst.fp,
		srcEdges: src.Edges(), dstEdges: dst.Edges(),
		srcCells: src.Cells(), dstCells: dst.Cells(),
	}
	if rows != nil {
		p.rows = make([]Row, len(rows))
		copy(p.rows, rows)
		for _, row := range p.rows {
			p.entries += len(row.Entries)
		}
	}
	if err := p.Validate(); err != nil {
		return nil, err
	}
	return p, nil
}

// SrcName, DstName, SrcFingerprint, DstFingerprint expose the bindings read-only.
func (p *Plan) SrcName() string        { return p.srcName }
func (p *Plan) DstName() string        { return p.dstName }
func (p *Plan) SrcFingerprint() uint64 { return p.srcFP }
func (p *Plan) DstFingerprint() uint64 { return p.dstFP }
func (p *Plan) SrcCells() int          { return p.srcCells }
func (p *Plan) DstCells() int          { return p.dstCells }
func (p *Plan) EntryCount() int        { return p.entries }

// Rows returns the write partition. The slice and its entries are read-only.
func (p *Plan) Rows() []Row { return p.rows }

// Validate checks the three things a plan claims to carry. It re-derives every
// claim from the stored edge sequences and the stored rationals, so it is not
// satisfied by construction: a plan with a perturbed weight fails it.
//
//	(1) geometry     weight * target width == overlap, for every entry
//	(2) partition    one row per destination cell, in order, entries strictly
//	                 ascending, and the row's overlaps sum to the cell width
//	                 (the cell is covered exactly once, never twice)
//	(3) conservation every source cell's measure is fully distributed over the
//	                 rows (sum_j w_ji * width_j == width_i)
func (p *Plan) Validate() error {
	if fingerprint(p.srcEdges) != p.srcFP || fingerprint(p.dstEdges) != p.dstFP {
		return fmt.Errorf("%w: stored edges do not match the stored fingerprints", ErrMeshIdentity)
	}
	if len(p.srcEdges)-1 != p.srcCells || len(p.dstEdges)-1 != p.dstCells {
		return fmt.Errorf("%w: edge count and cell count disagree", ErrRowCount)
	}
	if len(p.rows) != p.dstCells {
		return fmt.Errorf("%w: %d rows for %d destination cells", ErrRowCount, len(p.rows), p.dstCells)
	}
	seen := make([]bool, p.dstCells)
	dist := make([]big.Rat, p.srcCells) // accumulated measure per source cell
	for j, row := range p.rows {
		if row.Dst != j {
			return fmt.Errorf("%w: row %d carries destination index %d", ErrPartition, j, row.Dst)
		}
		if seen[row.Dst] {
			return fmt.Errorf("%w: destination %d appears twice", ErrPartition, row.Dst)
		}
		seen[row.Dst] = true
		dLo, dHi := p.dstEdges[j], p.dstEdges[j+1]
		width := dHi - dLo
		if row.Width != width {
			return fmt.Errorf("%w: row %d width %d, mesh says %d", ErrPartition, j, row.Width, width)
		}
		var covered int64
		prev := -1
		for _, e := range row.Entries {
			if e.Src <= prev {
				return fmt.Errorf("%w: row %d source indices not strictly ascending", ErrPartition, j)
			}
			prev = e.Src
			if e.Src < 0 || e.Src >= p.srcCells {
				return fmt.Errorf("%w: row %d source index %d out of range", ErrPartition, j, e.Src)
			}
			if e.W.Sign() <= 0 || e.W.Cmp(RatOf(1)) > 0 {
				return fmt.Errorf("%w: row %d weight %s outside (0,1]", ErrGeometry, j, e.W.RatString())
			}
			ov := OverlapLen(p.srcEdges[e.Src], p.srcEdges[e.Src+1], dLo, dHi)
			got := new(big.Rat).Mul(&e.W, new(big.Rat).SetInt64(width))
			if got.Cmp(new(big.Rat).SetInt64(ov)) != 0 {
				return fmt.Errorf("%w: row %d src %d: weight*width = %s, overlap = %d",
					ErrGeometry, j, e.Src, got.RatString(), ov)
			}
			covered += ov
			// accumulate w * width_j into the source cell's distributed measure
			share := new(big.Rat).Mul(&e.W, new(big.Rat).SetInt64(width))
			dist[e.Src].Add(&dist[e.Src], share)
		}
		if covered != width {
			return fmt.Errorf("%w: row %d overlaps sum to %d, cell width is %d",
				ErrPartition, j, covered, width)
		}
	}
	for i := 0; i < p.srcCells; i++ {
		want := new(big.Rat).SetInt64(p.srcEdges[i+1] - p.srcEdges[i])
		if dist[i].Cmp(want) != 0 {
			return fmt.Errorf("%w: source cell %d distributes %s of %s",
				ErrConservation, i, dist[i].RatString(), want.RatString())
		}
	}
	return nil
}

// CheckIntegral is the conservation law as a predicate the caller can run on the
// finished field: the exact total of the destination field must equal the exact
// total of the source field. It is independent of the weights.
func (p *Plan) CheckIntegral(srcVal, dstVal []*big.Rat) error {
	if len(srcVal) != p.srcCells || len(dstVal) != p.dstCells {
		return fmt.Errorf("%w: %d/%d values for %d/%d cells",
			ErrFieldLength, len(srcVal), len(dstVal), p.srcCells, p.dstCells)
	}
	srcTot, err := integralOver(p.srcEdges, srcVal)
	if err != nil {
		return err
	}
	dstTot, err := integralOver(p.dstEdges, dstVal)
	if err != nil {
		return err
	}
	if srcTot.Cmp(dstTot) != 0 {
		return fmt.Errorf("%w: source total %s, destination total %s",
			ErrConservation, srcTot.RatString(), dstTot.RatString())
	}
	return nil
}

// integralOver is the exact integral of a piecewise-constant field on its mesh.
func integralOver(edges []int64, val []*big.Rat) (*big.Rat, error) {
	if len(val) != len(edges)-1 {
		return nil, fmt.Errorf("%w: %d values for %d cells", ErrFieldLength, len(val), len(edges)-1)
	}
	tot := new(big.Rat)
	tmp := new(big.Rat)
	for i, v := range val {
		tmp.Mul(v, new(big.Rat).SetInt64(edges[i+1]-edges[i]))
		tot.Add(tot, tmp)
	}
	return tot, nil
}

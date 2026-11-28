package xfer

import (
	"fmt"
	"math/big"
)

// Run is the write-partition bookkeeping for one application of a plan. It is
// what turns "each destination cell is written exactly once" from a plan-time
// claim into something a worker sweep can be held to at runtime.
type Run struct {
	plan   *Plan
	wrote  []bool
	writes int
}

// NewRun starts a fresh application of the plan.
func (p *Plan) NewRun() *Run {
	return &Run{plan: p, wrote: make([]bool, p.dstCells)}
}

// Writes reports how many destination cells have been written so far.
func (r *Run) Writes() int { return r.writes }

func (r *Run) claim(j int) error {
	if j < 0 || j >= len(r.wrote) {
		return fmt.Errorf("%w: destination index %d out of range", ErrPartition, j)
	}
	if r.wrote[j] {
		return fmt.Errorf("%w: destination %d", ErrDoubleWrite, j)
	}
	r.wrote[j] = true
	r.writes++
	return nil
}

// Finish checks that the sweep covered the whole write partition: every
// destination cell written, none written twice.
func (r *Run) Finish() error {
	for j, w := range r.wrote {
		if !w {
			return fmt.Errorf("%w: destination %d never written", ErrPartition, j)
		}
	}
	return nil
}

func sameEdges(a, b []int64) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}

// bind is the identity gate. Every entry point goes through it before a single
// value is computed, so a plan can not be applied to a field of another mesh.
func (p *Plan) bind(src, dst *Mesh, srcVal []*big.Rat) error {
	if src == nil || dst == nil {
		return fmt.Errorf("%w: nil mesh in call", ErrMeshIdentity)
	}
	if src.fp != p.srcFP || dst.fp != p.dstFP ||
		!sameEdges(src.edges, p.srcEdges) || !sameEdges(dst.edges, p.dstEdges) {
		return fmt.Errorf("%w: plan is %s->%s (fp %016x->%016x), call names %s->%s (fp %016x->%016x)",
			ErrMeshIdentity, p.srcName, p.dstName, p.srcFP, p.dstFP,
			src.name, dst.name, src.fp, dst.fp)
	}
	if len(srcVal) != p.srcCells {
		return fmt.Errorf("%w: source field has %d values, mesh has %d cells",
			ErrFieldLength, len(srcVal), p.srcCells)
	}
	return nil
}

// RowView is what the generated-loop body is allowed to see: one destination row
// that this worker owns, its width, and the read-only weights. There is no
// destination index to choose -- ownership is fixed by the loop, and Store is the
// only write path.
type RowView struct {
	plan   *Plan
	dst    int
	srcVal []*big.Rat
	owned  bool
	sum    big.Rat
	tmp    big.Rat
	hasSum bool
}

func (v *RowView) reset(j int, srcVal []*big.Rat) {
	v.dst = j
	v.srcVal = srcVal
	v.owned = true
	v.hasSum = false
	v.sum.SetInt64(0)
}

// Dst returns the destination cell this row owns.
func (v *RowView) Dst() int { return v.dst }

// Width returns the destination cell width.
func (v *RowView) Width() int64 { return v.plan.rows[v.dst].Width }

// Entries returns the row's weights (read-only).
func (v *RowView) Entries() []Entry { return v.plan.rows[v.dst].Entries }

// Value computes the exact overlap-weighted average of the row. The returned
// rational is owned by the view and is overwritten by the next row, exactly like
// a value in a generated loop's local variable.
func (v *RowView) Value() *big.Rat {
	if !v.owned {
		panic("xfer: Value on an unowned row")
	}
	if v.hasSum {
		return &v.sum
	}
	v.sum.SetInt64(0)
	for _, e := range v.plan.rows[v.dst].Entries {
		v.tmp.Mul(&e.W, v.srcVal[e.Src])
		v.sum.Add(&v.sum, &v.tmp)
	}
	v.hasSum = true
	return &v.sum
}

// Store is the only write path available to a generated loop body. It writes the
// row this view owns and refuses any other destination index.
func (v *RowView) Store(dstVal []*big.Rat) error {
	if !v.owned {
		return fmt.Errorf("%w: view was not handed out by a sweep", ErrUnowned)
	}
	if len(dstVal) != v.plan.dstCells {
		return fmt.Errorf("%w: target field has %d values, mesh has %d cells",
			ErrFieldLength, len(dstVal), v.plan.dstCells)
	}
	dstVal[v.dst].Set(v.Value())
	return nil
}

// GatherRows is the generated-gather-loop shape. This worker is handed only the
// rows it owns, one at a time, and the body can not name any other index: the
// destination values of other workers' rows are not reachable from here.
//
// It returns the number of rows this worker wrote.
func (r *Run) GatherRows(worker, workers int, src, dst *Mesh,
	srcVal, dstVal []*big.Rat) (int, error) {
	p := r.plan
	if err := p.bind(src, dst, srcVal); err != nil {
		return 0, err
	}
	if workers < 1 {
		return 0, fmt.Errorf("xfer: workers must be >= 1, got %d", workers)
	}
	if worker < 0 || worker >= workers {
		return 0, fmt.Errorf("xfer: worker %d is not in [0,%d)", worker, workers)
	}
	if len(dstVal) != p.dstCells {
		return 0, fmt.Errorf("%w: target field has %d values, mesh has %d cells",
			ErrFieldLength, len(dstVal), p.dstCells)
	}
	var v RowView
	v.plan = p
	n := 0
	for j := worker; j < p.dstCells; j += workers {
		if err := r.claim(j); err != nil {
			return n, err
		}
		v.reset(j, srcVal)
		if err := v.Store(dstVal); err != nil {
			return n, err
		}
		n++
	}
	return n, nil
}

// ApplyRows is the callback shape: the library computes the value of a
// destination row and hands the caller both the destination index and the value,
// so the caller decides where to put it. That flexibility is the reason the
// identity gate in bind has to exist.
//
// The value pointer is transient: it is overwritten by the next row. Copy it if
// you keep it.
func (r *Run) ApplyRows(worker, workers int, src, dst *Mesh,
	srcVal []*big.Rat, write func(dstIndex int, v *big.Rat) error) (int, error) {
	p := r.plan
	if err := p.bind(src, dst, srcVal); err != nil {
		return 0, err
	}
	if write == nil {
		return 0, fmt.Errorf("xfer: nil write callback")
	}
	if workers < 1 {
		return 0, fmt.Errorf("xfer: workers must be >= 1, got %d", workers)
	}
	if worker < 0 || worker >= workers {
		return 0, fmt.Errorf("xfer: worker %d is not in [0,%d)", worker, workers)
	}
	var v RowView
	v.plan = p
	n := 0
	for j := worker; j < p.dstCells; j += workers {
		if err := r.claim(j); err != nil {
			return n, err
		}
		v.reset(j, srcVal)
		if err := write(j, v.Value()); err != nil {
			return n, err
		}
		n++
	}
	return n, nil
}

// Apply is the one-shot convenience form: a single-worker gather over the whole
// plan, followed by the partition check.
func (p *Plan) Apply(src, dst *Mesh, srcVal, dstVal []*big.Rat) error {
	run := p.NewRun()
	if _, err := run.GatherRows(0, 1, src, dst, srcVal, dstVal); err != nil {
		return err
	}
	return run.Finish()
}

// ApplyUnbound is the negative control for the whole spike: an ordinary
// index-based library that trusts the caller's word about which meshes the value
// slices belong to. It checks bounds only. It exists so the tests can measure the
// failure that the identity gate prevents, and so the cost of the gate can be
// compared against a library that does not pay it.
//
// It is not part of the proposed API. Do not use it in a task graph.
func (p *Plan) ApplyUnbound(srcVal, dstVal []*big.Rat,
	write func(dstIndex int, v *big.Rat) error) error {
	if len(srcVal) < p.srcCells {
		return fmt.Errorf("%w: source field has %d values, plan reads %d",
			ErrFieldLength, len(srcVal), p.srcCells)
	}
	if len(dstVal) < p.dstCells {
		return fmt.Errorf("%w: target field has %d values, plan writes %d",
			ErrFieldLength, len(dstVal), p.dstCells)
	}
	var v RowView
	v.plan = p
	v.srcVal = srcVal
	for j := 0; j < p.dstCells; j++ {
		v.reset(j, srcVal)
		dstVal[j].Set(v.Value())
		if write != nil {
			if err := write(j, v.Value()); err != nil {
				return err
			}
		}
	}
	return nil
}

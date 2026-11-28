// Package seek compiles a closed, acyclic, fixed-rate pipeline of pure scalar
// stages into an accessor that answers "what is output k?" from an immutable
// source without evaluating anything before k.
//
// This is the W-4593 experiment (issue #4593), written the way the proposal
// describes it: each stage declares (p inputs, q outputs) and a literal
// dependency list per output lane; output k belongs to firing k/q and lane
// k%q; that lane reads p*(k/q)+d for each dependency d. Composing the maps
// backwards turns a late output coordinate into a handful of source indices.
package seek

import (
	"errors"
	"fmt"
)

// Errors reported by Compile and At. Every one of them is a refusal: the
// compiler never guesses a dependency.
var (
	ErrStateful  = errors.New("seek: stage is stateful; a fixed-rate entry point cannot admit it")
	ErrRate      = errors.New("seek: stage arity must be positive")
	ErrDep       = errors.New("seek: dependency offset outside the input frame")
	ErrOverflow  = errors.New("seek: coordinate arithmetic overflows int64")
	ErrRange     = errors.New("seek: output coordinate out of range")
	ErrShortRead = errors.New("seek: source snapshot is shorter than the plan needs")
)

// Stage is one pure, fixed-rate scalar stage.
//
// Deps[l] lists the input offsets that output lane l reads. Eval computes the
// lane value from those inputs in the same order. Stateful stages carry
// Stateful = true and are refused by Compile: the proposal's entry point only
// admits stateless stages, and a running sum is the canonical counter-example.
type Stage struct {
	Name     string
	In, Out  int
	Deps     [][]int
	Eval     func(frame []uint32) uint32
	Stateful bool
}

// Pipeline is an ordered list of stages; stage 0 reads the source.
type Pipeline struct {
	Stages []Stage
}

// Snapshot is an immutable source with a read counter, so "how many words did
// this query touch" is measurable rather than asserted.
type Snapshot struct {
	Words []uint32
	seens map[int]struct{}
	reads int
}

// NewSnapshot wraps a word slice without copying it.
func NewSnapshot(words []uint32) *Snapshot {
	return &Snapshot{Words: words, seens: make(map[int]struct{}, 16)}
}

// ResetReads clears the counter and the distinct-index set.
func (s *Snapshot) ResetReads() {
	s.seens = make(map[int]struct{}, 16)
	s.reads = 0
}

// Reads reports how many distinct source words were touched since the last
// ResetReads.
func (s *Snapshot) Reads() int { return s.reads }

// Indices reports the distinct source indices touched since ResetReads, in
// ascending order.
func (s *Snapshot) Indices() []int {
	out := make([]int, 0, len(s.seens))
	for i := range s.seens {
		out = append(out, i)
	}
	for i := 1; i < len(out); i++ {
		for j := i; j > 0 && out[j] < out[j-1]; j-- {
			out[j], out[j-1] = out[j-1], out[j]
		}
	}
	return out
}

func (s *Snapshot) word(i int) (uint32, error) {
	if i < 0 || i >= len(s.Words) {
		return 0, ErrShortRead
	}
	if _, ok := s.seens[i]; !ok {
		s.seens[i] = struct{}{}
		s.reads++
	}
	return s.Words[i], nil
}

// Compiled is a plan: it knows the output length for a given source length and
// can resolve any in-range output coordinate to its value.
type Compiled struct {
	stages []Stage
}

// Compile validates the pipeline and returns a plan. Refusals are errors, never
// silent acceptances.
func Compile(p Pipeline) (*Compiled, error) {
	if len(p.Stages) == 0 {
		return nil, fmt.Errorf("seek: empty pipeline")
	}
	stages := make([]Stage, len(p.Stages))
	for i, st := range p.Stages {
		if st.Stateful {
			return nil, fmt.Errorf("%w: stage %d (%s)", ErrStateful, i, st.Name)
		}
		if st.In <= 0 || st.Out <= 0 {
			return nil, fmt.Errorf("%w: stage %d (%s) has %d->%d", ErrRate, i, st.Name, st.In, st.Out)
		}
		if len(st.Deps) != st.Out {
			return nil, fmt.Errorf("%w: stage %d (%s) declares %d lanes but %d output arity",
				ErrDep, i, st.Name, len(st.Deps), st.Out)
		}
		for lane, deps := range st.Deps {
			if len(deps) == 0 {
				return nil, fmt.Errorf("%w: stage %d (%s) lane %d has no dependencies",
					ErrDep, i, st.Name, lane)
			}
			for _, d := range deps {
				if d < 0 || d >= st.In {
					return nil, fmt.Errorf("%w: stage %d (%s) lane %d reads offset %d of %d",
						ErrDep, i, st.Name, lane, d, st.In)
				}
			}
		}
		stages[i] = st
	}
	return &Compiled{stages: stages}, nil
}

// OutLen returns how many outputs a source of n words produces, or an error if
// the coordinate arithmetic would overflow int64.
func (c *Compiled) OutLen(n int64) (int64, error) {
	if n < 0 {
		return 0, ErrRange
	}
	cur := n
	for i, st := range c.stages {
		frames := cur / int64(st.In)
		if frames != 0 && int64(st.Out) > (1<<62)/frames {
			return 0, fmt.Errorf("%w: stage %d output length", ErrOverflow, i)
		}
		cur = frames * int64(st.Out)
	}
	return cur, nil
}

// At returns output k for the given snapshot. It reads only the source words
// that k depends on: no prefix evaluation, no dense materialisation.
func (c *Compiled) At(s *Snapshot, k int64) (uint32, error) {
	outLen, err := c.OutLen(int64(len(s.Words)))
	if err != nil {
		return 0, err
	}
	if k < 0 || k >= outLen {
		return 0, fmt.Errorf("%w: k=%d, outLen=%d", ErrRange, k, outLen)
	}
	memo := make(map[valueKey]uint32, 8)
	v, err := c.eval(s, len(c.stages), k, memo)
	if err != nil {
		return 0, err
	}
	return v, nil
}

type valueKey struct {
	stage int
	index int64
}

// eval resolves the value of output `index` at stage `level` (level ==
// len(stages) means the pipeline's own output).
func (c *Compiled) eval(s *Snapshot, level int, index int64, memo map[valueKey]uint32) (uint32, error) {
	if level == 0 {
		w, err := s.word(int(index))
		if err != nil {
			return 0, err
		}
		return w, nil
	}
	key := valueKey{stage: level, index: index}
	if v, ok := memo[key]; ok {
		return v, nil
	}
	st := c.stages[level-1]
	if index < 0 {
		return 0, ErrRange
	}
	fire := index / int64(st.Out)
	lane := int(index % int64(st.Out))
	if fire != 0 && int64(st.In) > (1<<62)/fire {
		return 0, ErrOverflow
	}
	base := fire * int64(st.In)
	deps := st.Deps[lane]
	frame := make([]uint32, len(deps))
	for n, d := range deps {
		src := base + int64(d)
		if src != 0 && src/int64(st.In) != fire {
			return 0, ErrOverflow
		}
		v, err := c.eval(s, level-1, src, memo)
		if err != nil {
			return 0, err
		}
		frame[n] = v
	}
	v := st.Eval(frame)
	memo[key] = v
	return v, nil
}

// SourceIndices returns, without reading anything, the sorted distinct source
// indices output k depends on. It exists so a test can compare the plan against
// what At actually touched.
func (c *Compiled) SourceIndices(k int64, sourceLen int64) ([]int, error) {
	outLen, err := c.OutLen(sourceLen)
	if err != nil {
		return nil, err
	}
	if k < 0 || k >= outLen {
		return nil, fmt.Errorf("%w: k=%d, outLen=%d", ErrRange, k, outLen)
	}
	seen := make(map[int]struct{}, 8)
	if err := c.collect(len(c.stages), k, seen); err != nil {
		return nil, err
	}
	out := make([]int, 0, len(seen))
	for i := range seen {
		out = append(out, i)
	}
	for i := 1; i < len(out); i++ {
		for j := i; j > 0 && out[j] < out[j-1]; j-- {
			out[j], out[j-1] = out[j-1], out[j]
		}
	}
	return out, nil
}

func (c *Compiled) collect(level int, index int64, seen map[int]struct{}) error {
	if level == 0 {
		if index < 0 || index > 1<<40 {
			return ErrOverflow
		}
		seen[int(index)] = struct{}{}
		return nil
	}
	st := c.stages[level-1]
	fire := index / int64(st.Out)
	lane := int(index % int64(st.Out))
	for _, d := range st.Deps[lane] {
		if err := c.collect(level-1, fire*int64(st.In)+int64(d), seen); err != nil {
			return err
		}
	}
	return nil
}

// Dense evaluates the whole pipeline the obvious way. It is the reference the
// accessor is checked against, and the baseline the cost numbers are compared
// with.
func (c *Compiled) Dense(words []uint32) ([]uint32, error) {
	cur := append([]uint32(nil), words...)
	for i, st := range c.stages {
		if len(cur)%st.In != 0 {
			// incomplete frames emit nothing
		}
		frames := len(cur) / st.In
		next := make([]uint32, frames*st.Out)
		for f := 0; f < frames; f++ {
			base := f * st.In
			for lane := 0; lane < st.Out; lane++ {
				deps := st.Deps[lane]
				frame := make([]uint32, len(deps))
				for n, d := range deps {
					frame[n] = cur[base+d]
				}
				next[f*st.Out+lane] = st.Eval(frame)
			}
		}
		cur = next
		if i == len(c.stages)-1 && len(next) == 0 {
			return next, nil
		}
	}
	return cur, nil
}

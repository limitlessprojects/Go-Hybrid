// Package harness holds the executable acceptance tests for the #4596 spike.
//
// Every admitted program is checked three ways on the same records:
//
//	gen.Forward / gen.Inverse  (emitted Go)
//	ir interpreter             (independent of the emitter)
//	refimpl                    (hand-written, shares nothing with package ir)
package harness

import (
	"encoding/binary"
	"fmt"
	"math/rand/v2"
	"path/filepath"
	"sort"
	"testing"

	"revreg/gen"
	"revreg/ir"
	"revreg/refimpl"
)

// refPair bundles the hand-written forward and inverse for one case.
type refPair struct {
	fwd func(refimpl.S, uint64) refimpl.S
	inv func(refimpl.S, uint64) refimpl.S
}

var refs = map[string]refPair{
	"pos_rotl_xor_key":    {refimpl.ForwardPosRotlXorKey, refimpl.InvPosRotlXorKey},
	"pos_swap_pair":       {refimpl.ForwardPosSwapPair, refimpl.InvPosSwapPair},
	"pos_arx_round":       {refimpl.ForwardPosArxRound, refimpl.InvPosArxRound},
	"pos_perm_cycle":      {refimpl.ForwardPosPermCycle, refimpl.InvPosPermCycle},
	"pos_branch_addsub":   {refimpl.ForwardPosBranchAddsub, refimpl.InvPosBranchAddsub},
	"pos_branch_nested":   {refimpl.ForwardPosBranchNested, refimpl.InvPosBranchNested},
	"pos_branch_xor_key":  {refimpl.ForwardPosBranchXorKey, refimpl.InvPosBranchXorKey},
	"pos_wide_mix":        {refimpl.ForwardPosWideMix, refimpl.InvPosWideMix},
	"pos_sbox_bytes":      {refimpl.ForwardPosSboxBytes, refimpl.InvPosSboxBytes},
	"pos_full_mix_branch": {refimpl.ForwardPosFullMixBranch, refimpl.InvPosFullMixBranch},
}

func loadPrograms(t *testing.T, dir string) []*ir.Program {
	t.Helper()
	files, err := filepath.Glob(filepath.Join("..", dir, "*.rev"))
	if err != nil {
		t.Fatalf("glob %s: %v", dir, err)
	}
	if len(files) == 0 {
		t.Fatalf("no .rev files in ../%s", dir)
	}
	sort.Strings(files)
	var out []*ir.Program
	for _, f := range files {
		p, err := ir.ParseFile(f)
		if err != nil {
			t.Fatalf("parse %s: %v", f, err)
		}
		out = append(out, p)
	}
	return out
}

// ---- byte-exact comparison helpers: round-trip equality must be byte equality ----

func bytesOf(s gen.State) [64]byte {
	var b [64]byte
	for i, v := range [8]uint64{s.X0, s.X1, s.X2, s.X3, s.X4, s.X5, s.X6, s.X7} {
		binary.LittleEndian.PutUint64(b[i*8:], v)
	}
	return b
}

func bytesOfRef(s refimpl.S) [64]byte {
	var b [64]byte
	for i, v := range [8]uint64{s.A0, s.A1, s.A2, s.A3, s.A4, s.A5, s.A6, s.A7} {
		binary.LittleEndian.PutUint64(b[i*8:], v)
	}
	return b
}

func bytesOfIR(s ir.State) [64]byte {
	var b [64]byte
	for i := 0; i < 8; i++ {
		binary.LittleEndian.PutUint64(b[i*8:], s[i])
	}
	return b
}

func toIR(s gen.State, key uint64) ir.State {
	st := ir.State{s.X0, s.X1, s.X2, s.X3, s.X4, s.X5, s.X6, s.X7, 0}
	st[ir.KeyReg] = key
	return st
}

func fromIR(s ir.State) gen.State {
	return gen.State{X0: s[0], X1: s[1], X2: s[2], X3: s[3], X4: s[4], X5: s[5], X6: s[6], X7: s[7]}
}

func toRef(s gen.State) refimpl.S {
	return refimpl.S{A0: s.X0, A1: s.X1, A2: s.X2, A3: s.X3, A4: s.X4, A5: s.X5, A6: s.X6, A7: s.X7}
}

func fromRef(s refimpl.S) gen.State {
	return gen.State{X0: s.A0, X1: s.A1, X2: s.A2, X3: s.A3, X4: s.A4, X5: s.A5, X6: s.A6, X7: s.A7}
}

// boundaryValues is the pre-registered integer edge corpus.
var boundaryValues = []uint64{
	0, 1, ^uint64(0), 1 << 63, (1 << 63) - 1,
	0xAAAAAAAAAAAAAAAA, 0x5555555555555555, 0xFF,
}

func boundaryRecords() [][8]uint64 {
	var recs [][8]uint64
	for _, v := range boundaryValues { // all words on one edge value
		var r [8]uint64
		for i := range r {
			r[i] = v
		}
		recs = append(recs, r)
	}
	for pos := 0; pos < 8; pos++ { // one word on an edge value
		for _, v := range boundaryValues {
			var r [8]uint64
			for i := range r {
				r[i] = uint64(i)*0x0123456789abcdef + 0x1111
			}
			r[pos] = v
			recs = append(recs, r)
		}
	}
	rng := rand.New(rand.NewPCG(0x4596, 0xb0a17e)) // edge values mixed by a fixed PRNG
	for n := 0; n < 512; n++ {
		var r [8]uint64
		for i := range r {
			r[i] = boundaryValues[rng.IntN(len(boundaryValues))]
		}
		recs = append(recs, r)
	}
	return recs
}

func TestAdmission(t *testing.T) {
	pos := loadPrograms(t, "cases/pos")
	if len(pos) != 10 {
		t.Fatalf("want 10 positive cases, got %d", len(pos))
	}
	for _, p := range pos {
		if d := ir.Check(p); len(d) != 0 {
			t.Errorf("%s: expected admitted, rejected with %s", p.Name, d[0])
		}
	}

	neg := loadPrograms(t, "cases/neg")
	if len(neg) != 10 {
		t.Fatalf("want 10 negative cases, got %d", len(neg))
	}
	seen := map[string]string{}
	for _, p := range neg {
		d := ir.Check(p)
		if len(d) == 0 {
			t.Errorf("%s: admitted, must fail closed", p.Name)
			continue
		}
		want := p.MetaOr("expect", "")
		if d[0].Code != want {
			t.Errorf("%s: expected %s, got %s (%s)", p.Name, want, d[0].Code, d[0].Msg)
		}
		if prev, dup := seen[d[0].Code]; dup {
			t.Errorf("diagnostic %s is not distinguishable: used by %s and %s", d[0].Code, prev, p.Name)
		}
		seen[d[0].Code] = p.Name
		t.Logf("%-22s -> %s (line %d)", p.Name, d[0].Code, d[0].Line)
	}
	if len(seen) != 10 {
		t.Errorf("want 10 distinct diagnostic codes, got %d", len(seen))
	}
}

func TestCorpusPredictions(t *testing.T) {
	progs := loadPrograms(t, "corpus")
	admitted := 0
	byCode := map[string]int{}
	for _, p := range progs {
		want := p.MetaOr("expect", "")
		d := ir.Check(p)
		got := "ADMIT"
		if len(d) > 0 {
			got = d[0].Code
			byCode[got]++
		} else {
			admitted++
		}
		if got != want {
			t.Errorf("%s: pre-registered %q, observed %q", p.Name, want, got)
		}
	}
	rate := 100 * float64(admitted) / float64(len(progs))
	t.Logf("corpus: %d/%d admitted (%.1f%%), rejected by code: %v", admitted, len(progs), rate, byCode)
}

func TestRoundTrip(t *testing.T) {
	progs := loadPrograms(t, "cases/pos")
	byName := map[string]*ir.Program{}
	for _, p := range progs {
		byName[p.Name] = p
	}
	if len(gen.Cases) != 10 {
		t.Fatalf("emitted registry has %d cases, want 10", len(gen.Cases))
	}

	const perCase = 100000
	edges := boundaryRecords()
	totalRecords, totalInverseExec := 0, 0

	for _, c := range gen.Cases {
		c := c
		prog, ok := byName[c.Name]
		if !ok {
			t.Fatalf("emitted case %s has no source program", c.Name)
		}
		rf, ok := refs[c.Name]
		if !ok {
			t.Fatalf("no hand-written reference for %s", c.Name)
		}
		invProg := ir.Invert(prog)
		if invProg.IfCount != prog.IfCount {
			t.Fatalf("%s: inverse lost a branch", c.Name)
		}

		rng := rand.New(rand.NewPCG(0x45960000+uint64(len(c.Name)), uint64(len(c.Name))*7919+1))
		branchCount := map[int][2]int{}
		records := 0

		check := func(words [8]uint64, key uint64, kind string) {
			s := gen.State{X0: words[0], X1: words[1], X2: words[2], X3: words[3], X4: words[4], X5: words[5], X6: words[6], X7: words[7]}

			// (a) hand-written forward vs emitted forward
			refF := fromRef(rf.fwd(toRef(s), key))
			fGen := c.Forward(s, key)
			if got, want := bytesOf(fGen), bytesOf(refF); got != want {
				t.Fatalf("%s [%s]: emitted forward != hand-written forward\n in   %x\n got  %x\n want %x",
					c.Name, kind, bytesOf(s), got, want)
			}

			// (b) interpreter forward vs emitted forward, plus branch coverage
			stIR := toIR(s, key)
			fIR, trace := execOrFatal(t, prog, stIR, c.Name, kind)
			if got, want := bytesOfIR(fIR), bytesOf(fGen); got != want {
				t.Fatalf("%s [%s]: interpreter forward != emitted forward", c.Name, kind)
			}
			for _, d := range trace {
				bc := branchCount[d.ID]
				if d.Taken {
					bc[1]++
				} else {
					bc[0]++
				}
				branchCount[d.ID] = bc
			}

			// (c) emitted inverse must restore the input byte-for-byte
			backGen := c.Inverse(fGen, key)
			if got, want := bytesOf(backGen), bytesOf(s); got != want {
				t.Fatalf("%s [%s]: inverse(forward(s)) != s\n in   %x\n fwd  %x\n back %x",
					c.Name, kind, want, bytesOf(fGen), got)
			}

			// (d) hand-written inverse, applied to the same forward output
			backRef := fromRef(rf.inv(toRef(fGen), key))
			if got, want := bytesOf(backRef), bytesOf(s); got != want {
				t.Fatalf("%s [%s]: hand-written inverse(forward(s)) != s", c.Name, kind)
			}

			// (e) interpreter running the mechanically inverted program
			bIR, _ := execOrFatal(t, invProg, fIR, c.Name, kind+"-inv")
			if got, want := bytesOfIR(bIR), bytesOf(s); got != want {
				t.Fatalf("%s [%s]: interpreter inverse != s", c.Name, kind)
			}
			totalInverseExec++
			records++
		}

		for i := 0; i < perCase; i++ {
			var w [8]uint64
			for k := range w {
				w[k] = rng.Uint64()
			}
			check(w, rng.Uint64(), "random")
		}
		for _, w := range edges {
			check(w, 0, "edge")
			check(w, ^uint64(0), "edge")
		}

		if c.HasBranch {
			if len(branchCount) != prog.IfCount {
				t.Fatalf("%s: %d of %d conditionals never reached", c.Name, prog.IfCount-len(branchCount), prog.IfCount)
			}
			for id, bc := range branchCount {
				if bc[0] == 0 || bc[1] == 0 {
					t.Fatalf("%s: if#%d arms not both exercised: else=%d then=%d", c.Name, id, bc[0], bc[1])
				}
			}
		} else if len(branchCount) != 0 {
			t.Fatalf("%s: branch decisions recorded for a branch-free case", c.Name)
		}
		t.Logf("%-22s records=%-6d ifs=%d %s", c.Name, records, prog.IfCount, fmtArms(branchCount))
		totalRecords += records
	}

	t.Logf("TOTAL: %d records, %d inverse executions, 3-way agreement (emitted / interpreter / hand-written)",
		totalRecords, totalInverseExec)
	if totalRecords < 100000 {
		t.Fatalf("only %d records, pre-registration requires >= 100000", totalRecords)
	}
}

func fmtArms(bc map[int][2]int) string {
	if len(bc) == 0 {
		return ""
	}
	ids := make([]int, 0, len(bc))
	for id := range bc {
		ids = append(ids, id)
	}
	sort.Ints(ids)
	s := "arms[else/then]:"
	for _, id := range ids {
		s += fmt.Sprintf(" #%d=%d/%d", id, bc[id][0], bc[id][1])
	}
	return s
}

func execOrFatal(t *testing.T, p *ir.Program, s ir.State, name, kind string) (ir.State, []ir.Decision) {
	t.Helper()
	out, trace, err := ir.ExecProgram(p, s)
	if err != nil {
		t.Fatalf("%s [%s]: interpreter error: %v", name, kind, err)
	}
	return out, trace
}

func TestEveryDiagnosticIsExercised(t *testing.T) {
	progs := append(loadPrograms(t, "cases/neg"), loadPrograms(t, "corpus")...)
	used := map[string]bool{}
	for _, p := range progs {
		for _, d := range ir.Check(p) {
			used[d.Code] = true
			if _, ok := ir.Codes[d.Code]; !ok {
				t.Errorf("%s: diagnostic %s is not in the catalogue", p.Name, d.Code)
			}
		}
	}
	var missing []string
	for code := range ir.Codes {
		if !used[code] {
			missing = append(missing, code)
		}
	}
	sort.Strings(missing)
	if len(missing) > 0 {
		t.Errorf("catalogue codes never exercised: %v", missing)
	}
	t.Logf("diagnostics exercised: %d/%d catalogue entries", len(used), len(ir.Codes))
}

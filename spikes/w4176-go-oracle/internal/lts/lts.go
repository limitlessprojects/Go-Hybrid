// Package lts is the explicit-state labelled transition system over an
// extracted model: interleaved task steps, buffered and rendezvous channels,
// mutex acquisition, and vector-clock data-race detection.
package lts

import (
	"fmt"
	"sort"
	"strings"

	"hybrid.local/w4176gooracle/internal/model"
)

// Access records one memory access together with the accessing task's
// vector clock at that moment.
type Access struct {
	Task  int
	Write bool
	VC    []int
}

// State is one node of the reachable graph.
type State struct {
	PCs       []int // per task: -1 not spawned, len(steps) done
	Buf       []int // per channel: buffered item count
	Msg       [][]int
	Lock      []int // per mutex: holder task, -1 free
	MutexVC   [][]int
	VC        [][]int
	LastWrite []*Access
	Reads     [][]Access
	Closed    []bool
}

// Machine binds a model to index tables.
type Machine struct {
	Prog     *model.Program
	ChanIdx  map[string]int
	MutexIdx map[string]int
	LocIdx   map[string]int
	ChanName []string
	MutexNam []string
	LocName  []string
	N        int
}

// NewMachine builds the index tables for a program.
func NewMachine(p *model.Program) *Machine {
	m := &Machine{
		Prog:     p,
		ChanIdx:  map[string]int{},
		MutexIdx: map[string]int{},
		LocIdx:   map[string]int{},
		N:        len(p.Tasks),
	}
	for name := range p.ChanCap {
		m.ChanName = append(m.ChanName, name)
	}
	sort.Strings(m.ChanName)
	for i, n := range m.ChanName {
		m.ChanIdx[n] = i
	}
	for _, name := range p.Mutexes {
		m.MutexIdx[name] = len(m.MutexNam)
		m.MutexNam = append(m.MutexNam, name)
	}
	for _, name := range p.Locs {
		m.LocIdx[name] = len(m.LocName)
		m.LocName = append(m.LocName, name)
	}
	return m
}

// Initial returns s0: main at pc 0, no child spawned.
func (m *Machine) Initial() *State {
	s := &State{
		PCs:       make([]int, m.N),
		Buf:       make([]int, len(m.ChanName)),
		Msg:       make([][]int, len(m.ChanName)),
		Lock:      make([]int, len(m.MutexNam)),
		MutexVC:   make([][]int, len(m.MutexNam)),
		VC:        make([][]int, m.N),
		LastWrite: make([]*Access, len(m.LocName)),
		Reads:     make([][]Access, len(m.LocName)),
		Closed:    make([]bool, len(m.ChanName)),
	}
	for i := range s.PCs {
		s.PCs[i] = -1
	}
	for i := range s.Lock {
		s.Lock[i] = -1
	}
	for i := range s.MutexVC {
		s.MutexVC[i] = make([]int, m.N)
	}
	for i := range s.VC {
		s.VC[i] = make([]int, m.N)
	}
	s.PCs[0] = 0
	s.VC[0][0] = 1
	return s
}

func (s *State) clone() *State {
	c := &State{
		PCs:       append([]int(nil), s.PCs...),
		Buf:       append([]int(nil), s.Buf...),
		Msg:       make([][]int, len(s.Msg)),
		Lock:      append([]int(nil), s.Lock...),
		MutexVC:   make([][]int, len(s.MutexVC)),
		VC:        make([][]int, len(s.VC)),
		LastWrite: make([]*Access, len(s.LastWrite)),
		Reads:     make([][]Access, len(s.Reads)),
		Closed:    append([]bool(nil), s.Closed...),
	}
	for i, msg := range s.Msg {
		c.Msg[i] = append([]int(nil), msg...)
	}
	for i, vc := range s.MutexVC {
		c.MutexVC[i] = append([]int(nil), vc...)
	}
	for i, vc := range s.VC {
		c.VC[i] = append([]int(nil), vc...)
	}
	for i, a := range s.LastWrite {
		if a != nil {
			c.LastWrite[i] = &Access{Task: a.Task, VC: append([]int(nil), a.VC...)}
		}
	}
	for i, rs := range s.Reads {
		c.Reads[i] = append([]Access(nil), rs...)
	}
	return c
}

// Key is a canonical encoding of the state, used for merging.
func (s *State) Key() string {
	var b strings.Builder
	for _, v := range s.PCs {
		fmt.Fprintf(&b, "%d,", v)
	}
	b.WriteByte('|')
	for _, v := range s.Buf {
		fmt.Fprintf(&b, "%d,", v)
	}
	b.WriteByte('|')
	for _, msg := range s.Msg {
		for _, v := range msg {
			fmt.Fprintf(&b, "%d,", v)
		}
		b.WriteByte(';')
	}
	b.WriteByte('|')
	for _, v := range s.Lock {
		fmt.Fprintf(&b, "%d,", v)
	}
	b.WriteByte('|')
	for _, vc := range s.MutexVC {
		for _, v := range vc {
			fmt.Fprintf(&b, "%d,", v)
		}
		b.WriteByte(';')
	}
	b.WriteByte('|')
	for _, vc := range s.VC {
		for _, v := range vc {
			fmt.Fprintf(&b, "%d,", v)
		}
		b.WriteByte(';')
	}
	b.WriteByte('|')
	for _, a := range s.LastWrite {
		if a == nil {
			b.WriteString("-;")
			continue
		}
		fmt.Fprintf(&b, "%d:%v:", a.Task, a.Write)
		for _, v := range a.VC {
			fmt.Fprintf(&b, "%d,", v)
		}
		b.WriteByte(';')
	}
	b.WriteByte('|')
	for _, rs := range s.Reads {
		for _, a := range rs {
			fmt.Fprintf(&b, "%d:%v:", a.Task, a.Write)
			for _, v := range a.VC {
				fmt.Fprintf(&b, "%d,", v)
			}
			b.WriteByte('.')
		}
		b.WriteByte(';')
	}
	b.WriteByte('|')
	for _, v := range s.Closed {
		if v {
			b.WriteByte('1')
		} else {
			b.WriteByte('0')
		}
	}
	return b.String()
}

// Trans is one enabled transition.
type Trans struct {
	Task   int
	Label  string
	Res    string // resource key: "ch:x" | "mu:x" | "loc:x" | "spawn:x"
	Step   int
	Kind   model.StepKind
	Next   *State
	Race   bool
	RaceAt string
	Pair   []Access // the two conflicting accesses, when Race
}

func hb(a *Access, vc []int) bool {
	return a.VC[a.Task] <= vc[a.Task]
}

// Enabled returns every transition enabled in s, deterministically ordered.
func (m *Machine) Enabled(s *State) []Trans {
	var out []Trans
	for t := 0; t < m.N; t++ {
		pc := s.PCs[t]
		if pc < 0 || pc >= len(m.Prog.Tasks[t].Steps) {
			continue
		}
		st := m.Prog.Tasks[t].Steps[pc]
		switch st.Kind {
		case model.Spawn:
			n := s.clone()
			child := st.Task
			n.PCs[child] = 0
			n.VC[t][t]++
			n.VC[child] = append([]int(nil), n.VC[t]...)
			n.VC[child][child] = 1
			n.PCs[t]++
			out = append(out, Trans{Task: t, Step: pc, Kind: st.Kind, Label: st.Label,
				Res: "spawn:" + m.Prog.Tasks[child].Name, Next: n})
		case model.Send:
			ci := m.ChanIdx[st.Chan]
			cap := m.Prog.ChanCap[st.Chan]
			if cap == 0 {
				for u := 0; u < m.N; u++ {
					if u == t || s.PCs[u] < 0 || s.PCs[u] >= len(m.Prog.Tasks[u].Steps) {
						continue
					}
					ru := m.Prog.Tasks[u].Steps[s.PCs[u]]
					if ru.Kind != model.Recv || ru.Chan != st.Chan {
						continue
					}
					n := s.clone()
					n.VC[t][t]++
					n.VC[u] = vmax(n.VC[u], n.VC[t])
					n.VC[u][u]++
					n.PCs[t]++
					n.PCs[u]++
					out = append(out, Trans{Task: t, Step: pc, Kind: st.Kind,
						Label: st.Label + " <> " + ru.Label, Res: "ch:" + st.Chan, Next: n})
				}
				continue
			}
			if s.Buf[ci] >= cap {
				continue
			}
			n := s.clone()
			n.VC[t][t]++
			n.Buf[ci]++
			n.Msg[ci] = append(n.Msg[ci], append([]int(nil), n.VC[t]...)...)
			n.PCs[t]++
			out = append(out, Trans{Task: t, Step: pc, Kind: st.Kind, Label: st.Label,
				Res: "ch:" + st.Chan, Next: n})
		case model.Recv:
			ci := m.ChanIdx[st.Chan]
			cap := m.Prog.ChanCap[st.Chan]
			if cap == 0 {
				continue
			}
			if s.Buf[ci] == 0 {
				continue
			}
			n := s.clone()
			msg := n.Msg[ci][:m.N]
			n.Msg[ci] = append([]int(nil), n.Msg[ci][m.N:]...)
			n.VC[t] = vmax(n.VC[t], msg)
			n.VC[t][t]++
			n.Buf[ci]--
			n.PCs[t]++
			out = append(out, Trans{Task: t, Step: pc, Kind: st.Kind, Label: st.Label,
				Res: "ch:" + st.Chan, Next: n})
		case model.Lock:
			mi := m.MutexIdx[st.Mutex]
			if s.Lock[mi] != -1 {
				continue
			}
			n := s.clone()
			n.VC[t][t]++
			n.VC[t] = vmax(n.VC[t], n.MutexVC[mi])
			n.Lock[mi] = t
			n.PCs[t]++
			out = append(out, Trans{Task: t, Step: pc, Kind: st.Kind, Label: st.Label,
				Res: "mu:" + st.Mutex, Next: n})
		case model.Unlock:
			mi := m.MutexIdx[st.Mutex]
			if s.Lock[mi] != t {
				continue
			}
			n := s.clone()
			n.VC[t][t]++
			n.Lock[mi] = -1
			n.MutexVC[mi] = append([]int(nil), n.VC[t]...)
			n.PCs[t]++
			out = append(out, Trans{Task: t, Step: pc, Kind: st.Kind, Label: st.Label,
				Res: "mu:" + st.Mutex, Next: n})
		case model.Close:
			ci := m.ChanIdx[st.Chan]
			n := s.clone()
			n.VC[t][t]++
			n.Closed[ci] = true
			n.PCs[t]++
			out = append(out, Trans{Task: t, Step: pc, Kind: st.Kind, Label: st.Label,
				Res: "ch:" + st.Chan, Next: n})
		case model.Write:
			li := m.LocIdx[st.Loc]
			n := s.clone()
			n.VC[t][t]++
			race := false
			var pair []Access
			cur := Access{Task: t, Write: true, VC: append([]int(nil), n.VC[t]...)}
			if a := n.LastWrite[li]; a != nil && a.Task != t && !hb(a, n.VC[t]) {
				race = true
				pair = []Access{*a, cur}
			}
			for _, a := range n.Reads[li] {
				if a.Task != t && !hb(&a, n.VC[t]) {
					race = true
					pair = []Access{a, cur}
				}
			}
			n.LastWrite[li] = &cur
			n.Reads[li] = nil
			n.PCs[t]++
			out = append(out, Trans{Task: t, Step: pc, Kind: st.Kind, Label: st.Label,
				Res: "loc:" + st.Loc, Next: n, Race: race, RaceAt: st.Loc, Pair: pair})
		case model.Read:
			li := m.LocIdx[st.Loc]
			n := s.clone()
			n.VC[t][t]++
			race := false
			var pair []Access
			cur := Access{Task: t, VC: append([]int(nil), n.VC[t]...)}
			if a := n.LastWrite[li]; a != nil && a.Task != t && !hb(a, n.VC[t]) {
				race = true
				pair = []Access{*a, cur}
			}
			n.Reads[li] = append(n.Reads[li], cur)
			n.PCs[t]++
			out = append(out, Trans{Task: t, Step: pc, Kind: st.Kind, Label: st.Label,
				Res: "loc:" + st.Loc, Next: n, Race: race, RaceAt: st.Loc, Pair: pair})
		case model.Nop:
			n := s.clone()
			n.VC[t][t]++
			n.PCs[t]++
			out = append(out, Trans{Task: t, Step: pc, Kind: st.Kind, Label: st.Label, Next: n})
		}
	}
	return out
}

func vmax(a, b []int) []int {
	for i := range a {
		if b[i] > a[i] {
			a[i] = b[i]
		}
	}
	return a
}

// Independent reports whether two transitions commute for a safety check.
func Independent(a, b Trans) bool {
	if a.Task == b.Task || a.Res == "" || b.Res == "" {
		return false
	}
	if strings.HasPrefix(a.Res, "spawn:") || strings.HasPrefix(b.Res, "spawn:") {
		return false
	}
	return a.Res != b.Res
}

// Edge is one explored transition.
type Edge struct {
	From  int
	To    int
	Task  int
	Label string
}

// Pruned is a transition skipped by the persistent-set reduction.
type Pruned struct {
	From    int
	Task    int
	Label   string
	Witness string
}

// Result is the enumeration outcome.
type Result struct {
	States     []*State
	Edges      []Edge
	Pruned     []Pruned
	Race       bool
	RaceState  int
	RaceLoc    string
	RacePath   []Edge
	RacePair   []Access
	Parent     []int
	ParentEdge []Edge
	PORUsed    bool
}

// Enumerate explores the reachable graph. When por is true it uses a
// persistent-set reduction; the pruned transitions are recorded with a
// commutativity witness.
func (m *Machine) Enumerate(por bool, budget int) (*Result, error) {
	usePOR := por
	for _, c := range m.Prog.ChanCap {
		if c == 0 {
			usePOR = false // rendezvous joint transitions are dependent on all
		}
	}
	if len(m.Prog.Outside) > 0 {
		return nil, fmt.Errorf("outside bound: %v", m.Prog.Outside)
	}

	r := &Result{PORUsed: usePOR}
	init := m.Initial()
	idx := map[string]int{init.Key(): 0}
	r.States = append(r.States, init)
	r.Parent = append(r.Parent, -1)
	r.ParentEdge = append(r.ParentEdge, Edge{})
	queue := []int{0}

	for len(queue) > 0 {
		sid := queue[0]
		queue = queue[1:]
		s := r.States[sid]
		trans := m.Enabled(s)
		if len(trans) == 0 {
			continue
		}
		explore := trans
		if usePOR {
			persistent := PickPersistent(trans)
			explore = nil
			for _, tr := range trans {
				if tr.Task == persistent.Task || !Independent(persistent, tr) {
					explore = append(explore, tr)
				} else {
					r.Pruned = append(r.Pruned, Pruned{
						From: sid, Task: tr.Task, Label: tr.Label,
						Witness: fmt.Sprintf("independent of T%d:%s", persistent.Task, persistent.Label),
					})
				}
			}
		}
		for _, tr := range explore {
			if tr.Race && !r.Race {
				r.Race = true
				r.RaceState = sid
				r.RaceLoc = tr.RaceAt
				r.RacePair = tr.Pair
			}
			key := tr.Next.Key()
			nid, ok := idx[key]
			if !ok {
				if len(r.States) >= budget {
					return nil, fmt.Errorf("state budget exceeded (>%d)", budget)
				}
				nid = len(r.States)
				idx[key] = nid
				r.States = append(r.States, tr.Next)
				r.Parent = append(r.Parent, sid)
				r.ParentEdge = append(r.ParentEdge, Edge{From: sid, To: nid, Task: tr.Task, Label: tr.Label})
				queue = append(queue, nid)
			}
			r.Edges = append(r.Edges, Edge{From: sid, To: nid, Task: tr.Task, Label: tr.Label})
		}
	}
	if r.Race {
		r.RacePath = pathTo(r, r.RaceState)
	}
	return r, nil
}

// pickPersistent chooses the enabled transition with the smallest dependent
// set, so the persistent set prunes as much as possible. Any choice is a
// valid persistent set; this one just reduces more.
func PickPersistent(trans []Trans) Trans {
	best := trans[0]
	bestDeps := len(trans)
	for _, cand := range trans {
		deps := 0
		for _, tr := range trans {
			if tr.Task == cand.Task || !Independent(cand, tr) {
				deps++
			}
		}
		if deps < bestDeps {
			best, bestDeps = cand, deps
		}
	}
	return best
}

func pathTo(r *Result, id int) []Edge {
	var rev []Edge
	for id > 0 {
		e := r.ParentEdge[id]
		rev = append(rev, e)
		id = r.Parent[id]
	}
	for i, j := 0, len(rev)-1; i < j; i, j = i+1, j-1 {
		rev[i], rev[j] = rev[j], rev[i]
	}
	return rev
}

package cert

import (
	"fmt"
	"sort"
	"strings"

	"hybrid.local/w4176gooracle/internal/lts"
	"hybrid.local/w4176gooracle/internal/model"
)

// Verify replays a certificate against the emitted Go it claims to describe.
// It returns the failure codes (empty = accepted).
func Verify(c *Cert, goPath string) []string {
	var fail []string
	add := func(code, msg string) { fail = append(fail, code+": "+msg) }

	// E001 schema
	if c.Schema != "hcert-v2" {
		add("HYB-CERT-E001", "unsupported schema "+c.Schema)
		return fail
	}

	// E002 program binding: the certificate must name the bytes it describes
	sha, err := sha256File(goPath)
	if err != nil {
		add("HYB-CERT-E002", err.Error())
		return fail
	}
	if sha != c.ProgramSHA {
		add("HYB-CERT-E002", fmt.Sprintf("program_sha256 mismatch: cert %s, file %s", c.ProgramSHA, sha))
		return fail
	}

	// The verifier re-extracts the model from the Go file, so a tampered
	// model cannot ride along inside the certificate.
	p, err := model.Extract(goPath, sha)
	if err != nil {
		add("HYB-CERT-E003", "re-extraction failed: "+err.Error())
		return fail
	}
	if p.ModelHash() != c.ModelHash {
		add("HYB-CERT-E003", "model_hash mismatch: certificate model is not the model of the supplied Go")
		return fail
	}
	cp := JSONToModel(c.Model)
	m := lts.NewMachine(cp)
	nch, nmu, nloc := len(m.ChanName), len(m.MutexNam), len(m.LocName)

	// build the state table
	states := make(map[int]*lts.State, len(c.States))
	keys := make(map[string]int, len(c.States))
	for _, sj := range c.States {
		s := stateFromJSON(sj, m.N, nch, nmu, nloc)
		states[sj.ID] = s
		keys[s.Key()] = sj.ID
	}

	// E003 initial state
	init := m.Initial()
	s0, ok := states[0]
	if !ok {
		add("HYB-CERT-E003", "missing initial state s0")
	} else if s0.Key() != init.Key() {
		add("HYB-CERT-E003", "s0 is not the initial state of the model")
	}

	// E004 invariant at every listed state
	for id, s := range states {
		if loc, tasks, bad := violation(s, m); bad {
			add("HYB-CERT-E004", fmt.Sprintf("I_DRF violated at state %d on %s by tasks %v", id, loc, tasks))
		}
	}

	// E005 every listed edge is a legal transition with the right successor
	edgeSet := map[string]bool{}
	for _, e := range c.Edges {
		edgeSet[fmt.Sprintf("%d>%d:%s", e.From, e.To, e.Label)] = true
	}
	for _, e := range c.Edges {
		from, ok := states[e.From]
		if !ok {
			add("HYB-CERT-E005", fmt.Sprintf("edge from unknown state %d", e.From))
			continue
		}
		legal := false
		for _, tr := range m.Enabled(from) {
			if tr.Label == e.Label {
				if to, ok := states[e.To]; ok && tr.Next.Key() == to.Key() {
					legal = true
					break
				}
			}
		}
		if !legal {
			add("HYB-CERT-E005", fmt.Sprintf("edge %d->%d (%s) is not a legal transition", e.From, e.To, e.Label))
		}
	}

	if c.Verdict == "CERTIFIED" {
		// E006 successor closure: every enabled transition that the POR did
		// not prune must have its successor in the certificate.
		pruned := map[string]bool{}
		if c.POR != nil {
			for _, pr := range c.POR.Pruned {
				pruned[fmt.Sprintf("%d/%d/%s", pr.From, pr.Task, pr.Label)] = true
			}
		}
		for id, s := range states {
			for _, tr := range m.Enabled(s) {
				if pruned[fmt.Sprintf("%d/%d/%s", id, tr.Task, tr.Label)] {
					continue
				}
				to, ok := keys[tr.Next.Key()]
				if !ok {
					add("HYB-CERT-E006", fmt.Sprintf("successor of state %d via T%d:%s is missing (state omitted)", id, tr.Task, tr.Label))
				} else if !edgeSet[fmt.Sprintf("%d>%d:%s", id, to, tr.Label)] {
					add("HYB-CERT-E006", fmt.Sprintf("successor of state %d via T%d:%s is present but its edge was omitted", id, tr.Task, tr.Label))
				}
			}
		}

		// E007 reachability
		adj := map[int][]int{}
		for _, e := range c.Edges {
			adj[e.From] = append(adj[e.From], e.To)
		}
		seen := map[int]bool{0: true}
		queue := []int{0}
		for len(queue) > 0 {
			cur := queue[0]
			queue = queue[1:]
			for _, nx := range adj[cur] {
				if !seen[nx] {
					seen[nx] = true
					queue = append(queue, nx)
				}
			}
		}
		if len(seen) != len(states) {
			add("HYB-CERT-E007", fmt.Sprintf("reachable %d of %d listed states", len(seen), len(states)))
		}

		// E009 POR soundness: recompute the persistent set and compare
		if c.POR != nil && c.POR.Used {
			for id, s := range states {
				trans := m.Enabled(s)
				if len(trans) == 0 {
					continue
				}
				persistent := lts.PickPersistent(trans)
				var prunedHere []string
				for _, tr := range trans {
					if tr.Task == persistent.Task || !lts.Independent(persistent, tr) {
						continue
					}
					prunedHere = append(prunedHere, fmt.Sprintf("%d/%d/%s", id, tr.Task, tr.Label))
					if !lts.Independent(persistent, tr) {
						add("HYB-CERT-E009", fmt.Sprintf("pruned transition at %d is not independent", id))
					}
				}
				sort.Strings(prunedHere)
				var certHere []string
				for _, pr := range c.POR.Pruned {
					if pr.From == id {
						certHere = append(certHere, fmt.Sprintf("%d/%d/%s", id, pr.Task, pr.Label))
					}
				}
				sort.Strings(certHere)
				if strings.Join(prunedHere, "|") != strings.Join(certHere, "|") {
					add("HYB-CERT-E009", fmt.Sprintf("pruned set at state %d does not match the recomputed persistent set (want %v got %v persistent=%d/%s)", id, prunedHere, certHere, persistent.Task, persistent.Label))
				}
			}
		}
	}

	if c.Verdict == "RACE_DETECTED" {
		// replay the witness path
		if len(c.Edges) == 0 || len(c.States) == 0 {
			add("HYB-CERT-E010", "empty witness path")
		} else {
			byFrom := map[int]EdgeJSON{}
			for _, e := range c.Edges {
				byFrom[e.From] = e
			}
			cur := 0
			for i := 0; i < len(c.Edges); i++ {
				e, ok := byFrom[cur]
				if !ok {
					add("HYB-CERT-E010", fmt.Sprintf("witness path breaks at state %d", cur))
					break
				}
				from := states[e.From]
				to := states[e.To]
				legal := false
				for _, tr := range m.Enabled(from) {
					if tr.Label == e.Label && tr.Next.Key() == to.Key() {
						legal = true
						break
					}
				}
				if !legal {
					add("HYB-CERT-E010", fmt.Sprintf("witness step %d->%d (%s) is not legal", e.From, e.To, e.Label))
				}
				cur = e.To
			}
			if c.Race == nil || len(c.Race.Accesses) != 2 {
				add("HYB-CERT-E010", "witness carries no conflicting access pair")
			} else {
				a := lts.Access{Task: c.Race.Accesses[0].Task, Write: c.Race.Accesses[0].Write, VC: c.Race.Accesses[0].VC}
				b := lts.Access{Task: c.Race.Accesses[1].Task, Write: c.Race.Accesses[1].Write, VC: c.Race.Accesses[1].VC}
				if a.Task == b.Task {
					add("HYB-CERT-E010", "conflicting pair is from one task")
				} else if !a.Write && !b.Write {
					add("HYB-CERT-E010", "conflicting pair is read/read")
				} else if ordered(&a, &b) || ordered(&b, &a) {
					add("HYB-CERT-E010", "conflicting pair is happens-before ordered")
				}
			}
		}
	}

	// E008 digest
	if digest(c) != c.Digest {
		add("HYB-CERT-E008", "digest mismatch")
	}
	return fail
}

// violation finds an unsynchronised conflicting pair of accesses.
func violation(s *lts.State, m *lts.Machine) (string, []int, bool) {
	for li := 0; li < len(m.LocName); li++ {
		var accs []*lts.Access
		var isWrite []bool
		if w := s.LastWrite[li]; w != nil {
			accs = append(accs, w)
			isWrite = append(isWrite, true)
		}
		for i := range s.Reads[li] {
			a := s.Reads[li][i]
			accs = append(accs, &a)
			isWrite = append(isWrite, false)
		}
		for i := 0; i < len(accs); i++ {
			for j := i + 1; j < len(accs); j++ {
				if accs[i].Task == accs[j].Task {
					continue
				}
				if !isWrite[i] && !isWrite[j] {
					continue
				}
				if ordered(accs[i], accs[j]) || ordered(accs[j], accs[i]) {
					continue
				}
				return m.LocName[li], []int{accs[i].Task, accs[j].Task}, true
			}
		}
	}
	return "", nil, false
}

func ordered(a, b *lts.Access) bool {
	return a.VC[a.Task] <= b.VC[a.Task]
}

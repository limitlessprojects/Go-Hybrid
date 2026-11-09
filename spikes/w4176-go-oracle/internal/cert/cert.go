// Package cert defines the hcert-v2 artifact, its emitter and its
// independent replay verifier.
package cert

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"sort"

	"hybrid.local/w4176gooracle/internal/lts"
	"hybrid.local/w4176gooracle/internal/model"
)

// AccessJSON mirrors lts.Access.
type AccessJSON struct {
	Task  int   `json:"task"`
	Write bool  `json:"write"`
	VC    []int `json:"vc"`
}

// StateJSON is one certificate state.
type StateJSON struct {
	ID        int          `json:"id"`
	PCs       []int        `json:"pcs"`
	Buf       []int        `json:"buffers"`
	Msg       [][]int      `json:"msg,omitempty"`
	Lock      []int        `json:"locks"`
	MutexVC   [][]int      `json:"mutex_vc,omitempty"`
	VC        [][]int      `json:"vcs"`
	LastWrite []*AccessJSON `json:"last_write,omitempty"`
	Reads     [][]AccessJSON `json:"reads,omitempty"`
	Closed    []bool       `json:"closed,omitempty"`
}

// StepJSON mirrors model.Step.
type StepJSON struct {
	Kind  int    `json:"kind"`
	Task  int    `json:"task"`
	Chan  string `json:"chan,omitempty"`
	Mutex string `json:"mutex,omitempty"`
	Loc   string `json:"loc,omitempty"`
	Label string `json:"label"`
}

// TaskJSON mirrors model.Task.
type TaskJSON struct {
	ID    int        `json:"id"`
	Name  string     `json:"name"`
	Steps []StepJSON `json:"steps"`
}

// ModelJSON is the serialised extracted model.
type ModelJSON struct {
	Tasks   []TaskJSON     `json:"tasks"`
	ChanCap map[string]int `json:"chan_cap"`
	Mutexes []string       `json:"mutexes"`
	Locs    []string       `json:"locs"`
}

// AdmissionJSON records the measured admission numbers.
type AdmissionJSON struct {
	NTasks      int    `json:"n_tasks"`
	SumChanCap  int    `json:"sum_chan_depth"`
	Tier        string `json:"tier"`
	TaskBound   int    `json:"task_bound"`
	ChanBound   int    `json:"chan_bound"`
	StateBudget int    `json:"state_budget"`
}

// EdgeJSON is one certificate edge.
type EdgeJSON struct {
	From  int    `json:"from"`
	To    int    `json:"to"`
	Task  int    `json:"task"`
	Label string `json:"label"`
}

// PrunedJSON is one POR-pruned transition with its witness.
type PrunedJSON struct {
	From    int    `json:"from"`
	Task    int    `json:"task"`
	Label   string `json:"label"`
	Witness string `json:"witness"`
}

// PORJSON is the reduction record.
type PORJSON struct {
	Used       bool         `json:"used"`
	FullStates int          `json:"full_states"`
	PORStates  int          `json:"por_states"`
	Pruned     []PrunedJSON `json:"pruned"`
}

// RaceJSON is the witness for a RACE_DETECTED certificate.
type RaceJSON struct {
	State    int          `json:"state"`
	Loc      string       `json:"loc"`
	Accesses []AccessJSON `json:"accesses"`
}

// Cert is the hcert-v2 artifact.
type Cert struct {
	Schema     string        `json:"schema"`
	Program    string        `json:"program"`
	ProgramSHA string        `json:"program_sha256"`
	ModelHash  string        `json:"model_hash"`
	Admission  AdmissionJSON `json:"admission"`
	Model      ModelJSON     `json:"model"`
	Verdict    string        `json:"verdict"`
	States     []StateJSON   `json:"states"`
	Edges      []EdgeJSON   `json:"edges"`
	Race       *RaceJSON     `json:"race,omitempty"`
	POR        *PORJSON      `json:"por,omitempty"`
	Digest     string        `json:"digest"`
}

// ModelToJSON serialises an extracted model.
func ModelToJSON(p *model.Program) ModelJSON {
	mj := ModelJSON{ChanCap: map[string]int{}, Mutexes: p.Mutexes, Locs: p.Locs}
	for _, t := range p.Tasks {
		tj := TaskJSON{ID: t.ID, Name: t.Name}
		for _, s := range t.Steps {
			tj.Steps = append(tj.Steps, StepJSON{
				Kind: int(s.Kind), Task: s.Task, Chan: s.Chan,
				Mutex: s.Mutex, Loc: s.Loc, Label: s.Label,
			})
		}
		mj.Tasks = append(mj.Tasks, tj)
	}
	for k, v := range p.ChanCap {
		mj.ChanCap[k] = v
	}
	return mj
}

// JSONToModel reconstructs a model from a certificate.
func JSONToModel(mj ModelJSON) *model.Program {
	p := &model.Program{
		ChanCap: map[string]int{},
		Mutexes: mj.Mutexes,
		Locs:    mj.Locs,
	}
	for _, tj := range mj.Tasks {
		t := model.Task{ID: tj.ID, Name: tj.Name}
		for _, s := range tj.Steps {
			t.Steps = append(t.Steps, model.Step{
				Kind: model.StepKind(s.Kind), Task: s.Task, Chan: s.Chan,
				Mutex: s.Mutex, Loc: s.Loc, Label: s.Label,
			})
		}
		p.Tasks = append(p.Tasks, t)
	}
	for k, v := range mj.ChanCap {
		p.ChanCap[k] = v
	}
	p.NTasks = len(p.Tasks)
	for _, c := range p.ChanCap {
		p.SumChanCap += c
	}
	return p
}

// Emit builds a certificate from an enumeration result.
func Emit(p *model.Program, m *lts.Machine, full, reduced *lts.Result) *Cert {
	c := &Cert{
		Schema:     "hcert-v2",
		Program:    p.Name,
		ProgramSHA: p.GoSHA,
		ModelHash:  p.ModelHash(),
		Admission: AdmissionJSON{
			NTasks: p.NTasks, SumChanCap: p.SumChanCap, Tier: p.Tier,
			TaskBound: 4, ChanBound: 8, StateBudget: 100000,
		},
		Model: ModelToJSON(p),
	}
	if reduced.Race {
		c.Verdict = "RACE_DETECTED"
		// the certificate is the witness schedule: s0 .. s_race
		path := reduced.RacePath
		ids := []int{0}
		for _, e := range path {
			ids = append(ids, e.To)
		}
		for i, id := range ids {
			sj := stateToJSON(reduced.States[id])
			sj.ID = i
			c.States = append(c.States, sj)
		}
		for i := 1; i < len(ids); i++ {
			e := path[i-1]
			c.Edges = append(c.Edges, EdgeJSON{From: i - 1, To: i, Task: e.Task, Label: e.Label})
		}
		c.Race = &RaceJSON{State: len(ids) - 1, Loc: reduced.RaceLoc}
		for _, a := range reduced.RacePair {
			c.Race.Accesses = append(c.Race.Accesses, AccessJSON{Task: a.Task, Write: a.Write, VC: a.VC})
		}
	} else {
		c.Verdict = "CERTIFIED"
		for i, st := range reduced.States {
			sj := stateToJSON(st)
			sj.ID = i
			c.States = append(c.States, sj)
		}
		for _, e := range reduced.Edges {
			c.Edges = append(c.Edges, EdgeJSON{From: e.From, To: e.To, Task: e.Task, Label: e.Label})
		}
		c.Edges = dedupe(c.Edges)
		pj := &PORJSON{Used: reduced.PORUsed, FullStates: len(full.States), PORStates: len(reduced.States)}
		for _, pr := range reduced.Pruned {
			pj.Pruned = append(pj.Pruned, PrunedJSON{From: pr.From, Task: pr.Task, Label: pr.Label, Witness: pr.Witness})
		}
		c.POR = pj
	}
	c.Digest = digest(c)
	return c
}

func dedupe(in []EdgeJSON) []EdgeJSON {
	seen := map[string]bool{}
	var out []EdgeJSON
	for _, e := range in {
		k := fmt.Sprintf("%d/%d/%d/%s", e.From, e.To, e.Task, e.Label)
		if seen[k] {
			continue
		}
		seen[k] = true
		out = append(out, e)
	}
	sort.Slice(out, func(i, j int) bool {
		if out[i].From != out[j].From {
			return out[i].From < out[j].From
		}
		if out[i].To != out[j].To {
			return out[i].To < out[j].To
		}
		return out[i].Label < out[j].Label
	})
	return out
}

func stateToJSON(s *lts.State) StateJSON {
	j := StateJSON{
		PCs: s.PCs, Buf: s.Buf, Lock: s.Lock, VC: s.VC, Closed: s.Closed,
	}
	for _, msg := range s.Msg {
		j.Msg = append(j.Msg, append([]int(nil), msg...))
	}
	for _, vc := range s.MutexVC {
		j.MutexVC = append(j.MutexVC, append([]int(nil), vc...))
	}
	for _, a := range s.LastWrite {
		if a == nil {
			j.LastWrite = append(j.LastWrite, nil)
		} else {
			j.LastWrite = append(j.LastWrite, &AccessJSON{Task: a.Task, Write: a.Write, VC: a.VC})
		}
	}
	for _, rs := range s.Reads {
		var row []AccessJSON
		for _, a := range rs {
			row = append(row, AccessJSON{Task: a.Task, Write: a.Write, VC: a.VC})
		}
		j.Reads = append(j.Reads, row)
	}
	return j
}

func stateFromJSON(j StateJSON, n int, nch, nmu, nloc int) *lts.State {
	s := &lts.State{
		PCs: append([]int(nil), j.PCs...), Buf: append([]int(nil), j.Buf...),
		Lock: append([]int(nil), j.Lock...), VC: make([][]int, n),
		MutexVC: make([][]int, nmu), LastWrite: make([]*lts.Access, nloc),
		Reads: make([][]lts.Access, nloc), Closed: append([]bool(nil), j.Closed...),
	}
	for i := 0; i < n; i++ {
		s.VC[i] = append([]int(nil), j.VC[i]...)
	}
	for len(s.PCs) < n {
		s.PCs = append(s.PCs, -1)
	}
	for len(s.Closed) < nch {
		s.Closed = append(s.Closed, false)
	}
	for len(s.LastWrite) < nloc {
		s.LastWrite = append(s.LastWrite, nil)
	}
	for len(s.Reads) < nloc {
		s.Reads = append(s.Reads, nil)
	}
	for len(s.MutexVC) < nmu {
		s.MutexVC = append(s.MutexVC, make([]int, n))
	}
	for i := 0; i < nmu; i++ {
		s.MutexVC[i] = append([]int(nil), j.MutexVC[i]...)
	}
	s.Msg = make([][]int, nch)
	for i := 0; i < nch && i < len(j.Msg); i++ {
		s.Msg[i] = append([]int(nil), j.Msg[i]...)
	}
	for i := 0; i < nloc && i < len(j.LastWrite); i++ {
		if j.LastWrite[i] != nil {
			s.LastWrite[i] = &lts.Access{Task: j.LastWrite[i].Task, Write: j.LastWrite[i].Write, VC: append([]int(nil), j.LastWrite[i].VC...)}
		}
	}
	for i := 0; i < nloc && i < len(j.Reads); i++ {
		for _, a := range j.Reads[i] {
			s.Reads[i] = append(s.Reads[i], lts.Access{Task: a.Task, Write: a.Write, VC: append([]int(nil), a.VC...)})
		}
	}
	return s
}

func digest(c *Cert) string {
	cp := *c
	cp.Digest = ""
	b, _ := json.Marshal(cp)
	sum := sha256.Sum256(b)
	return hex.EncodeToString(sum[:])
}

// Save writes the certificate.
func (c *Cert) Save(path string) error {
	b, err := json.MarshalIndent(c, "", " ")
	if err != nil {
		return err
	}
	return os.WriteFile(path, append(b, '\n'), 0o644)
}

// Load reads a certificate.
func Load(path string) (*Cert, error) {
	b, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	var c Cert
	if err := json.Unmarshal(b, &c); err != nil {
		return nil, err
	}
	return &c, nil
}

func sha256File(path string) (string, error) {
	b, err := os.ReadFile(path)
	if err != nil {
		return "", err
	}
	sum := sha256.Sum256(b)
	return hex.EncodeToString(sum[:]), nil
}

// SHA256Bytes hashes raw program bytes.
func SHA256Bytes(b []byte) string {
	sum := sha256.Sum256(b)
	return hex.EncodeToString(sum[:])
}

// RecomputeDigest refreshes the digest after a deliberate mutation, so a
// tamper test exercises the structural checks rather than the digest check.
func (c *Cert) RecomputeDigest() {
	c.Digest = digest(c)
}

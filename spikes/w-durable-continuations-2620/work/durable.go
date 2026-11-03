package durable

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
)

const (
	ExpectedBuildID       = "hyb-w2620-build-v1"
	ExpectedSchemaVersion = 1
)

var (
	ErrBuildIdentityMismatch = errors.New("HYB-R3620-BUILD: snapshot build_id or schema_version mismatch")
	ErrCorruptSnapshot       = errors.New("HYB-R3620-DIGEST: snapshot integrity digest mismatch")
	ErrTornCut               = errors.New("HYB-R3620-TORN: inconsistent cut (produced != in_flight + consumed)")
	ErrInvalidContinuation   = errors.New("HYB-R3620-CONT: continuation PC or blocked channel wait-state invalid")
	ErrMidWriteCrash         = errors.New("HYB-R3620-IO: simulated process termination mid-commit before atomic seal")
)

type TaskCont struct {
	ID        string         `json:"id"`
	Kind      string         `json:"kind"`
	PC        int            `json:"pc"`
	MaxPC     int            `json:"max_pc"`
	Locals    map[string]int `json:"locals"`
	BlockedOn string         `json:"blocked_on"`
	Done      bool           `json:"done"`
}
type ChanQueue struct {
	Name   string `json:"name"`
	Cap    int    `json:"cap"`
	Items  []int  `json:"items"`
	Closed bool   `json:"closed"`
}
type RegionSnapshot struct {
	RegionID        string               `json:"region_id"`
	BuildID         string               `json:"build_id"`
	SchemaVersion   int                  `json:"schema_version"`
	CommitSeq       int                  `json:"commit_seq"`
	SchedulerCursor int                  `json:"scheduler_cursor"`
	Tasks           []TaskCont           `json:"tasks"`
	Queues          map[string]ChanQueue `json:"queues"`
	Result          int                  `json:"result"`
	Completed       bool                 `json:"completed"`
	Digest          string               `json:"digest"`
}
func ComputeDigest(s RegionSnapshot) string {
	s.Digest = ""
	raw, _ := json.Marshal(s)
	sum := sha256.Sum256(raw)
	return hex.EncodeToString(sum[:])
}
func CloneSnapshot(s RegionSnapshot) RegionSnapshot {
	raw, _ := json.Marshal(s)
	var out RegionSnapshot
	_ = json.Unmarshal(raw, &out)
	return out
}

func ValidateSnapshot(s RegionSnapshot) error {
	if s.BuildID != ExpectedBuildID || s.SchemaVersion != ExpectedSchemaVersion {
		return ErrBuildIdentityMismatch
	}
	if s.Digest == "" || ComputeDigest(s) != s.Digest {
		return ErrCorruptSnapshot
	}
	produced, consumed, inFlight := 0, 0, 0
	for _, t := range s.Tasks {
		if t.PC < 0 || t.PC > t.MaxPC || (t.BlockedOn != "" && s.Queues[t.BlockedOn].Name == "") {
			return fmt.Errorf("%w: task %s pc=%d", ErrInvalidContinuation, t.ID, t.PC)
		}
		produced += t.Locals["emitted"]
		consumed += t.Locals["consumed"]
	}
	for _, q := range s.Queues {
		if len(q.Items) > q.Cap {
			return fmt.Errorf("%w: queue %s overflow", ErrInvalidContinuation, q.Name)
		}
		inFlight += len(q.Items)
	}
	if produced != inFlight+consumed {
		return fmt.Errorf("%w: %d != %d + %d", ErrTornCut, produced, inFlight, consumed)
	}
	return nil
}

type AtomicSnapshotStore struct {
	committed *RegionSnapshot
	staged    []byte
}
func (st *AtomicSnapshotStore) CommitAtomic(s RegionSnapshot, crashMidWrite bool) error {
	s.Digest = ComputeDigest(s)
	st.staged, _ = json.Marshal(s)
	if crashMidWrite {
		return ErrMidWriteCrash
	}
	cp := CloneSnapshot(s)
	st.committed, st.staged = &cp, nil
	return nil
}
func (st *AtomicSnapshotStore) LoadLatest() (*RegionSnapshot, error) {
	if st.committed == nil {
		return nil, nil
	}
	if err := ValidateSnapshot(*st.committed); err != nil {
		return nil, err
	}
	cp := CloneSnapshot(*st.committed)
	return &cp, nil
}

func NewPTRRegion(n, qCap int) RegionSnapshot {
	s := RegionSnapshot{
		RegionID: "ptr_v1", BuildID: ExpectedBuildID, SchemaVersion: ExpectedSchemaVersion,
		Tasks: []TaskCont{
			{ID: "producer", Kind: "prod", MaxPC: 2, Locals: map[string]int{"i": 1, "n": n}},
			{ID: "transformer", Kind: "xfm", MaxPC: 2, Locals: map[string]int{}},
			{ID: "reducer", Kind: "red", MaxPC: 2, Locals: map[string]int{}},
		},
		Queues: map[string]ChanQueue{"raw_q": {Name: "raw_q", Cap: qCap, Items: []int{}}, "scaled_q": {Name: "scaled_q", Cap: qCap, Items: []int{}}},
	}
	s.Digest = ComputeDigest(s)
	return s
}

func NewFanOutRegion(n, workers, qCap int) RegionSnapshot {
	tasks := []TaskCont{{ID: "dispatcher", Kind: "disp", MaxPC: 2, Locals: map[string]int{"i": 1, "n": n}}}
	for w := 0; w < workers; w++ {
		tasks = append(tasks, TaskCont{ID: fmt.Sprintf("w_%d", w), Kind: "work", MaxPC: 2, Locals: map[string]int{}})
	}
	tasks = append(tasks, TaskCont{ID: "collector", Kind: "coll", MaxPC: 2, Locals: map[string]int{"rem": n}})
	s := RegionSnapshot{
		RegionID: "fanout_v1", BuildID: ExpectedBuildID, SchemaVersion: ExpectedSchemaVersion, Tasks: tasks,
		Queues: map[string]ChanQueue{"in_q": {Name: "in_q", Cap: qCap, Items: []int{}}, "out_q": {Name: "out_q", Cap: qCap, Items: []int{}}},
	}
	s.Digest = ComputeDigest(s)
	return s
}

func StepOnce(s *RegionSnapshot) bool {
	if s.Completed {
		return false
	}
	for off, n := 0, len(s.Tasks); off < n; off++ {
		idx := (s.SchedulerCursor + off) % n
		if t := &s.Tasks[idx]; !t.Done && stepTask(s, t) {
			s.SchedulerCursor, s.Completed = (idx+1)%n, true
			for i := range s.Tasks {
				s.Completed = s.Completed && s.Tasks[i].Done
			}
			return true
		}
	}
	return false
}

func stepTask(s *RegionSnapshot, t *TaskCont) bool {
	switch t.Kind {
	case "prod", "disp":
		qName := map[string]string{"prod": "raw_q", "disp": "in_q"}[t.Kind]
		q := s.Queues[qName]
		if t.Locals["i"] > t.Locals["n"] {
			q.Closed, t.Done, t.PC, t.BlockedOn = true, true, 2, ""
			s.Queues[qName] = q
			return true
		}
		if len(q.Items) >= q.Cap {
			t.BlockedOn = qName
			return false
		}
		v := t.Locals["i"]
		q.Items = append(q.Items, v*v+3*v)
		s.Queues[qName], t.Locals["i"], t.Locals["emitted"], t.PC, t.BlockedOn = q, v+1, t.Locals["emitted"]+1, 1, ""
		return true
	case "xfm", "work":
		inName, outName := "raw_q", "scaled_q"
		if t.Kind == "work" {
			inName, outName = "in_q", "out_q"
		}
		if t.PC == 0 {
			inQ := s.Queues[inName]
			if len(inQ.Items) == 0 {
				if !inQ.Closed {
					t.BlockedOn = inName
					return false
				}
				if t.Kind == "xfm" {
					outQ := s.Queues[outName]
					outQ.Closed = true
					s.Queues[outName] = outQ
				}
				t.Done, t.PC, t.BlockedOn = true, 2, ""
				return true
			}
			t.Locals["cur"], inQ.Items = inQ.Items[0], inQ.Items[1:]
			s.Queues[inName], t.Locals["consumed"], t.PC, t.BlockedOn = inQ, t.Locals["consumed"]+1, 1, ""
			return true
		}
		outQ := s.Queues[outName]
		if len(outQ.Items) >= outQ.Cap {
			t.BlockedOn = outName
			return false
		}
		outQ.Items = append(outQ.Items, t.Locals["cur"]*5+7)
		s.Queues[outName], t.Locals["emitted"], t.PC, t.BlockedOn = outQ, t.Locals["emitted"]+1, 0, ""
		return true
	case "red", "coll":
		inName := map[string]string{"red": "scaled_q", "coll": "out_q"}[t.Kind]
		inQ := s.Queues[inName]
		if len(inQ.Items) == 0 {
			if (t.Kind == "red" && inQ.Closed) || (t.Kind == "coll" && t.Locals["rem"] == 0) {
				s.Result, t.Done, t.PC, t.BlockedOn = t.Locals["acc"], true, 2, ""
				return true
			}
			t.BlockedOn = inName
			return false
		}
		t.Locals["acc"], inQ.Items, t.Locals["consumed"] = t.Locals["acc"]+inQ.Items[0], inQ.Items[1:], t.Locals["consumed"]+1
		s.Queues[inName], t.PC, t.BlockedOn = inQ, 1, ""
		if t.Kind == "coll" {
			t.Locals["rem"]--
			if t.Locals["rem"] == 0 {
				s.Result, t.Done, t.PC = t.Locals["acc"], true, 2
			}
		}
		return true
	}
	return false
}

// Package w605replayisland implements the M1/M2 feasibility spike for Wildcard #605:
// Replayable Task Islands — compiler-admitted concurrent regions lowered into
// cooperatively scheduled state machines with deterministic record/replay trace
// verification and fail-closed rejection of unmediated nondeterministic effects.
package w605replayisland

import (
	"crypto/sha256"
	"encoding/binary"
	"errors"
	"fmt"
	"sort"
	"strings"
)

var (
	ErrAdmissionRejectedEffect   = errors.New("HYB-E3601: unmediated nondeterministic effect inside replay_island")
	ErrAdmissionManagedBlock     = errors.New("HYB-E3602: managed escape block forbidden inside replay_island")
	ErrAdmissionMapIterUnordered = errors.New("HYB-E3603: unmediated map iteration order inside replay_island")
	ErrAdmissionUninstrumentedCh = errors.New("HYB-E3604: uninstrumented raw channel or goroutine inside replay_island")
	ErrReplayTraceTruncated      = errors.New("HYB-E3605: replay trace truncated before island quiescence")
	ErrReplayVersionMismatch     = errors.New("HYB-E3606: replay trace program/input version mismatch")
	ErrReplayEventIDMismatch     = errors.New("HYB-E3607: replay trace event ID or enabled-set mismatch")
	ErrReplayDigestMismatch      = errors.New("HYB-E3608: replay final state digest mismatch")
)

// AdmissionDiagnostic represents a static rejection emitted by the Phase-1
// `replay_island` admission pass before lowering.
type AdmissionDiagnostic struct {
	Code    string
	Message string
}

// CheckIslandAdmission statically inspects a `.hy` source snippet intended for a
// `replay_island` region and fails closed on any unmediated source of nondeterminism:
//   - `HYB-E3601`: wall-clock reads (`time.now`, `time.since`), RNG (`rand.`),
//     external network/filesystem I/O (`net.`, `os.`, `http.`), or foreign callbacks
//   - `HYB-E3602`: `managed(` escape blocks
//   - `HYB-E3603`: unordered `for k, v := range map_` without deterministic key sort
//   - `HYB-E3604`: raw Go `go func` or unmediated channel ops outside the island coordinator
func CheckIslandAdmission(hySource string) []AdmissionDiagnostic {
	var diags []AdmissionDiagnostic
	lines := strings.Split(hySource, "\n")
	for idx, rawLine := range lines {
		line := strings.TrimSpace(rawLine)
		if line == "" || strings.HasPrefix(line, "//") {
			continue
		}
		lineNo := idx + 1
		if strings.Contains(line, "managed(") {
			diags = append(diags, AdmissionDiagnostic{
				Code:    "HYB-E3602",
				Message: fmt.Sprintf("line %d: managed(...) escape block forbidden inside replay_island", lineNo),
			})
		}
		for _, tok := range []string{"time.now(", "time.since(", "rand.", "net.", "os.", "http.", "foreign_callback("} {
			if strings.Contains(line, tok) {
				diags = append(diags, AdmissionDiagnostic{
					Code:    "HYB-E3601",
					Message: fmt.Sprintf("line %d: unmediated nondeterministic effect %q inside replay_island", lineNo, tok),
				})
			}
		}
		if strings.Contains(line, "range map_") || strings.Contains(line, "/*unordered_map_range*/") {
			diags = append(diags, AdmissionDiagnostic{
				Code:    "HYB-E3603",
				Message: fmt.Sprintf("line %d: unmediated map iteration order inside replay_island", lineNo),
			})
		}
		if strings.Contains(line, "raw_chan_send(") || strings.Contains(line, "go func(") {
			diags = append(diags, AdmissionDiagnostic{
				Code:    "HYB-E3604",
				Message: fmt.Sprintf("line %d: uninstrumented channel/goroutine bypasses island coordinator", lineNo),
			})
		}
	}
	return diags
}

// EventKind identifies the mediated scheduling action in a Replayable Task Island.
type EventKind uint8

const (
	EventChanSend     EventKind = 1
	EventChanRecv     EventKind = 2
	EventSelectArm    EventKind = 3
	EventVirtualTimer EventKind = 4
	EventTaskSpawn    EventKind = 5
)

// TraceEvent is a compact, deterministic 16-byte scheduling decision recorded by
// the island coordinator.
type TraceEvent struct {
	StepIndex  uint32
	EventID    uint32 // stable static site + task + arm identifier
	TaskID     uint16
	ChoiceSlot uint16 // selected arm or enabled index
	VirtualNow uint32 // virtual clock tick at decision point
}

// ReplayTrace binds a sequence of scheduling decisions to a specific program
// build hash, input hash, and final state digest.
type ReplayTrace struct {
	ProgramVersion uint64
	InputHash      uint64
	Events         []TraceEvent
	FinalDigest    [32]byte
}

// EncodeBinary serializes `ReplayTrace` into a compact binary wire representation
// (16 bytes per event + 48-byte header) to measure trace storage cost.
func (t ReplayTrace) EncodeBinary() []byte {
	buf := make([]byte, 48+len(t.Events)*16)
	binary.LittleEndian.PutUint64(buf[0:8], t.ProgramVersion)
	binary.LittleEndian.PutUint64(buf[8:16], t.InputHash)
	copy(buf[16:48], t.FinalDigest[:])
	off := 48
	for _, ev := range t.Events {
		binary.LittleEndian.PutUint32(buf[off+0:off+4], ev.StepIndex)
		binary.LittleEndian.PutUint32(buf[off+4:off+8], ev.EventID)
		binary.LittleEndian.PutUint16(buf[off+8:off+10], ev.TaskID)
		binary.LittleEndian.PutUint16(buf[off+10:off+12], ev.ChoiceSlot)
		binary.LittleEndian.PutUint32(buf[off+12:off+16], ev.VirtualNow)
		off += 16
	}
	return buf
}

// EnabledTransition represents a single enabled state-machine step ready to be
// chosen by the coordinator in either Record (PRNG-seeded) or Replay mode.
type EnabledTransition struct {
	EventID    uint32
	TaskID     uint16
	ChoiceSlot uint16
	Kind       EventKind
	Apply      func(c *IslandCoordinator)
}

// IslandChannel is a coordinator-mediated typed integer channel inside the island.
type IslandChannel struct {
	ID       uint16
	Capacity int
	Queue    []int64
	Closed   bool
}

// VirtualTimer represents a deterministic timer scheduled on the island's virtual clock.
type VirtualTimer struct {
	TimerID  uint16
	WakeTick uint32
	Fired    bool
	Payload  int64
	TargetCh *IslandChannel
}

// IslandTask is a resumable state machine lowered from a Hybrid `task` inside
// a `replay_island` block.
type IslandTask struct {
	TaskID uint16
	Name   string
	PC     int
	Done   bool
	Step   func(c *IslandCoordinator, self *IslandTask) []EnabledTransition
}

// IslandCoordinator drives a closed set of `IslandTask` state machines under
// either deterministic seeded Record mode or strict trace-checked Replay mode.
type IslandCoordinator struct {
	ProgramVersion uint64
	InputHash      uint64
	VirtualNow     uint32
	Tasks          []*IslandTask
	Channels       map[uint16]*IslandChannel
	Timers         []*VirtualTimer
	Outputs        []string
	rngState       uint64
}

func NewCoordinator(progVer, inputHash, seed uint64) *IslandCoordinator {
	if seed == 0 {
		seed = 0x9e3779b97f4a7c15
	}
	return &IslandCoordinator{
		ProgramVersion: progVer,
		InputHash:      inputHash,
		VirtualNow:     0,
		Channels:       make(map[uint16]*IslandChannel),
		rngState:       seed,
	}
}

func (c *IslandCoordinator) NewChannel(id uint16, capacity int) *IslandChannel {
	ch := &IslandChannel{
		ID:       id,
		Capacity: capacity,
		Queue:    make([]int64, 0, capacity),
	}
	c.Channels[id] = ch
	return ch
}

func (c *IslandCoordinator) AddTimer(timerID uint16, wakeTick uint32, payload int64, targetCh *IslandChannel) {
	c.Timers = append(c.Timers, &VirtualTimer{
		TimerID:  timerID,
		WakeTick: wakeTick,
		Payload:  payload,
		TargetCh: targetCh,
	})
}

func (c *IslandCoordinator) AddTask(id uint16, name string, stepFn func(c *IslandCoordinator, self *IslandTask) []EnabledTransition) {
	c.Tasks = append(c.Tasks, &IslandTask{
		TaskID: id,
		Name:   name,
		PC:     0,
		Done:   false,
		Step:   stepFn,
	})
}

func (c *IslandCoordinator) nextRand(n int) int {
	// SplitMix64 deterministic PRNG for controlled schedule exploration.
	c.rngState += 0x9e3779b97f4a7c15
	z := c.rngState
	z = (z ^ (z >> 30)) * 0xbf58476d1ce4e5b9
	z = (z ^ (z >> 27)) * 0x94d049bb133111eb
	z = z ^ (z >> 31)
	return int(z % uint64(n))
}

func (c *IslandCoordinator) collectEnabled() []EnabledTransition {
	var list []EnabledTransition
	for _, t := range c.Tasks {
		if t.Done {
			continue
		}
		list = append(list, t.Step(c, t)...)
	}
	// If no task transition is immediately enabled, check whether a virtual timer
	// can advance `VirtualNow` deterministically.
	if len(list) == 0 {
		var earliest *VirtualTimer
		for _, tm := range c.Timers {
			if !tm.Fired && len(tm.TargetCh.Queue) < tm.TargetCh.Capacity {
				if earliest == nil || tm.WakeTick < earliest.WakeTick || (tm.WakeTick == earliest.WakeTick && tm.TimerID < earliest.TimerID) {
					earliest = tm
				}
			}
		}
		if earliest != nil {
			tm := earliest
			evID := (uint32(EventVirtualTimer) << 24) | (uint32(tm.TimerID) << 8)
			list = append(list, EnabledTransition{
				EventID:    evID,
				TaskID:     0,
				ChoiceSlot: tm.TimerID,
				Kind:       EventVirtualTimer,
				Apply: func(coord *IslandCoordinator) {
					if tm.WakeTick > coord.VirtualNow {
						coord.VirtualNow = tm.WakeTick
					}
					tm.Fired = true
					tm.TargetCh.Queue = append(tm.TargetCh.Queue, tm.Payload)
					coord.Outputs = append(coord.Outputs, fmt.Sprintf("timer[%d]@%d->%d", tm.TimerID, coord.VirtualNow, tm.Payload))
				},
			})
		}
	}
	// Canonical sort by (EventID, TaskID, ChoiceSlot) so the enabled set has a
	// deterministic order independent of Go map or slice construction.
	sort.Slice(list, func(i, j int) bool {
		if list[i].EventID != list[j].EventID {
			return list[i].EventID < list[j].EventID
		}
		if list[i].TaskID != list[j].TaskID {
			return list[i].TaskID < list[j].TaskID
		}
		return list[i].ChoiceSlot < list[j].ChoiceSlot
	})
	return list
}

func (c *IslandCoordinator) computeDigest() [32]byte {
	h := sha256.New()
	var b [8]byte
	binary.LittleEndian.PutUint64(b[:], c.ProgramVersion)
	h.Write(b[:])
	binary.LittleEndian.PutUint64(b[:], c.InputHash)
	h.Write(b[:])
	binary.LittleEndian.PutUint32(b[:4], c.VirtualNow)
	h.Write(b[:4])
	for _, out := range c.Outputs {
		h.Write([]byte(out))
		h.Write([]byte{0})
	}
	var digest [32]byte
	copy(digest[:], h.Sum(nil))
	return digest
}

// RunRecord executes the island under controlled PRNG scheduling and returns
// the resulting `ReplayTrace` and observable output transcript.
func (c *IslandCoordinator) RunRecord() (ReplayTrace, []string, error) {
	var events []TraceEvent
	stepIdx := uint32(0)
	for {
		enabled := c.collectEnabled()
		if len(enabled) == 0 {
			break
		}
		pick := c.nextRand(len(enabled))
		chosen := enabled[pick]
		chosen.Apply(c)
		events = append(events, TraceEvent{
			StepIndex:  stepIdx,
			EventID:    chosen.EventID,
			TaskID:     chosen.TaskID,
			ChoiceSlot: chosen.ChoiceSlot,
			VirtualNow: c.VirtualNow,
		})
		stepIdx++
	}
	digest := c.computeDigest()
	return ReplayTrace{
		ProgramVersion: c.ProgramVersion,
		InputHash:      c.InputHash,
		Events:         events,
		FinalDigest:    digest,
	}, append([]string(nil), c.Outputs...), nil
}

// RunReplay executes the island strictly following `trace`. Any divergence in
// version, input contract, enabled event ID, trace length, or final state digest
// fails closed with a typed `HYB-E3605..HYB-E3608` error.
func (c *IslandCoordinator) RunReplay(trace ReplayTrace) ([]string, [32]byte, error) {
	if c.ProgramVersion != trace.ProgramVersion || c.InputHash != trace.InputHash {
		return nil, [32]byte{}, ErrReplayVersionMismatch
	}
	for idx, expectedEv := range trace.Events {
		enabled := c.collectEnabled()
		if len(enabled) == 0 {
			return nil, [32]byte{}, ErrReplayEventIDMismatch
		}
		matchedIdx := -1
		for i, cand := range enabled {
			if cand.EventID == expectedEv.EventID && cand.TaskID == expectedEv.TaskID && cand.ChoiceSlot == expectedEv.ChoiceSlot {
				matchedIdx = i
				break
			}
		}
		if matchedIdx < 0 || uint32(idx) != expectedEv.StepIndex {
			return nil, [32]byte{}, ErrReplayEventIDMismatch
		}
		enabled[matchedIdx].Apply(c)
		if c.VirtualNow != expectedEv.VirtualNow {
			return nil, [32]byte{}, ErrReplayEventIDMismatch
		}
	}
	// If any transition remains enabled after the trace ends, the trace was truncated.
	if len(c.collectEnabled()) > 0 {
		return nil, [32]byte{}, ErrReplayTraceTruncated
	}
	digest := c.computeDigest()
	if digest != trace.FinalDigest {
		return nil, digest, ErrReplayDigestMismatch
	}
	return append([]string(nil), c.Outputs...), digest, nil
}

// ---------------------------------------------------------------------------
// Phase-2 Reference Workloads (Two Racing Producers, Multi-Ready Select, Virtual Timers)
// ---------------------------------------------------------------------------

// BuildWorkload1TwoProducersRacing constructs Phase-2 Workload 1:
// Two concurrent producer tasks (`P1`, `P2`) racing to send `[101, 102]` and
// `[201, 202]` into one shared channel consumed by `Consumer`. Different seeds
// produce different arrival interleavings, and replaying any trace reproduces
// the exact interleaving and state digest.
func BuildWorkload1TwoProducersRacing(seed uint64) *IslandCoordinator {
	c := NewCoordinator(1001, 0xA1, seed)
	ch := c.NewChannel(1, 4)

	makeProducer := func(taskID uint16, vals []int64) func(*IslandCoordinator, *IslandTask) []EnabledTransition {
		return func(coord *IslandCoordinator, self *IslandTask) []EnabledTransition {
			if self.PC >= len(vals) {
				self.Done = true
				return nil
			}
			if len(ch.Queue) >= ch.Capacity {
				return nil
			}
			v := vals[self.PC]
			evID := (uint32(EventChanSend) << 24) | (uint32(taskID) << 16) | uint32(self.PC)
			return []EnabledTransition{{
				EventID:    evID,
				TaskID:     taskID,
				ChoiceSlot: uint16(self.PC),
				Kind:       EventChanSend,
				Apply: func(co *IslandCoordinator) {
					ch.Queue = append(ch.Queue, v)
					co.Outputs = append(co.Outputs, fmt.Sprintf("P%d->%d", taskID, v))
					self.PC++
					if self.PC >= len(vals) {
						self.Done = true
					}
				},
			}}
		}
	}

	c.AddTask(1, "producer1", makeProducer(1, []int64{101, 102}))
	c.AddTask(2, "producer2", makeProducer(2, []int64{201, 202}))
	c.AddTask(3, "consumer", func(coord *IslandCoordinator, self *IslandTask) []EnabledTransition {
		if self.PC >= 4 {
			self.Done = true
			return nil
		}
		if len(ch.Queue) == 0 {
			return nil
		}
		evID := (uint32(EventChanRecv) << 24) | (uint32(3) << 16) | uint32(self.PC)
		return []EnabledTransition{{
			EventID:    evID,
			TaskID:     3,
			ChoiceSlot: uint16(self.PC),
			Kind:       EventChanRecv,
			Apply: func(co *IslandCoordinator) {
				v := ch.Queue[0]
				ch.Queue = ch.Queue[1:]
				co.Outputs = append(co.Outputs, fmt.Sprintf("C<-%d", v))
				self.PC++
				if self.PC >= 4 {
					self.Done = true
				}
			},
		}}
	})
	return c
}

// BuildWorkload2MultiReadySelect constructs Phase-2 Workload 2:
// A `select` statement with two simultaneously ready input channels (`chA`, `chB`).
// Both `select` arms are enabled simultaneously; the coordinator records which
// arm (`ChoiceSlot 0` vs `ChoiceSlot 1`) won at each step.
func BuildWorkload2MultiReadySelect(seed uint64) *IslandCoordinator {
	c := NewCoordinator(1002, 0xB2, seed)
	chA := c.NewChannel(1, 2)
	chB := c.NewChannel(2, 2)
	chA.Queue = append(chA.Queue, 10, 20)
	chB.Queue = append(chB.Queue, 30, 40)

	c.AddTask(1, "selector", func(coord *IslandCoordinator, self *IslandTask) []EnabledTransition {
		if self.PC >= 4 {
			self.Done = true
			return nil
		}
		var arms []EnabledTransition
		if len(chA.Queue) > 0 {
			arms = append(arms, EnabledTransition{
				EventID:    (uint32(EventSelectArm) << 24) | (uint32(self.PC) << 8) | 0,
				TaskID:     1,
				ChoiceSlot: 0,
				Kind:       EventSelectArm,
				Apply: func(co *IslandCoordinator) {
					v := chA.Queue[0]
					chA.Queue = chA.Queue[1:]
					co.Outputs = append(co.Outputs, fmt.Sprintf("select:A=%d", v))
					self.PC++
					if self.PC >= 4 {
						self.Done = true
					}
				},
			})
		}
		if len(chB.Queue) > 0 {
			arms = append(arms, EnabledTransition{
				EventID:    (uint32(EventSelectArm) << 24) | (uint32(self.PC) << 8) | 1,
				TaskID:     1,
				ChoiceSlot: 1,
				Kind:       EventSelectArm,
				Apply: func(co *IslandCoordinator) {
					v := chB.Queue[0]
					chB.Queue = chB.Queue[1:]
					co.Outputs = append(co.Outputs, fmt.Sprintf("select:B=%d", v))
					self.PC++
					if self.PC >= 4 {
						self.Done = true
					}
				},
			})
		}
		return arms
	})
	return c
}

// BuildWorkload3VirtualTimers constructs Phase-2 Workload 3:
// A task graph combining a worker channel and two virtual timers (`tick=5`, `tick=12`)
// racing with a `select` consumer.
func BuildWorkload3VirtualTimers(seed uint64) *IslandCoordinator {
	c := NewCoordinator(1003, 0xC3, seed)
	workCh := c.NewChannel(1, 2)
	timerCh := c.NewChannel(2, 2)
	workCh.Queue = append(workCh.Queue, 77)
	c.AddTimer(1, 5, 500, timerCh)
	c.AddTimer(2, 12, 1200, timerCh)

	c.AddTask(1, "timed_worker", func(coord *IslandCoordinator, self *IslandTask) []EnabledTransition {
		if self.PC >= 3 {
			self.Done = true
			return nil
		}
		var arms []EnabledTransition
		if len(workCh.Queue) > 0 {
			arms = append(arms, EnabledTransition{
				EventID:    (uint32(EventSelectArm) << 24) | (uint32(self.PC) << 8) | 0,
				TaskID:     1,
				ChoiceSlot: 0,
				Kind:       EventSelectArm,
				Apply: func(co *IslandCoordinator) {
					v := workCh.Queue[0]
					workCh.Queue = workCh.Queue[1:]
					co.Outputs = append(co.Outputs, fmt.Sprintf("work@%d=%d", co.VirtualNow, v))
					self.PC++
					if self.PC >= 3 {
						self.Done = true
					}
				},
			})
		}
		if len(timerCh.Queue) > 0 {
			arms = append(arms, EnabledTransition{
				EventID:    (uint32(EventSelectArm) << 24) | (uint32(self.PC) << 8) | 1,
				TaskID:     1,
				ChoiceSlot: 1,
				Kind:       EventSelectArm,
				Apply: func(co *IslandCoordinator) {
					v := timerCh.Queue[0]
					timerCh.Queue = timerCh.Queue[1:]
					co.Outputs = append(co.Outputs, fmt.Sprintf("timeout@%d=%d", co.VirtualNow, v))
					self.PC++
					if self.PC >= 3 {
						self.Done = true
					}
				},
			})
		}
		return arms
	})
	return c
}

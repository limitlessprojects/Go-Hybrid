// Package deterministic is a closed-effect experiment, not a Hybrid compiler pass.
package deterministic

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"math"
)

type Op struct {
	Kind    string
	Value   uint64
	Channel string
}
type Task struct {
	ID  uint64
	Ops []Op
}
type Event struct {
	Tick, Task uint64
	Kind       string
	Value      uint64
}
type Certificate struct {
	Version         string
	Program, Events string
	Seed            uint64
	Count           int
}
type frame struct {
	task Task
	pc   int
	tick uint64
}

func digest(value any) string {
	data, _ := json.Marshal(value)
	sum := sha256.Sum256(data)
	return hex.EncodeToString(sum[:])
}

// Run admits only explicit data operations. It never invokes user callbacks or Go I/O.
func Run(program []Task, seed uint64, limit int) ([]Event, Certificate, error) {
	empty := Certificate{}
	if limit < 1 || limit > 1_000_000 {
		return nil, empty, errors.New("invalid step budget")
	}
	frames := make([]frame, len(program))
	seen := map[uint64]bool{}
	total := 0
	for i, task := range program {
		if seen[task.ID] {
			return nil, empty, errors.New("duplicate task ID")
		}
		seen[task.ID] = true
		frames[i] = frame{task: Task{task.ID, append([]Op(nil), task.Ops...)}}
		for _, op := range task.Ops {
			switch op.Kind {
			case "emit", "delay", "rand":
			case "send", "recv":
				if op.Channel == "" {
					return nil, empty, errors.New("missing channel")
				}
			default:
				return nil, empty, fmt.Errorf("unadmitted effect %q", op.Kind)
			}
			total++
			if total > limit {
				return nil, empty, errors.New("step budget exceeded")
			}
		}
	}
	queues := map[string][]uint64{}
	events := make([]Event, 0, total)
	rng := seed
	clock := uint64(0)
	if rng == 0 {
		rng = 1
	}
	for remaining := total; remaining > 0; remaining-- {
		chosen := -1
		for i := range frames {
			f := &frames[i]
			if f.pc == len(f.task.Ops) {
				continue
			}
			op := f.task.Ops[f.pc]
			if op.Kind == "recv" && len(queues[op.Channel]) == 0 {
				continue
			}
			if chosen < 0 || f.tick < frames[chosen].tick || f.tick == frames[chosen].tick && f.task.ID < frames[chosen].task.ID {
				chosen = i
			}
		}
		if chosen < 0 {
			return nil, empty, errors.New("closed-effect deadlock")
		}
		f := &frames[chosen]
		if f.tick < clock {
			f.tick = clock
		}
		clock = f.tick
		op := f.task.Ops[f.pc]
		value := op.Value
		advance := uint64(1)
		switch op.Kind {
		case "delay":
			if value == math.MaxUint64 {
				return nil, empty, errors.New("clock overflow")
			}
			advance += value
		case "rand":
			rng ^= rng << 13
			rng ^= rng >> 7
			rng ^= rng << 17
			value = rng
		case "send":
			if len(queues[op.Channel]) >= 16 {
				return nil, empty, errors.New("channel capacity exceeded")
			}
			queues[op.Channel] = append(queues[op.Channel], value)
		case "recv":
			value = queues[op.Channel][0]
			queues[op.Channel] = queues[op.Channel][1:]
		}
		if f.tick > math.MaxUint64-advance {
			return nil, empty, errors.New("clock overflow")
		}
		events = append(events, Event{f.tick, f.task.ID, op.Kind, value})
		f.tick += advance
		f.pc++
	}
	cert := Certificate{"closed-effects/v1", digest(program), digest(events), seed, len(events)}
	return events, cert, nil
}

func Verify(program []Task, cert Certificate, limit int) error {
	_, expected, err := Run(program, cert.Seed, limit)
	if err != nil {
		return err
	}
	if cert != expected {
		return errors.New("replay certificate mismatch")
	}
	return nil
}

// Package periodic is an isolated W-018 experiment, not a Hybrid compiler API.
package periodic

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"regexp"
)

// Limits keep the experiment's input, replay, and generated source bounded.
const (
	MaxActors   = 16
	MaxEdges    = 32
	MaxSteps    = 4096
	MaxRate     = 4096
	MaxCapacity = 65536
	MaxInput    = 1 << 20
)

type Actor struct {
	Name   string `json:"name"`
	Op     string `json:"op"`
	Factor uint64 `json:"factor"`
}

type Edge struct {
	Name    string   `json:"name"`
	From    int      `json:"from"`
	To      int      `json:"to"`
	Produce int      `json:"produce"`
	Consume int      `json:"consume"`
	Initial []uint64 `json:"initial"`
}

type Graph struct {
	Actors []Actor `json:"actors"`
	Edges  []Edge  `json:"edges"`
}

// Certificate describes a single witness. Capacities need not be optimal.
type Certificate struct {
	GraphSHA256  string `json:"graph_sha256"`
	Schedule     []int  `json:"schedule"`
	Repetitions  []int  `json:"repetitions"`
	Capacities   []int  `json:"capacities"`
	HighWater    []int  `json:"high_water"`
	PayloadBytes uint64 `json:"payload_bytes"`
}

type Input struct {
	Graph       Graph       `json:"graph"`
	Certificate Certificate `json:"certificate"`
}

type Report struct {
	GraphSHA256  string `json:"graph_sha256"`
	Firings      []int  `json:"firings"`
	HighWater    []int  `json:"high_water"`
	PayloadBytes uint64 `json:"payload_bytes"`
}

// Digest binds rates, topology, actor semantics and initial values. It hashes
// encoding/json's compact encoding of Graph, including struct field order.
// This is content binding, not authentication or a digital signature.
func Digest(g Graph) string {
	b, _ := json.Marshal(g) // Graph contains only JSON-encodable fields.
	h := sha256.Sum256(b)
	return hex.EncodeToString(h[:])
}

// Decode accepts one bounded JSON document and rejects unsupported fields.
func Decode(r io.Reader) (Input, error) {
	var in Input
	b, err := io.ReadAll(io.LimitReader(r, MaxInput+1))
	if err != nil {
		return in, err
	}
	if len(b) > MaxInput {
		return in, fmt.Errorf("input exceeds %d bytes", MaxInput)
	}
	d := json.NewDecoder(bytes.NewReader(b))
	d.DisallowUnknownFields()
	if err := d.Decode(&in); err != nil {
		return in, err
	}
	var extra any
	if err := d.Decode(&extra); err != io.EOF {
		return in, fmt.Errorf("expected exactly one JSON document")
	}
	return in, nil
}

var namePattern = regexp.MustCompile(`^[A-Za-z][A-Za-z0-9_]{0,63}$`)

func validateGraph(g Graph) error {
	if len(g.Actors) == 0 || len(g.Actors) > MaxActors || len(g.Edges) == 0 || len(g.Edges) > MaxEdges {
		return fmt.Errorf("graph size outside experiment limits")
	}
	names := map[string]bool{}
	for _, a := range g.Actors {
		if !namePattern.MatchString(a.Name) || names[a.Name] {
			return fmt.Errorf("invalid or duplicate actor name %q", a.Name)
		}
		names[a.Name] = true
		switch a.Op {
		case "source", "sum", "sink":
			if a.Factor != 0 {
				return fmt.Errorf("factor only supported for scale")
			}
		case "scale":
		default:
			return fmt.Errorf("unmodeled actor operation %q", a.Op)
		}
	}
	in, out := make([]int, len(g.Actors)), make([]int, len(g.Actors))
	names = map[string]bool{}
	for _, e := range g.Edges {
		if !namePattern.MatchString(e.Name) || names[e.Name] {
			return fmt.Errorf("invalid or duplicate edge name %q", e.Name)
		}
		names[e.Name] = true
		if e.From < 0 || e.From >= len(g.Actors) || e.To < 0 || e.To >= len(g.Actors) {
			return fmt.Errorf("edge %s has invalid endpoint", e.Name)
		}
		if e.Produce < 1 || e.Produce > MaxRate || e.Consume < 1 || e.Consume > MaxRate || len(e.Initial) > MaxCapacity {
			return fmt.Errorf("edge %s has unsupported rate or initial marking", e.Name)
		}
		out[e.From]++
		in[e.To]++
	}
	for i, a := range g.Actors {
		valid := false
		switch a.Op {
		case "source":
			valid = in[i] == 0 && out[i] > 0
		case "scale":
			valid = in[i] == 1 && out[i] > 0
		case "sum":
			valid = in[i] > 0 && out[i] > 0
		case "sink":
			valid = in[i] > 0 && out[i] == 0
		}
		if !valid {
			return fmt.Errorf("actor %s topology incompatible with %s", a.Name, a.Op)
		}
	}
	return nil
}

// Verify replays token accounting without trusting producer summaries.
// It establishes no claims about actors outside the closed registry, execution
// time, optimality, or liveness under any other schedule.
func Verify(g Graph, c Certificate) (Report, error) {
	var report Report
	if err := validateGraph(g); err != nil {
		return report, err
	}
	if c.GraphSHA256 != Digest(g) {
		return report, fmt.Errorf("graph digest mismatch")
	}
	if len(c.Schedule) == 0 || len(c.Schedule) > MaxSteps || len(c.Repetitions) != len(g.Actors) || len(c.Capacities) != len(g.Edges) || len(c.HighWater) != len(g.Edges) {
		return report, fmt.Errorf("malformed witness dimensions")
	}
	for _, n := range c.Repetitions {
		if n < 1 || n > MaxSteps {
			return report, fmt.Errorf("each actor must fire a positive bounded number of times")
		}
	}
	occupancy, high := make([]int, len(g.Edges)), make([]int, len(g.Edges))
	var payload uint64
	for i, e := range g.Edges {
		cap := c.Capacities[i]
		if cap < 1 || cap > MaxCapacity || len(e.Initial) > cap {
			return report, fmt.Errorf("edge %s has invalid capacity/initial marking", e.Name)
		}
		occupancy[i], high[i] = len(e.Initial), len(e.Initial)
		payload += uint64(cap) * 8 // bounded by MaxEdges * MaxCapacity * 8
	}
	if payload != c.PayloadBytes {
		return report, fmt.Errorf("payload budget: declared %d, actual %d", c.PayloadBytes, payload)
	}
	fired := make([]int, len(g.Actors))
	for step, a := range c.Schedule {
		if a < 0 || a >= len(g.Actors) {
			return report, fmt.Errorf("invalid actor at step %d", step)
		}
		// All inputs are consumed before any outputs become available, including
		// on self-loops. A firing cannot consume its own future output.
		for i, e := range g.Edges {
			if e.To == a {
				if occupancy[i] < e.Consume {
					return report, fmt.Errorf("underflow on %s at step %d", e.Name, step)
				}
				occupancy[i] -= e.Consume
			}
		}
		for i, e := range g.Edges {
			if e.From == a {
				occupancy[i] += e.Produce
				if occupancy[i] > c.Capacities[i] {
					return report, fmt.Errorf("overflow on %s at step %d", e.Name, step)
				}
				if occupancy[i] > high[i] {
					high[i] = occupancy[i]
				}
			}
		}
		fired[a]++
	}
	for i, n := range fired {
		if n != c.Repetitions[i] {
			return report, fmt.Errorf("actor %s firing count mismatch", g.Actors[i].Name)
		}
	}
	for i, e := range g.Edges {
		if occupancy[i] != len(e.Initial) {
			return report, fmt.Errorf("nonperiodic marking on %s", e.Name)
		}
		if high[i] != c.HighWater[i] {
			return report, fmt.Errorf("high-water mismatch on %s", e.Name)
		}
	}
	return Report{Digest(g), fired, high, payload}, nil
}

package periodic

import (
	"bytes"
	"encoding/json"
	"os"
	"strings"
	"testing"
)

func fixture(t *testing.T, name string) Input {
	t.Helper()
	f, err := os.Open("fixtures/" + name + ".json")
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	in, err := Decode(f)
	if err != nil {
		t.Fatal(err)
	}
	return in
}

func TestHandWitnesses(t *testing.T) {
	for _, tt := range []struct {
		name  string
		bytes uint64
	}{
		{"pipeline", 16}, {"multirate", 48}, {"join", 24}, {"feedback", 16},
	} {
		t.Run(tt.name, func(t *testing.T) {
			in := fixture(t, tt.name)
			r, err := Verify(in.Graph, in.Certificate)
			if err != nil {
				t.Fatal(err)
			}
			if r.PayloadBytes != tt.bytes {
				t.Fatalf("payload = %d; want %d", r.PayloadBytes, tt.bytes)
			}
			t.Logf("verified FIFO payload: %d bytes", r.PayloadBytes)
		})
	}
}

func TestMutatedWitnesses(t *testing.T) {
	tests := []struct {
		name string
		edit func(*Input)
	}{
		{"underflow", func(in *Input) { in.Certificate.Schedule = []int{1, 0, 2} }},
		{"overflow", func(in *Input) { in.Certificate.Schedule = []int{0, 0, 1, 2} }},
		{"nonperiodic", func(in *Input) { in.Certificate.Schedule = []int{0, 1} }},
		{"empty_schedule", func(in *Input) { in.Certificate.Schedule = nil }},
		{"invalid_actor", func(in *Input) { in.Certificate.Schedule[1] = len(in.Graph.Actors) }},
		{"negative_actor", func(in *Input) { in.Certificate.Schedule[1] = -1 }},
		{"omitted_actor", func(in *Input) { in.Certificate.Schedule = []int{0} }},
		{"lying_repetitions", func(in *Input) { in.Certificate.Repetitions[0]++ }},
		{"zero_repetitions", func(in *Input) { in.Certificate.Repetitions[0] = 0 }},
		{"unbounded_repetitions", func(in *Input) { in.Certificate.Repetitions[0] = MaxSteps + 1 }},
		{"missing_repetitions", func(in *Input) { in.Certificate.Repetitions = nil }},
		{"missing_capacities", func(in *Input) { in.Certificate.Capacities = nil }},
		{"zero_capacity", func(in *Input) { in.Certificate.Capacities[0] = 0 }},
		{"negative_capacity", func(in *Input) { in.Certificate.Capacities[0] = -1 }},
		{"unbounded_capacity", func(in *Input) { in.Certificate.Capacities[0] = MaxCapacity + 1 }},
		{"underclaimed_high_water", func(in *Input) { in.Certificate.HighWater[0]-- }},
		{"overclaimed_high_water", func(in *Input) { in.Certificate.HighWater[0]++ }},
		{"missing_high_water", func(in *Input) { in.Certificate.HighWater = nil }},
		{"underclaimed_bytes", func(in *Input) { in.Certificate.PayloadBytes-- }},
		{"overclaimed_bytes", func(in *Input) { in.Certificate.PayloadBytes++ }},
		{"wrapped_bytes", func(in *Input) { in.Certificate.PayloadBytes = ^uint64(0) }},
		{"oversized_schedule", func(in *Input) { in.Certificate.Schedule = make([]int, MaxSteps+1) }},
		{"digest", func(in *Input) { in.Certificate.GraphSHA256 = strings.Repeat("0", 64) }},
		{"initial_values_binding", func(in *Input) { in.Graph.Edges[0].Initial = []uint64{7} }},
		{"actor_semantics_binding", func(in *Input) { in.Graph.Actors[1].Factor = 4 }},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			in := fixture(t, "pipeline")
			tt.edit(&in)
			if _, err := Verify(in.Graph, in.Certificate); err == nil {
				t.Fatal("mutated certificate accepted")
			}
			if _, err := Emit(in.Graph, in.Certificate, "kernel"); err == nil {
				t.Fatal("emitter bypassed verification")
			}
		})
	}
}

func TestGraphAdmission(t *testing.T) {
	tests := []struct {
		name string
		edit func(*Graph)
	}{
		{"dynamic_actor", func(g *Graph) { g.Actors[1].Op = "callback" }},
		{"foreign_call", func(g *Graph) { g.Actors[1].Op = "ffi" }},
		{"blocking_actor", func(g *Graph) { g.Actors[1].Op = "sleep" }},
		{"zero_rate", func(g *Graph) { g.Edges[0].Produce = 0 }},
		{"negative_rate", func(g *Graph) { g.Edges[0].Consume = -1 }},
		{"large_rate", func(g *Graph) { g.Edges[0].Produce = MaxRate + 1 }},
		{"bad_endpoint", func(g *Graph) { g.Edges[0].To = 8 }},
		{"negative_endpoint", func(g *Graph) { g.Edges[0].From = -1 }},
		{"duplicate_actor", func(g *Graph) { g.Actors[1].Name = g.Actors[0].Name }},
		{"duplicate_edge", func(g *Graph) { g.Edges[1].Name = g.Edges[0].Name }},
		{"code_in_name", func(g *Graph) { g.Edges[0].Name = "x\npackage evil" }},
		{"empty_graph", func(g *Graph) { *g = Graph{} }},
		{"actor_limit", func(g *Graph) { g.Actors = make([]Actor, MaxActors+1) }},
		{"edge_limit", func(g *Graph) { g.Edges = make([]Edge, MaxEdges+1) }},
		{"oversized_initial", func(g *Graph) { g.Edges[0].Initial = make([]uint64, MaxCapacity+1) }},
		{"initial_exceeds_capacity", func(g *Graph) { g.Edges[0].Initial = []uint64{1, 2} }},
		{"source_receives", func(g *Graph) { g.Edges[0].To = 0 }},
		{"sink_sends", func(g *Graph) { g.Edges[0].From = 2 }},
		{"scale_no_input", func(g *Graph) { g.Edges[0].To = 2 }},
		{"sum_no_output", func(g *Graph) { g.Actors[2].Op = "sum" }},
		{"unmodeled_factor", func(g *Graph) { g.Actors[0].Factor = 1 }},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			in := fixture(t, "pipeline")
			tt.edit(&in.Graph)
			in.Certificate.GraphSHA256 = Digest(in.Graph)
			if _, err := Verify(in.Graph, in.Certificate); err == nil {
				t.Fatal("unsupported graph accepted")
			}
		})
	}
}

func TestStrictInput(t *testing.T) {
	in := fixture(t, "pipeline")
	good, err := json.Marshal(in)
	if err != nil {
		t.Fatal(err)
	}
	for _, bad := range []string{
		`{"graph":{"callback":"unknown effect"}}`,
		`{"graph":{"edges":[{"token_type":"pointer"}]}}`,
		`{"certificate":{"payload_bytes":-1}}`,
		`{"certificate":{"payload_bytes":18446744073709551616}}`,
		`{"certificate":{"schedule":[0.5]}}`,
		`{"graph":{"actors":[{"factor":1e10}]}}`,
		string(good) + `{}`,
		string(good) + `junk`,
		strings.Repeat(" ", MaxInput+1),
	} {
		if _, err := Decode(strings.NewReader(bad)); err == nil {
			t.Fatal("unsupported/malformed input accepted")
		}
	}
	decoded, err := Decode(bytes.NewReader(good))
	if err != nil {
		t.Fatal(err)
	}
	if _, err := Verify(decoded.Graph, decoded.Certificate); err != nil {
		t.Fatal(err)
	}
}

func TestPackageAdmission(t *testing.T) {
	in := fixture(t, "pipeline")
	for _, name := range []string{"", "_", "package", "a.b", "x; import evil"} {
		if _, err := Emit(in.Graph, in.Certificate, name); err == nil {
			t.Fatalf("invalid package %q accepted", name)
		}
	}
}

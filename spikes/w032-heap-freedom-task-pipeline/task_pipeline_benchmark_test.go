package w032

import (
	"testing"
)

type PipelinePayload struct {
	ID        uint64
	Timestamp int64
	Value     float64
	Flags     uint32
}

// Named value-receiver task struct (replaces heap-allocated closure trampoline)
type ValueTaskWorker struct {
	Payload PipelinePayload
}

func (w ValueTaskWorker) Run(out *PipelinePayload) {
	out.ID = w.Payload.ID
	out.Timestamp = w.Payload.Timestamp
	out.Value = w.Payload.Value * 1.5
	out.Flags = w.Payload.Flags | 0x01
}

func BenchmarkZeroAllocTaskPipeline(b *testing.B) {
	payload := PipelinePayload{
		ID:        1001,
		Timestamp: 1729000000,
		Value:     42.5,
		Flags:     0x10,
	}

	var result PipelinePayload

	b.ReportAllocs()
	b.ResetTimer()

	for i := 0; i < b.N; i++ {
		worker := ValueTaskWorker{Payload: payload}
		worker.Run(&result)
	}
}

func TestZeroAllocsPerRun(t *testing.T) {
	payload := PipelinePayload{
		ID:        42,
		Timestamp: 1729000000,
		Value:     100.0,
		Flags:     0x02,
	}

	var result PipelinePayload

	allocs := testing.AllocsPerRun(100000, func() {
		worker := ValueTaskWorker{Payload: payload}
		worker.Run(&result)
	})

	if allocs != 0.0 {
		t.Fatalf("Expected strictly 0.0 allocs/op, got %f", allocs)
	}

	if result.Value != 150.0 {
		t.Fatalf("Unexpected computation result: got %f, want 150.0", result.Value)
	}
}

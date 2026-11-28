package w4699artifactmanifest

import (
	"bytes"
	"math"
	"testing"
)

// nanWorker returns a successful result whose value is the IEEE-754 bytes of
// NaN. It is used to prove the coordinator actually rejects non-finite payloads
// when a Validate hook is installed.
func nanWorker(in TaskInput) (Result, []TaskInput) {
	bits := math.Float64bits(math.NaN())
	val := Value{
		byte(bits), byte(bits >> 8), byte(bits >> 16), byte(bits >> 24),
		byte(bits >> 32), byte(bits >> 40), byte(bits >> 48), byte(bits >> 56),
	}
	return Ok(val), nil
}

// finiteFloatValidator decodes an 8-byte little-endian float64 and rejects
// NaN/Inf. It is the Validate hook used to prove non-finite rejection.
func finiteFloatValidator(v Value) error {
	if len(v) != 8 {
		return nil // not a float payload; nothing to check
	}
	bits := uint64(v[0]) | uint64(v[1])<<8 | uint64(v[2])<<16 | uint64(v[3])<<24 |
		uint64(v[4])<<32 | uint64(v[5])<<40 | uint64(v[6])<<48 | uint64(v[7])<<56
	f := math.Float64frombits(bits)
	return CheckFinite(f)
}

// TestM1_ValidateHookRejectsNonFinite proves the coordinator's Validate hook
// actually rejects a NaN payload end-to-end (not just the CheckFinite helper in
// isolation). Without the hook the same worker succeeds; with the hook the group
// is rejected and no manifest is published.
func TestM1_ValidateHookRejectsNonFinite(t *testing.T) {
	// Without a Validate hook the NaN worker runs and produces a manifest.
	c := NewCoordinator(nanWorker, Budget{MaxTasks: 8, MaxBytes: 1 << 16})
	m, err := c.Run(FIFO)
	if err != nil {
		t.Fatalf("no-hook run should succeed, got %v", err)
	}
	if m == nil || len(m.Entries) != 1 {
		t.Fatalf("no-hook run should produce one entry, got %+v", m)
	}

	// With the finite-float Validate hook the NaN payload is rejected and no
	// manifest is published.
	c2 := NewCoordinator(nanWorker, Budget{MaxTasks: 8, MaxBytes: 1 << 16})
	c2.Validate = finiteFloatValidator
	m2, err2 := c2.Run(FIFO)
	if err2 != ErrNonFiniteValue {
		t.Fatalf("expected ErrNonFiniteValue, got %v", err2)
	}
	if m2 != nil {
		t.Fatalf("rejected group must produce no manifest, got %+v", m2)
	}

	// A finite float passes the same hook.
	c3 := NewCoordinator(func(in TaskInput) (Result, []TaskInput) {
		bits := math.Float64bits(3.5)
		return Ok(Value{byte(bits), byte(bits >> 8), byte(bits >> 16), byte(bits >> 24),
			byte(bits >> 32), byte(bits >> 40), byte(bits >> 48), byte(bits >> 56)}), nil
	}, Budget{MaxTasks: 8, MaxBytes: 1 << 16})
	c3.Validate = finiteFloatValidator
	if _, err := c3.Run(FIFO); err != nil {
		t.Fatalf("finite float rejected: %v", err)
	}
}

// TestM1_PoliciesAreGenuinelyDistinct proves the four schedule policies produce
// different execution traces (so they are real, distinct schedules) while still
// publishing byte-identical manifests. This closes the "DepthFirst == LIFO"
// defect: the traces must differ.
func TestM1_PoliciesAreGenuinelyDistinct(t *testing.T) {
	traces := map[SchedulePolicy][]string{}
	manifests := map[SchedulePolicy][]byte{}
	for _, p := range []SchedulePolicy{FIFO, LIFO, DepthFirst, Random} {
		c := NewCoordinator(binaryTreeWorker, Budget{MaxTasks: 64, MaxBytes: 1 << 16})
		c.SetRootValue(Value{'1'})
		c.SetSeed(12345)
		m, err := c.Run(p)
		if err != nil {
			t.Fatalf("policy %d: %v", p, err)
		}
		traces[p] = append([]string(nil), c.Trace...)
		manifests[p] = m.Encode()
	}

	// FIFO and LIFO must differ (first vs last ready task).
	if bytes.Equal([]byte(join(traces[FIFO])), []byte(join(traces[LIFO]))) {
		t.Fatalf("FIFO and LIFO traces identical: %v", traces[FIFO])
	}
	// DepthFirst must differ from LIFO (deepest-first vs last-enqueued).
	if bytes.Equal([]byte(join(traces[DepthFirst])), []byte(join(traces[LIFO]))) {
		t.Fatalf("DepthFirst and LIFO traces identical: %v", traces[DepthFirst])
	}
	// DepthFirst must differ from FIFO.
	if bytes.Equal([]byte(join(traces[DepthFirst])), []byte(join(traces[FIFO]))) {
		t.Fatalf("DepthFirst and FIFO traces identical: %v", traces[DepthFirst])
	}

	// All four manifests must be byte-identical despite different traces.
	for p, mb := range manifests {
		if !bytes.Equal(mb, manifests[FIFO]) {
			t.Fatalf("policy %d manifest differs from FIFO", p)
		}
	}
}

func join(s []string) string {
	out := ""
	for _, v := range s {
		out += v + ","
	}
	return out
}
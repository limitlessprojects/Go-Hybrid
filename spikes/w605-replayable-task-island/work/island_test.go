package w605replayisland

import (
	"slices"
	"strings"
	"testing"
)

// TestPhase1_AdmissionPassRejectsUnmediatedNondeterminism verifies Phase 1:
// The static `replay_island` admission pass admits closed task/channel/select/timer
// regions and rejects wall-clock reads, RNG, external I/O, `managed` blocks,
// unordered map iteration, and uninstrumented channels/goroutines (`HYB-E3601..HYB-E3604`).
func TestPhase1_AdmissionPassRejectsUnmediatedNondeterminism(t *testing.T) {
	validIsland := `
replay_island("order-matching"):
    ch := make(chan int, 2)
    task():
        ch <- 101
    v := <-ch
`
	if diags := CheckIslandAdmission(validIsland); len(diags) != 0 {
		t.Fatalf("expected valid island to pass with 0 diagnostics, got %+v", diags)
	}

	negativeCases := []struct {
		name     string
		snippet  string
		wantCode string
	}{
		{"wall_clock", "t := time.now()", "HYB-E3601"},
		{"random_source", "r := rand.Intn(10)", "HYB-E3601"},
		{"foreign_callback", "foreign_callback(cb)", "HYB-E3601"},
		{"managed_escape", `managed("escape", "audit"):`, "HYB-E3602"},
		{"unordered_map_range", "for k, v := range map_orders:", "HYB-E3603"},
		{"uninstrumented_chan", "raw_chan_send(ch, 1)", "HYB-E3604"},
	}

	for _, tc := range negativeCases {
		diags := CheckIslandAdmission(tc.snippet)
		if len(diags) == 0 || diags[0].Code != tc.wantCode {
			t.Fatalf("case %s: got %+v, want %s", tc.name, diags, tc.wantCode)
		}
	}
}

// TestPhase2_ThreeWorkloadsRecordAndReplayCompleteness verifies Phase 2:
// Across all 3 reference workloads (two producers racing, multi-ready select arms,
// and virtual timers), different seeds explore distinct schedules, and replaying
// each recorded trace reproduces the exact event sequence, output transcript,
// and 32-byte SHA-256 final state digest.
func TestPhase2_ThreeWorkloadsRecordAndReplayCompleteness(t *testing.T) {
	builders := []struct {
		name         string
		minDistinct  int
		buildWithSeed func(uint64) *IslandCoordinator
	}{
		{"W1_TwoProducersRacing", 3, BuildWorkload1TwoProducersRacing},
		{"W2_MultiReadySelect", 4, BuildWorkload2MultiReadySelect},
		{"W3_VirtualTimers", 1, BuildWorkload3VirtualTimers},
	}

	for _, wb := range builders {
		distinctDigests := make(map[[32]byte]bool)
		for seed := uint64(1); seed <= 32; seed++ {
			recCoord := wb.buildWithSeed(seed)
			trace, recOut, err := recCoord.RunRecord()
			if err != nil {
				t.Fatalf("%s seed=%d RunRecord failed: %v", wb.name, seed, err)
			}
			distinctDigests[trace.FinalDigest] = true

			// Verify wire encoding size is 48 + 16 * len(Events).
			encoded := trace.EncodeBinary()
			if len(encoded) != 48+16*len(trace.Events) {
				t.Fatalf("%s encoded size = %d, want %d", wb.name, len(encoded), 48+16*len(trace.Events))
			}

			// Replay against a fresh coordinator initialized with a completely different
			// PRNG seed (999999) to prove replay depends ONLY on `trace`, never on runtime RNG.
			repCoord := wb.buildWithSeed(999999)
			repOut, repDigest, err := repCoord.RunReplay(trace)
			if err != nil {
				t.Fatalf("%s seed=%d RunReplay failed: %v", wb.name, seed, err)
			}
			if repDigest != trace.FinalDigest || !slices.Equal(repOut, recOut) {
				t.Fatalf("%s seed=%d replay mismatch: got %v, want %v", wb.name, seed, repOut, recOut)
			}
		}
		if len(distinctDigests) < wb.minDistinct {
			t.Fatalf("%s explored %d distinct schedules across 32 seeds, want >= %d", wb.name, len(distinctDigests), wb.minDistinct)
		}
	}
}

// TestPhase3_NegativeControlsTamperedTracesFailClosed verifies Phase 3:
// Altered event IDs, swapped select choice slots, truncated traces, version/input
// mismatches, and corrupted final state digests are deterministically rejected
// (`HYB-E3605..HYB-E3608`).
func TestPhase3_NegativeControlsTamperedTracesFailClosed(t *testing.T) {
	baseCoord := BuildWorkload1TwoProducersRacing(7)
	validTrace, _, err := baseCoord.RunRecord()
	if err != nil {
		t.Fatalf("RunRecord failed: %v", err)
	}

	// 1. Version / input contract mismatch -> ErrReplayVersionMismatch (HYB-E3606)
	badVer := validTrace
	badVer.ProgramVersion ^= 1
	if _, _, err := BuildWorkload1TwoProducersRacing(1).RunReplay(badVer); err != ErrReplayVersionMismatch {
		t.Fatalf("expected ErrReplayVersionMismatch, got %v", err)
	}

	// 2. Corrupted EventID -> ErrReplayEventIDMismatch (HYB-E3607)
	badEv := validTrace
	badEv.Events = append([]TraceEvent(nil), validTrace.Events...)
	badEv.Events[0].EventID ^= 0xFF
	if _, _, err := BuildWorkload1TwoProducersRacing(1).RunReplay(badEv); err != ErrReplayEventIDMismatch {
		t.Fatalf("expected ErrReplayEventIDMismatch on corrupted EventID, got %v", err)
	}

	// 3. Truncated trace before island quiescence -> ErrReplayTraceTruncated (HYB-E3605)
	truncated := validTrace
	truncated.Events = validTrace.Events[:len(validTrace.Events)-1]
	if _, _, err := BuildWorkload1TwoProducersRacing(1).RunReplay(truncated); err != ErrReplayTraceTruncated {
		t.Fatalf("expected ErrReplayTraceTruncated, got %v", err)
	}

	// 4. Uninstrumented mutation / digest tampering -> ErrReplayDigestMismatch (HYB-E3608)
	badDigest := validTrace
	badDigest.FinalDigest[0] ^= 0x80
	if _, _, err := BuildWorkload1TwoProducersRacing(1).RunReplay(badDigest); err != ErrReplayDigestMismatch {
		t.Fatalf("expected ErrReplayDigestMismatch, got %v", err)
	}

	// 5. Swapped select choice slot on Workload 2 changes schedule and triggers
	// ErrReplayDigestMismatch when replayed against the original FinalDigest.
	w2Coord := BuildWorkload2MultiReadySelect(3)
	w2Trace, w2Out, err := w2Coord.RunRecord()
	if err != nil {
		t.Fatalf("W2 RunRecord failed: %v", err)
	}
	swapped := w2Trace
	swapped.Events = append([]TraceEvent(nil), w2Trace.Events...)
	// Flip first select decision between ChoiceSlot 0 (chA) and ChoiceSlot 1 (chB).
	swapped.Events[0].ChoiceSlot ^= 1
	swapped.Events[0].EventID ^= 1
	if _, _, err := BuildWorkload2MultiReadySelect(1).RunReplay(swapped); err == nil {
		t.Fatalf("expected swapped select arm trace to fail replay verification (original out=%v)", strings.Join(w2Out, ","))
	}
}

func BenchmarkWorkload1Record(b *testing.B) {
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		c := BuildWorkload1TwoProducersRacing(uint64(i + 1))
		if _, _, err := c.RunRecord(); err != nil {
			b.Fatal(err)
		}
	}
}

func BenchmarkWorkload1Replay(b *testing.B) {
	c0 := BuildWorkload1TwoProducersRacing(42)
	trace, _, err := c0.RunRecord()
	if err != nil {
		b.Fatal(err)
	}
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		c := BuildWorkload1TwoProducersRacing(999)
		if _, _, err := c.RunReplay(trace); err != nil {
			b.Fatal(err)
		}
	}
}

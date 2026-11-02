package kahnspike

import (
	"runtime"
	"testing"
)

const FuzzerRuns = 1000

// TestH1_KahnAdmittedProgramsZeroDivergenceAcross1000Runs verifies Hypothesis H1 (Soundness):
// Every program admitted into the Kahn schedule-independent class produces byte-identical
// output across N = 1,000 runs under adversarial scheduling (GOMAXPROCS varied 1..8 and
// randomized runtime.Gosched() injection at every channel operation).
func TestH1_KahnAdmittedProgramsZeroDivergenceAcross1000Runs(t *testing.T) {
	origProcs := runtime.GOMAXPROCS(0)
	defer runtime.GOMAXPROCS(origProcs)

	admittedSuite := []struct {
		name string
		fn   func(seed uint64) string
		want string
	}{
		{
			name: "Example04_PipelineBackpressure_PureKahn_NoTimeout",
			fn:   RunExample04PipelinePureKahn,
			want: "total=204",
		},
		{
			name: "Corpus16_MoveSendClean_SPSC",
			fn:   RunCorpus16MoveSendClean,
			want: "ingested 8 batches acks=[0 1 2 3 4 5 6 7]",
		},
		{
			name: "Corpus23_MapslotCopyAtSpawn_SPSC",
			fn:   RunCorpus23MapslotCopyAtSpawn,
			want: "frontend: 5",
		},
		{
			name: "Corpus41_WrapperAliasMoveControl_SPSC",
			fn:   RunCorpus41WrapperAliasMoveControl,
			want: "before: 1 count: 1",
		},
		{
			name: "Corpus44_CopyPrimitiveElementControl_SPSC",
			fn:   RunCorpus44CopyPrimitiveControl,
			want: "src=5 dst=9",
		},
		{
			name: "Corpus62_ShadowingBinderLastUse_SPSC",
			fn:   RunCorpus62ShadowingBinderLastUse,
			want: "total=4950",
		},
		{
			name: "MultiStage_KahnDiamond_DedicatedEdgeChannels",
			fn:   RunMultiStageKahnDiamond,
			want: "(3,2),(5,5),(7,8),(9,11),(11,14),(13,17)",
		},
	}

	for _, tc := range admittedSuite {
		t.Run(tc.name, func(t *testing.T) {
			distinct := make(map[string]int)
			for i := 0; i < FuzzerRuns; i++ {
				if i%125 == 0 {
					runtime.GOMAXPROCS((i/125)%8 + 1)
				}
				seed := uint64(i)*0x9e3779b97f4a7c15 + 0xbf58476d1ce4e5b9
				out := tc.fn(seed)
				distinct[out]++
				if out != tc.want {
					t.Fatalf("H1 FALSIFIED on %s at run %d: got %q, want %q", tc.name, i, out, tc.want)
				}
			}
			if len(distinct) != 1 {
				t.Fatalf("H1 FALSIFIED on %s: observed %d distinct outputs: %v", tc.name, len(distinct), distinct)
			}
			t.Logf("[H1 PASS] %s: 1 distinct output (%q) across %d adversarial runs", tc.name, tc.want, FuzzerRuns)
		})
	}
}

// TestH2_TrueNondeterministicRejectedProgramsVaryAcross1000Runs verifies the first half of H2:
// Programs rejected by the Kahn classifier that lack a commutative sink genuinely vary in
// output under the schedule fuzzer (`select` arbitration, `default` races, raw multi-producer
// arrival order, shared `atomic` interleavings, and unordered map iteration).
func TestH2_TrueNondeterministicRejectedProgramsVaryAcross1000Runs(t *testing.T) {
	origProcs := runtime.GOMAXPROCS(0)
	defer runtime.GOMAXPROCS(origProcs)
	runtime.GOMAXPROCS(4)

	rejectedVarying := []struct {
		name string
		code string
		fn   func(seed uint64) string
	}{
		{
			name: "Corpus_select-two-recv-define-arms-accept",
			code: "HYB-E3503 (KAHN-SELECT)",
			fn:   RunRejectedSelectTwoRecvArms,
		},
		{
			name: "Corpus_35-default-fallback_under_concurrent_peer",
			code: "HYB-E3503 (KAHN-SELECT-DEFAULT)",
			fn:   RunRejectedSelectDefaultRace,
		},
		{
			name: "MultiProducer_UnsortedArrivalOrder",
			code: "HYB-E3501 (KAHN-MULTI-PRODUCER)",
			fn:   RunRejectedMultiProducerArrivalOrder,
		},
		{
			name: "SharedAtomic_ReadWriteInterleaving",
			code: "HYB-E3505 (KAHN-ATOMIC)",
			fn:   RunRejectedAtomicInterleaving,
		},
		{
			name: "UnorderedMapIteration_ToChannel",
			code: "HYB-E3506 (KAHN-MAP-ITER)",
			fn:   RunRejectedMapIterationToChannel,
		},
	}

	for _, tc := range rejectedVarying {
		t.Run(tc.name, func(t *testing.T) {
			distinct := make(map[string]int)
			for i := 0; i < FuzzerRuns; i++ {
				seed := uint64(i)*0x94d049bb133111eb + 1
				out := tc.fn(seed)
				distinct[out]++
			}
			if len(distinct) < 2 {
				t.Fatalf("expected rejected program %s (%s) to vary across %d runs, but got only 1 output: %v",
					tc.name, tc.code, FuzzerRuns, distinct)
			}
			t.Logf("[H2 TRUE POSITIVE] %s (%s): observed %d distinct outputs across %d runs",
				tc.name, tc.code, len(distinct), FuzzerRuns)
		})
	}
}

// TestH2_CanonicalFalseNegatives_CommutativeSinksAbsorbChannelOrderVariation verifies the
// second half of H2 and the core architectural finding connecting #2475 to #2385:
// Idiomatic Go fan-out/fan-in (`examples/03-worker-pool`, `corpus/15-fanin-per-task-state`,
// and `examples/gomod-integration/calc.hy`) violates strict SPSC Kahn channel uniqueness
// (their raw channel arrival traces vary across tens of permutations in 1,000 runs),
// YET their final output never varies (1 distinct output across 1,000 runs) because the
// consumer folds the multi-producer channel into a commutative sink (`sort.ints`, `merged[w] += n`,
// or `total += v`).
func TestH2_CanonicalFalseNegatives_CommutativeSinksAbsorbChannelOrderVariation(t *testing.T) {
	origProcs := runtime.GOMAXPROCS(0)
	defer runtime.GOMAXPROCS(origProcs)
	runtime.GOMAXPROCS(4)

	t.Run("Example03_WorkerPool_SortSink", func(t *testing.T) {
		finalOutputs := make(map[string]int)
		rawArrivals := make(map[string]int)
		for i := 0; i < FuzzerRuns; i++ {
			seed := uint64(i)*0xbf58476d1ce4e5b9 + 7
			finalOut, rawOrder := RunCanonicalFN_Example03WorkerPoolSorted(seed)
			finalOutputs[finalOut]++
			rawArrivals[rawOrder]++
		}
		if len(finalOutputs) != 1 {
			t.Fatalf("expected sorted output of 03-worker-pool to be invariant, got %v", finalOutputs)
		}
		if len(rawArrivals) < 2 {
			t.Fatalf("expected raw channel arrival order of 03-worker-pool to vary, got 1")
		}
		t.Logf("[H2 CANONICAL FN #1] examples/03-worker-pool: %d distinct channel arrival orders collapsed to %d final output (%q) via sort.ints sink",
			len(rawArrivals), len(finalOutputs), "[2 2 4 6 8 10 12 18]")
	})

	t.Run("Corpus15_FaninPerTaskState_CommutativeMapAddSink", func(t *testing.T) {
		finalOutputs := make(map[string]int)
		rawArrivals := make(map[string]int)
		for i := 0; i < FuzzerRuns; i++ {
			seed := uint64(i)*0x9e3779b97f4a7c15 + 13
			finalOut, arrival := RunCanonicalFN_Corpus15FaninCommutativeMapMerge(seed)
			finalOutputs[finalOut]++
			rawArrivals[arrival]++
		}
		if len(finalOutputs) != 1 {
			t.Fatalf("expected final merged output of 15-fanin-per-task-state to be invariant, got %v", finalOutputs)
		}
		if len(rawArrivals) < 2 {
			t.Fatalf("expected shard arrival order of 15-fanin-per-task-state to vary, got 1")
		}
		t.Logf("[H2 CANONICAL FN #2] corpus/15-fanin-per-task-state: %d distinct shard arrival orders collapsed to %d final output (%q) via commutative `merged[w] += n` fold (#2385)",
			len(rawArrivals), len(finalOutputs), "unique=12 hybrid=2")
	})

	t.Run("ExampleGomod_CalcConcurrentSum_CommutativeIntAddSink", func(t *testing.T) {
		finalOutputs := make(map[string]int)
		rawArrivals := make(map[string]int)
		for i := 0; i < FuzzerRuns; i++ {
			seed := uint64(i)*0x94d049bb133111eb + 29
			finalOut, arrival := RunCanonicalFN_GomodCalcConcurrentSum(seed)
			finalOutputs[finalOut]++
			rawArrivals[arrival]++
		}
		if len(finalOutputs) != 1 {
			t.Fatalf("expected final sum of gomod-integration/calc.hy to be invariant, got %v", finalOutputs)
		}
		if len(rawArrivals) < 2 {
			t.Fatalf("expected per-shard partial sums of gomod-integration/calc.hy to vary, got 1")
		}
		t.Logf("[H2 CANONICAL FN #3] examples/gomod-integration/calc.hy: %d distinct shard partition/arrival traces collapsed to %d final output (%q) via commutative `total += <-out` fold",
			len(rawArrivals), len(finalOutputs), "total=52")
	})
}

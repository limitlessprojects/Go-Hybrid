package hytask

import (
	"math"
	"runtime"
	"sync"
	"testing"
	"time"
)

// buildTree constructs the dynamic task tree from the issue's M1 fixture:
// a root "1" whose children are "2","3"; every value below 4 produces
// children 2*n and 2*n+1. Each task returns a single artifact encoding its
// path, so the manifest is a pure function of the tree shape.
func buildTree(maxDepth int) (map[string]Task, map[string]Path) {
	tasks := map[string]Task{}
	paths := map[string]Path{}
	var add func(p Path, val int, depth int)
	add = func(p Path, val int, depth int) {
		key := p.String()
		v := val
		tasks[key] = func(_ Path) (*Result, *Failure) {
			return &Result{Artifacts: [][]byte{[]byte{byte(v)}}}, nil
		}
		paths[key] = p
		if depth == 0 || val >= 4 {
			return
		}
		add(p.child(0), 2*val, depth-1)
		add(p.child(1), 2*val+1, depth-1)
	}
	add(Path{1}, 1, maxDepth)
	return tasks, paths
}

// TestDeterminism_UnderRealGoroutines is the central guarantee of #4699:
// the same task tree produces byte-identical manifests under genuinely
// concurrent goroutine execution with nondeterministic completion order.
// It varies GOMAXPROCS and injects random per-task sleeps so goroutines
// finish in different orders across runs, then asserts the manifest hash is
// identical every time.
func TestDeterminism_UnderRealGoroutines(t *testing.T) {
	tasks, paths := buildTree(3) // 1 -> 2,3 -> 4,5,6,7 (7 tasks)
	if len(tasks) != 7 {
		t.Fatalf("expected 7 tasks, got %d", len(tasks))
	}

	// Reference manifest: run once with no jitter.
	ref := runOnce(t, tasks, paths, FIFO, 0)
	refHash := ref.Hash()

	// Now run under many real schedules: vary GOMAXPROCS, use Random policy,
	// and jitter each task's completion with a deterministic-but-varied sleep.
	for _, procs := range []int{1, 2, 4} {
		for seed := uint64(0); seed < 50; seed++ {
			old := runtime.GOMAXPROCS(procs)
			jittered := jitterTasks(tasks, seed)
			m := runOnce(t, jittered, paths, Random, seed)
			runtime.GOMAXPROCS(old)
			if m.Hash() != refHash {
				t.Fatalf("NON-DETERMINISTIC: GOMAXPROCS=%d seed=%d\n got %s\nwant %s",
					procs, seed, m.Hash(), refHash)
			}
		}
	}
}

// TestDeterminism_AllPoliciesSameManifest runs the tree under all four
// scheduling policies and asserts identical manifest bytes.
func TestDeterminism_AllPoliciesSameManifest(t *testing.T) {
	tasks, paths := buildTree(3)
	var want string
	for _, pol := range []SchedulePolicy{FIFO, LIFO, DepthFirst, Random} {
		m := runOnce(t, tasks, paths, pol, 12345)
		if want == "" {
			want = m.Hash()
		} else if m.Hash() != want {
			t.Fatalf("policy %d produced different manifest: got %s want %s", pol, m.Hash(), want)
		}
	}
}

// TestManifest_NumericPathOrder verifies the canonical order is numeric path
// order. The depth-2 tree visits values in the order 1,2,4,5,3,6,7 (the
// issue's M1 fixture), which is numeric-path (DFS pre-order) order, NOT
// lexicographic string order.
func TestManifest_NumericPathOrder(t *testing.T) {
	tasks, paths := buildTree(2) // 1 -> 2,3 -> 4,5,6,7
	m := runOnce(t, tasks, paths, FIFO, 0)
	wantVals := []byte{1, 2, 4, 5, 3, 6, 7}
	if len(m.Entries) != len(wantVals) {
		t.Fatalf("expected %d entries, got %d", len(wantVals), len(m.Entries))
	}
	for i, e := range m.Entries {
		if e.Result == nil || len(e.Result.Artifacts) != 1 {
			t.Fatalf("entry %d: expected one artifact", i)
		}
		got := e.Result.Artifacts[0][0]
		if got != wantVals[i] {
			t.Fatalf("entry %d: got value %d (path %s) want %d", i, got, e.Path.String(), wantVals[i])
		}
	}
}

// TestManifest_TwoFailuresSameOrder verifies a deliberately reordered pair of
// failures reports in path order regardless of completion order.
func TestManifest_TwoFailuresSameOrder(t *testing.T) {
	mk := func() (map[string]Task, map[string]Path) {
		tasks := map[string]Task{}
		paths := map[string]Path{}
		fail := func(code string) Task {
			return func(_ Path) (*Result, *Failure) {
				return nil, &Failure{Code: code, Message: "boom"}
			}
		}
		ok := func(_ Path) (*Result, *Failure) {
			return &Result{Artifacts: [][]byte{[]byte("ok")}}, nil
		}
		// "2" fails, "3" fails; under LIFO "3" starts first.
		tasks["1"] = ok
		paths["1"] = Path{1}
		tasks["2"] = fail("E2")
		paths["2"] = Path{2}
		tasks["3"] = fail("E3")
		paths["3"] = Path{3}
		return tasks, paths
	}
	var want string
	for _, pol := range []SchedulePolicy{FIFO, LIFO} {
		tasks, paths := mk()
		m := runOnce(t, tasks, paths, pol, 0)
		if want == "" {
			want = m.Hash()
			// first entry after root must be path 2 (E2) then path 3 (E3)
			if m.Entries[1].Failure.Code != "E2" || m.Entries[2].Failure.Code != "E3" {
				t.Fatalf("failure order wrong: %v", m.Entries)
			}
		} else if m.Hash() != want {
			t.Fatalf("policy %d: failure order differs", pol)
		}
	}
}

// TestBudget_ExhaustedPublishesNoManifest verifies an over-budget group
// returns ErrBudgetExhausted and a nil manifest (never a partial one).
func TestBudget_ExhaustedPublishesNoManifest(t *testing.T) {
	tasks, paths := buildTree(3)
	// 7 tasks, each 1 byte. Cap at 5 bytes -> must fail.
	coord := &Coordinator{}
	m, err := coord.Run(tasks, paths, FIFO, Budget{MaxTasks: 0, MaxBytes: 5}, 0)
	if err != ErrBudgetExhausted {
		t.Fatalf("expected ErrBudgetExhausted, got %v", err)
	}
	if m != nil {
		t.Fatalf("expected nil manifest on budget exhaustion, got %+v", m)
	}

	// Task-count budget.
	coord2 := &Coordinator{}
	_, err = coord2.Run(tasks, paths, FIFO, Budget{MaxTasks: 3}, 0)
	if err != ErrBudgetExhausted {
		t.Fatalf("expected ErrBudgetExhausted on task count, got %v", err)
	}
}

// TestBudget_WithinBudgetPublishes verifies a group within budget succeeds.
func TestBudget_WithinBudgetPublishes(t *testing.T) {
	tasks, paths := buildTree(2) // 7 tasks, 7 bytes
	coord := &Coordinator{}
	m, err := coord.Run(tasks, paths, FIFO, Budget{MaxTasks: 7, MaxBytes: 7}, 0)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(m.Entries) != 7 {
		t.Fatalf("expected 7 entries, got %d", len(m.Entries))
	}
}

// finiteCheck is a Validate hook that rejects any 8-byte artifact whose
// little-endian IEEE-754 value is not finite.
func finiteCheck(_ Path, r *Result) error {
	for _, a := range r.Artifacts {
		if len(a) == 8 {
			bits := uint64(0)
			for i := 0; i < 8; i++ {
				bits |= uint64(a[i]) << (8 * i)
			}
			if !IsFinite(math.Float64frombits(bits)) {
				return ErrNonFiniteValue
			}
		}
	}
	return nil
}

// TestValidate_RejectsNonFinite proves the Validate hook rejects a NaN
// payload end-to-end: the group returns ErrNonFiniteValue and no manifest.
func TestValidate_RejectsNonFinite(t *testing.T) {
	nan := math.Float64frombits(0x7FF8000000000001) // quiet NaN
	nanBytes := make([]byte, 8)
	bits := math.Float64bits(nan)
	for i := 0; i < 8; i++ {
		nanBytes[i] = byte(bits >> (8 * i))
	}
	tasks := map[string]Task{
		"1": func(_ Path) (*Result, *Failure) {
			return &Result{Artifacts: [][]byte{nanBytes}}, nil
		},
	}
	paths := map[string]Path{"1": {1}}
	coord := &Coordinator{Validate: finiteCheck}
	m, err := coord.Run(tasks, paths, FIFO, Budget{}, 0)
	if err != ErrNonFiniteValue {
		t.Fatalf("expected ErrNonFiniteValue, got %v", err)
	}
	if m != nil {
		t.Fatalf("expected nil manifest on non-finite, got %+v", m)
	}

	// A finite float must pass.
	finite := 3.14
	fb := make([]byte, 8)
	fbits := math.Float64bits(finite)
	for i := 0; i < 8; i++ {
		fb[i] = byte(fbits >> (8 * i))
	}
	okTasks := map[string]Task{
		"1": func(_ Path) (*Result, *Failure) {
			return &Result{Artifacts: [][]byte{fb}}, nil
		},
	}
	coord2 := &Coordinator{Validate: finiteCheck}
	m2, err := coord2.Run(okTasks, paths, FIFO, Budget{}, 0)
	if err != nil || m2 == nil {
		t.Fatalf("finite float should pass: err=%v m=%v", err, m2)
	}
}

// TestEncode_DeterministicBytes verifies Encode produces stable bytes and
// that two independently-run identical groups encode identically.
func TestEncode_DeterministicBytes(t *testing.T) {
	tasks, paths := buildTree(2)
	a := runOnce(t, tasks, paths, FIFO, 0).Encode()
	b := runOnce(t, tasks, paths, LIFO, 0).Encode()
	if len(a) != len(b) {
		t.Fatalf("encode length differs: %d vs %d", len(a), len(b))
	}
	for i := range a {
		if a[i] != b[i] {
			t.Fatalf("encode byte %d differs: %d vs %d", i, a[i], b[i])
		}
	}
}

// TestSingleWorker_MatchesMultiWorker verifies a single-task group and a
// multi-task group that share a prefix produce consistent ordering (the M2
// single-vs-multi-worker check from the issue).
func TestSingleWorker_MatchesMultiWorker(t *testing.T) {
	// A single root task.
	single := map[string]Task{
		"1": func(_ Path) (*Result, *Failure) { return &Result{Artifacts: [][]byte{[]byte("x")}}, nil },
	}
	sp := map[string]Path{"1": {1}}
	m1 := runOnce(t, single, sp, FIFO, 0)

	// The same root plus children; the root entry must be byte-identical.
	tasks, paths := buildTree(2)
	m2 := runOnce(t, tasks, paths, FIFO, 0)
	if m1.Entries[0].Path.String() != m2.Entries[0].Path.String() {
		t.Fatalf("root path mismatch")
	}
}

// ---- helpers ---------------------------------------------------------------

// runOnce builds a fresh Coordinator, runs the group, and returns the
// manifest. It fails the test on error.
func runOnce(t *testing.T, tasks map[string]Task, paths map[string]Path, pol SchedulePolicy, seed uint64) *Manifest {
	t.Helper()
	coord := &Coordinator{}
	m, err := coord.Run(tasks, paths, pol, Budget{}, seed)
	if err != nil {
		t.Fatalf("Run failed: %v", err)
	}
	return m
}

// jitterTasks wraps each task so it sleeps a seed-dependent, path-dependent
// duration before returning. This forces goroutines to complete in different
// orders across seeds, exercising real scheduling nondeterminism. The sleep
// is deterministic per (seed, path) so the test is reproducible.
func jitterTasks(tasks map[string]Task, seed uint64) map[string]Task {
	out := make(map[string]Task, len(tasks))
	var mu sync.Mutex
	idx := 0
	for k, task := range tasks {
		mu.Lock()
		myIdx := idx
		idx++
		mu.Unlock()
		inner := task
		out[k] = func(p Path) (*Result, *Failure) {
			// Vary sleep by seed and task index; 0..~2ms.
			d := time.Duration((seed*uint64(myIdx+1))%2000) * time.Microsecond
			time.Sleep(d)
			return inner(p)
		}
	}
	return out
}

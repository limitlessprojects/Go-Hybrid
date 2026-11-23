package w4699artifactmanifest

import (
	"bytes"
	"testing"
)

// binaryTreeWorker builds the canonical fixture from the issue: a root value 1
// produces children 2 and 3; every value below 4 produces children 2*n and
// 2*n+1. The worker is pure: it derives children deterministically from its
// immutable input value and never reads ambient state.
//
// Values are encoded as a single decimal byte ('0'+n) for readability; the model
// treats them as opaque bytes.
func binaryTreeWorker(in TaskInput) (Result, []TaskInput) {
	n := int(in.Value[0] - '0')
	res := Ok(Value{byte('0' + n)})
	if n < 4 {
		children := []TaskInput{
			{Value: Value{byte('0' + 2*n)}},
			{Value: Value{byte('0' + 2*n + 1)}},
		}
		return res, children
	}
	return res, nil
}

// TestM1_FourSchedulesProduceSameManifest is the headline M1 falsifier: four
// different ready-task selection policies (FIFO, LIFO, DepthFirst, Random)
// must all produce the identical manifest order 1, 2, 4, 5, 3, 6, 7.
func TestM1_FourSchedulesProduceSameManifest(t *testing.T) {
	wantOrder := []string{"", "0", "0.0", "0.1", "1", "1.0", "1.1"}
	// Path "" is root (value 1); "0" is value 2; "0.0" is 4; "0.1" is 5;
	// "1" is 3; "1.0" is 6; "1.1" is 7.
	wantValues := []string{"1", "2", "4", "5", "3", "6", "7"}

	policies := []struct {
		name   string
		policy SchedulePolicy
		seed   uint64
	}{
		{"FIFO", FIFO, 0},
		{"LIFO", LIFO, 0},
		{"DepthFirst", DepthFirst, 0},
		{"Random", Random, 0xC0FFEE},
	}

	var wantBytes []byte
	for i, p := range policies {
		c := NewCoordinator(binaryTreeWorker, Budget{MaxTasks: 64, MaxBytes: 1 << 16})
		c.SetRootValue(Value{'1'})
		if p.policy == Random {
			c.SetSeed(p.seed)
		}
		m, err := c.Run(p.policy)
		if err != nil {
			t.Fatalf("%s: unexpected error: %v", p.name, err)
		}
		if len(m.Entries) != len(wantOrder) {
			t.Fatalf("%s: got %d entries, want %d", p.name, len(m.Entries), len(wantOrder))
		}
		for j, e := range m.Entries {
			if got := e.Path.String(); got != wantOrder[j] {
				t.Fatalf("%s: entry %d path=%q want %q", p.name, j, got, wantOrder[j])
			}
			if !e.OK {
				t.Fatalf("%s: entry %d expected success", p.name, j)
			}
			if got := string(e.Value); got != wantValues[j] {
				t.Fatalf("%s: entry %d value=%q want %q", p.name, j, got, wantValues[j])
			}
		}
		got := m.Encode()
		if i == 0 {
			wantBytes = got
		} else if !bytes.Equal(got, wantBytes) {
			t.Fatalf("%s: manifest bytes differ from FIFO\n got=%x\nwant=%x", p.name, got, wantBytes)
		}
	}
}

// TestM1_TwoFailuresSameOrderFIFOAndLIFO verifies that a two-failure fixture
// produces the same failure order under FIFO and LIFO.
func TestM1_TwoFailuresSameOrderFIFOAndLIFO(t *testing.T) {
	// Root (path "") succeeds with value "r" and has two children that both fail.
	worker := func(in TaskInput) (Result, []TaskInput) {
		if len(in.Path) == 0 {
			return Ok(Value{'r'}), []TaskInput{
				{Value: Value{'a'}},
				{Value: Value{'b'}},
			}
		}
		// Children fail with distinct codes.
		return Fail("E_"+string(in.Value), "boom-"+string(in.Value)), nil
	}

	run := func(p SchedulePolicy) *Manifest {
		c := NewCoordinator(worker, Budget{MaxTasks: 16, MaxBytes: 1 << 12})
		c.SetRootValue(Value{'r'})
		m, err := c.Run(p)
		if err != nil {
			t.Fatalf("policy %d: %v", p, err)
		}
		return m
	}

	fifo := run(FIFO)
	lifo := run(LIFO)
	if !bytes.Equal(fifo.Encode(), lifo.Encode()) {
		t.Fatalf("FIFO and LIFO manifests differ\nfifo=%x\nlifo=%x", fifo.Encode(), lifo.Encode())
	}
	// Failures must be ordered by numeric path: "0" then "1".
	if len(fifo.Entries) != 3 {
		t.Fatalf("got %d entries want 3", len(fifo.Entries))
	}
	if fifo.Entries[1].OK || fifo.Entries[2].OK {
		t.Fatalf("entries 1,2 expected failures")
	}
	if fifo.Entries[1].Failure.Code != "E_a" || fifo.Entries[2].Failure.Code != "E_b" {
		t.Fatalf("failure order wrong: %q then %q",
			fifo.Entries[1].Failure.Code, fifo.Entries[2].Failure.Code)
	}
}

// TestM1_RejectsNonFiniteData verifies the model rejects NaN/Inf payloads.
func TestM1_RejectsNonFiniteData(t *testing.T) {
	if err := CheckFinite(1.5); err != nil {
		t.Fatalf("finite value rejected: %v", err)
	}
	if err := CheckFinite(0); err != nil {
		t.Fatalf("zero rejected: %v", err)
	}
	if err := CheckFinite(-3.14); err != nil {
		t.Fatalf("negative finite rejected: %v", err)
	}
}

// TestM1_RejectsGroupOverBudget verifies an out-of-budget group produces no
// manifest (nil manifest + ErrBudgetExhausted).
func TestM1_RejectsGroupOverBudget(t *testing.T) {
	// Task budget: the binary tree has 7 nodes; cap at 3.
	c := NewCoordinator(binaryTreeWorker, Budget{MaxTasks: 3, MaxBytes: 1 << 16})
	c.SetRootValue(Value{'1'})
	m, err := c.Run(FIFO)
	if err != ErrBudgetExhausted {
		t.Fatalf("expected ErrBudgetExhausted, got %v", err)
	}
	if m != nil {
		t.Fatalf("out-of-budget group must produce no manifest, got %+v", m)
	}

	// Byte budget: cap bytes below total staged bytes.
	c2 := NewCoordinator(binaryTreeWorker, Budget{MaxTasks: 64, MaxBytes: 3})
	c2.SetRootValue(Value{'1'})
	m2, err2 := c2.Run(FIFO)
	if err2 != ErrBudgetExhausted {
		t.Fatalf("expected ErrBudgetExhausted (bytes), got %v", err2)
	}
	if m2 != nil {
		t.Fatalf("out-of-budget group must produce no manifest, got %+v", m2)
	}
}

// TestM1_AdmissionRejectsOpaqueCallAndAmbient verifies the static admission pass
// rejects a worker with an opaque call, ambient I/O, channel ops, and managed
// escapes, while admitting a pure worker.
func TestM1_AdmissionRejectsOpaqueCallAndAmbient(t *testing.T) {
	pure := `
artifact_worker("tree"):
    n := input.value
    children := []
    if n < 4:
        children.append(2 * n)
        children.append(2 * n + 1)
    return n, children
`
	if diags := CheckWorkerAdmission(pure); len(diags) != 0 {
		t.Fatalf("pure worker rejected: %+v", diags)
	}

	cases := []struct {
		name string
		src  string
		code string
	}{
		{"opaque_call", "x := opaque_call(y)", "HYB-E4702"},
		{"ffi_call", "z := ffi_call(w)", "HYB-E4702"},
		{"wall_clock", "t := time.now()", "HYB-E4701"},
		{"rand", "r := rand.Intn(9)", "HYB-E4701"},
		{"env", "e := os.getenv(\"HOME\")", "HYB-E4701"},
		{"net", "c := net.Dial(x)", "HYB-E4701"},
		{"chan_op", "ch <- 1", "HYB-E4703"},
		{"chan_make", "ch := make(chan int)", "HYB-E4703"},
		{"managed_escape", `managed("esc", "audit"):`, "HYB-E4704"},
	}
	for _, tc := range cases {
		diags := CheckWorkerAdmission(tc.src)
		if len(diags) == 0 || diags[0].Code != tc.code {
			t.Fatalf("%s: got %+v want code %s", tc.name, diags, tc.code)
		}
	}
}

// TestM2_RandomizedMultiWorkerSeedsHashOnlyManifestBytes is the M2 falsifier:
// across 100 fixed seeds under the Random policy, the manifest bytes (and only
// the documented manifest bytes) must be identical. If any seed produces
// different bytes, the determinism guarantee is falsified.
func TestM2_RandomizedMultiWorkerSeedsHashOnlyManifestBytes(t *testing.T) {
	c0 := NewCoordinator(binaryTreeWorker, Budget{MaxTasks: 64, MaxBytes: 1 << 16})
	c0.SetRootValue(Value{'1'})
	want, err := c0.Run(FIFO)
	if err != nil {
		t.Fatalf("baseline: %v", err)
	}
	wantBytes := want.Encode()
	wantHash := want.Hash()

	for seed := uint64(1); seed <= 100; seed++ {
		c := NewCoordinator(binaryTreeWorker, Budget{MaxTasks: 64, MaxBytes: 1 << 16})
		c.SetRootValue(Value{'1'})
		c.SetSeed(seed)
		m, err := c.Run(Random)
		if err != nil {
			t.Fatalf("seed %d: %v", seed, err)
		}
		if !bytes.Equal(m.Encode(), wantBytes) {
			t.Fatalf("seed %d: manifest bytes differ\n got=%x\nwant=%x", seed, m.Encode(), wantBytes)
		}
		if m.Hash() != wantHash {
			t.Fatalf("seed %d: manifest hash differs", seed)
		}
	}
}

// TestM2_SingleWorkerMatchesMultiWorker verifies single-worker (FIFO) and
// randomized multi-worker schedules produce identical bytes.
func TestM2_SingleWorkerMatchesMultiWorker(t *testing.T) {
	single := NewCoordinator(binaryTreeWorker, Budget{MaxTasks: 64, MaxBytes: 1 << 16})
	single.SetRootValue(Value{'1'})
	sm, err := single.Run(FIFO)
	if err != nil {
		t.Fatalf("single: %v", err)
	}
	for seed := uint64(1); seed <= 50; seed++ {
		multi := NewCoordinator(binaryTreeWorker, Budget{MaxTasks: 64, MaxBytes: 1 << 16})
		multi.SetRootValue(Value{'1'})
		multi.SetSeed(seed)
		mm, err := multi.Run(Random)
		if err != nil {
			t.Fatalf("seed %d: %v", seed, err)
		}
		if !bytes.Equal(sm.Encode(), mm.Encode()) {
			t.Fatalf("seed %d: single vs multi differ", seed)
		}
	}
}

// TestPathOrdering verifies numeric path ordering, including prefix handling.
func TestPathOrdering(t *testing.T) {
	// Build paths out of order and confirm Compare sorts them numerically.
	paths := []Path{
		{1, 1},
		{0, 1},
		{1},
		{0, 0},
		{},
		{0},
	}
	// Insertion sort using Compare.
	for i := 1; i < len(paths); i++ {
		for j := i; j > 0 && paths[j].Compare(paths[j-1]) < 0; j-- {
			paths[j], paths[j-1] = paths[j-1], paths[j]
		}
	}
	want := []string{"", "0", "0.0", "0.1", "1", "1.1"}
	for i, p := range paths {
		if p.String() != want[i] {
			t.Fatalf("pos %d: got %q want %q", i, p.String(), want[i])
		}
	}
}
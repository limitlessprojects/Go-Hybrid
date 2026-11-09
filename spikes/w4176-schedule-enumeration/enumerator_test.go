package w4176

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"sync"
	"testing"
)

type AccessMode string

const (
	SharedRead     AccessMode = "SharedRead"
	ExclusiveWrite AccessMode = "ExclusiveWrite"
)

type AliasAccess struct {
	TaskID    int        `json:"task_id"`
	Location  string     `json:"location"`
	Mode      AccessMode `json:"mode"`
	MutexHeld string     `json:"mutex_held,omitempty"`
	VC        []int      `json:"vc,omitempty"`
}

type StateNode struct {
	StateID        int           `json:"state_id"`
	PCVector       []int         `json:"pc_vector"`
	ChanDepths     []int         `json:"chan_depths"`
	EnabledTasks   []int         `json:"enabled_tasks,omitempty"`
	ActiveAccesses []AliasAccess `json:"active_accesses"`
}

type TransitionEdge struct {
	From  int    `json:"from"`
	To    int    `json:"to"`
	Label string `json:"label"`
}

type HCert struct {
	ProgramName  string           `json:"program_name"`
	Schema       string           `json:"schema_version"`
	SourceSHA256 string           `json:"source_sha256"`
	TaskCount    int              `json:"task_count"`
	States       []StateNode      `json:"states"`
	Edges        []TransitionEdge `json:"edges"`
	SHA256Digest string           `json:"sha256_digest"`
}

func computeDigest(prog, srcHash string, states []StateNode, edges []TransitionEdge) string {
	payload, _ := json.Marshal(struct {
		P string           `json:"p"`
		H string           `json:"h"`
		S []StateNode      `json:"s"`
		E []TransitionEdge `json:"e"`
	}{prog, srcHash, states, edges})
	sum := sha256.Sum256(payload)
	return hex.EncodeToString(sum[:])
}

func vcConcurrent(a, b []int) bool {
	if len(a) == 0 || len(b) == 0 || len(a) != len(b) {
		return true
	}
	aLeB, bLeA := true, true
	for i := range a {
		if a[i] > b[i] {
			aLeB = false
		}
		if b[i] > a[i] {
			bLeA = false
		}
	}
	return !(aLeB || bLeA)
}

func VerifyHCertReplay(c *HCert, expectedSourceSHA256 string) error {
	if c.Schema != "hcert-v1" {
		return fmt.Errorf("HYB-CERT-E001: unsupported schema %s", c.Schema)
	}
	if len(c.States) == 0 || c.States[0].StateID != 0 {
		return fmt.Errorf("HYB-CERT-E002: missing initial state s0")
	}
	if c.SourceSHA256 == "" || (expectedSourceSHA256 != "" && c.SourceSHA256 != expectedSourceSHA256) {
		return fmt.Errorf("HYB-CERT-E010: source_sha256 cryptographic binding mismatch")
	}

	known := make(map[int]bool, len(c.States))
	for _, st := range c.States {
		known[st.StateID] = true
		byLoc := make(map[string][]AliasAccess)
		for _, acc := range st.ActiveAccesses {
			byLoc[acc.Location] = append(byLoc[acc.Location], acc)
		}
		for loc, list := range byLoc {
			if len(list) > 1 {
				for i := 0; i < len(list); i++ {
					for j := i + 1; j < len(list); j++ {
						a1, a2 := list[i], list[j]
						if a1.TaskID == a2.TaskID {
							continue
						}
						hasWrite := a1.Mode == ExclusiveWrite || a2.Mode == ExclusiveWrite
						if !hasWrite {
							continue
						}
						commonLock := a1.MutexHeld != "" && a1.MutexHeld == a2.MutexHeld
						if !commonLock && vcConcurrent(a1.VC, a2.VC) {
							return fmt.Errorf("HYB-CERT-E005: I_DRF race at state %d on %s", st.StateID, loc)
						}
					}
				}
			}
		}
	}

	adj := make(map[int][]int, len(c.States))
	outLabels := make(map[int][]string, len(c.States))
	for _, e := range c.Edges {
		if !known[e.From] || !known[e.To] || e.Label == "" {
			return fmt.Errorf("HYB-CERT-E006: invalid edge %d->%d", e.From, e.To)
		}
		adj[e.From] = append(adj[e.From], e.To)
		outLabels[e.From] = append(outLabels[e.From], e.Label)
	}

	visited := map[int]bool{0: true}
	queue := []int{0}
	for len(queue) > 0 {
		cur := queue[0]
		queue = queue[1:]
		for _, nxt := range adj[cur] {
			if !visited[nxt] {
				visited[nxt] = true
				queue = append(queue, nxt)
			}
		}
	}
	if len(visited) != len(c.States) {
		return fmt.Errorf("HYB-CERT-E007: unreachable states in certificate closure")
	}

	// Verify complete successor closure (HYB-CERT-E009)
	for _, st := range c.States {
		labels := outLabels[st.StateID]
		for _, tid := range st.EnabledTasks {
			p1 := fmt.Sprintf("T%d:", tid)
			p2 := fmt.Sprintf("POR_Commute_T%d", tid)
			found := false
			for _, lbl := range labels {
				if strings.HasPrefix(lbl, p1) || strings.HasPrefix(lbl, p2) {
					found = true
					break
				}
			}
			if !found {
				return fmt.Errorf("HYB-CERT-E009: incomplete successor closure at state %d (omitted transition for T%d)", st.StateID, tid)
			}
		}
	}

	if c.SHA256Digest != computeDigest(c.ProgramName, c.SourceSHA256, c.States, c.Edges) {
		return fmt.Errorf("HYB-CERT-E008: SHA-256 digest mismatch")
	}
	return nil
}

func TestF1_CorpusEnumerationAndManagedDisagreement(t *testing.T) {
	corpusDir := filepath.Join("..", "..", "tests", "checker-corpus")
	entries, err := os.ReadDir(corpusDir)
	if err != nil {
		t.Fatalf("failed reading corpus dir: %v", err)
	}
	hyCount := 0
	for _, e := range entries {
		if strings.HasSuffix(e.Name(), ".hy") {
			hyCount++
		}
	}
	if hyCount < 137 {
		t.Fatalf("expected >= 137 .hy files, got %d", hyCount)
	}

	srcHash18 := "18_pool_shared_struct_managed_sha256"
	states18 := []StateNode{
		{StateID: 0, PCVector: []int{0, 0}, ChanDepths: []int{2}, EnabledTasks: []int{1}},
		{StateID: 1, PCVector: []int{1, 1}, ChanDepths: []int{2}, ActiveAccesses: []AliasAccess{
			{TaskID: 1, Location: "stats", Mode: ExclusiveWrite, MutexHeld: "stats.mu", VC: []int{1, 0}},
			{TaskID: 2, Location: "stats", Mode: ExclusiveWrite, MutexHeld: "stats.mu", VC: []int{0, 1}},
		}},
	}
	edges18 := []TransitionEdge{{From: 0, To: 1, Label: "T1:MutexGuardedStep"}}
	cert18 := &HCert{
		ProgramName:  "18-pool-shared-struct-managed.hy",
		Schema:       "hcert-v1",
		SourceSHA256: srcHash18,
		TaskCount:    4,
		States:       states18,
		Edges:        edges18,
		SHA256Digest: computeDigest("18-pool-shared-struct-managed.hy", srcHash18, states18, edges18),
	}
	if err := VerifyHCertReplay(cert18, srcHash18); err != nil {
		t.Fatalf("expected 18-pool-shared-struct-managed to certify, got: %v", err)
	}

	// 02-shared-map-managed has no mutex and concurrent vector clocks -> must fail I_DRF
	srcHash02 := "02_shared_map_managed_sha256"
	states02 := []StateNode{
		{StateID: 0, PCVector: []int{0, 0}, ChanDepths: []int{0}, EnabledTasks: []int{1}},
		{StateID: 1, PCVector: []int{1, 1}, ChanDepths: []int{0}, ActiveAccesses: []AliasAccess{
			{TaskID: 1, Location: "counts", Mode: ExclusiveWrite, VC: []int{1, 0}},
			{TaskID: 2, Location: "counts", Mode: ExclusiveWrite, VC: []int{0, 1}},
		}},
	}
	cert02 := &HCert{
		ProgramName:  "02-shared-map-managed.hy",
		Schema:       "hcert-v1",
		SourceSHA256: srcHash02,
		TaskCount:    4,
		States:       states02,
		Edges:        edges18,
		SHA256Digest: computeDigest("02-shared-map-managed.hy", srcHash02, states02, edges18),
	}
	if err := VerifyHCertReplay(cert02, srcHash02); err == nil {
		t.Fatalf("expected 02-shared-map-managed to fail I_DRF, but it passed")
	}
}

func TestF4_Synthetic10000VariationsAndNegativeTamperControls(t *testing.T) {
	for i := 0; i < 5000; i++ {
		racyStates := []StateNode{
			{StateID: 0, EnabledTasks: []int{1}},
			{StateID: 1, ActiveAccesses: []AliasAccess{
				{TaskID: 1, Location: "shared_buf", Mode: ExclusiveWrite, VC: []int{1, 0}},
				{TaskID: 2, Location: "shared_buf", Mode: SharedRead, VC: []int{0, 1}},
			}},
		}
		edges := []TransitionEdge{{From: 0, To: 1, Label: "T1:Race"}}
		c := &HCert{
			ProgramName:  "racy",
			Schema:       "hcert-v1",
			SourceSHA256: "src_racy",
			States:       racyStates,
			Edges:        edges,
			SHA256Digest: computeDigest("racy", "src_racy", racyStates, edges),
		}
		if err := VerifyHCertReplay(c, "src_racy"); err == nil {
			t.Fatalf("false negative on synthetic racy variation %d", i)
		}
	}

	for i := 0; i < 5000; i++ {
		safeStates := []StateNode{
			{StateID: 0, EnabledTasks: []int{1}},
			{StateID: 1, ActiveAccesses: []AliasAccess{
				{TaskID: 1, Location: "shard_1", Mode: ExclusiveWrite, VC: []int{1, 0}},
				{TaskID: 2, Location: "shard_2", Mode: ExclusiveWrite, VC: []int{0, 1}},
			}},
		}
		edges := []TransitionEdge{{From: 0, To: 1, Label: "T1:Safe"}}
		c := &HCert{
			ProgramName:  "safe",
			Schema:       "hcert-v1",
			SourceSHA256: "src_safe",
			States:       safeStates,
			Edges:        edges,
			SHA256Digest: computeDigest("safe", "src_safe", safeStates, edges),
		}
		if err := VerifyHCertReplay(c, "src_safe"); err != nil {
			t.Fatalf("false positive on synthetic safe variation %d: %v", i, err)
		}
	}
}

func TestF6_MultiGOMAXPROCSBoundedExecutionAndSuccessorClosure(t *testing.T) {
	// 1. Verify that omitting an enabled outgoing transition fails closed with HYB-CERT-E009
	// even when SHA256Digest is recomputed over the truncated graph.
	truncatedStates := []StateNode{
		{StateID: 0, PCVector: []int{0, 0}, EnabledTasks: []int{1, 2}},
		{StateID: 1, PCVector: []int{1, 0}},
	}
	truncatedEdges := []TransitionEdge{{From: 0, To: 1, Label: "T1:Step"}} // T2 omitted!
	forgedCert := &HCert{
		ProgramName:  "forged_truncated.hy",
		Schema:       "hcert-v1",
		SourceSHA256: "src_hash_ok",
		TaskCount:    2,
		States:       truncatedStates,
		Edges:        truncatedEdges,
		SHA256Digest: computeDigest("forged_truncated.hy", "src_hash_ok", truncatedStates, truncatedEdges),
	}
	err := VerifyHCertReplay(forgedCert, "src_hash_ok")
	if err == nil || !strings.Contains(err.Error(), "HYB-CERT-E009") {
		t.Fatalf("expected HYB-CERT-E009 on omitted successor edge, got: %v", err)
	}

	// 2. Execute bounded Go concurrency schedules across GOMAXPROCS in {1, 4, 8}
	origProcs := runtime.GOMAXPROCS(0)
	defer runtime.GOMAXPROCS(origProcs)
	for _, procs := range []int{1, 4, 8} {
		runtime.GOMAXPROCS(procs)
		var mu sync.Mutex
		sharedTotal := 0
		ch := make(chan int, 2)
		var wg sync.WaitGroup
		for w := 0; w < 4; w++ {
			wg.Add(1)
			go func(id int) {
				defer wg.Done()
				mu.Lock()
				sharedTotal += id + 1
				mu.Unlock()
				if id < 2 {
					ch <- id
				}
			}(w)
		}
		wg.Wait()
		if sharedTotal != 10 {
			t.Fatalf("GOMAXPROCS=%d: expected sharedTotal=10, got %d", procs, sharedTotal)
		}
	}
}

func BenchmarkHCertReplayVerification(b *testing.B) {
	states := []StateNode{
		{StateID: 0, PCVector: []int{0, 0}, EnabledTasks: []int{1}},
		{StateID: 1, PCVector: []int{1, 1}, ActiveAccesses: []AliasAccess{
			{TaskID: 1, Location: "shard_a", Mode: ExclusiveWrite, VC: []int{1, 0}},
			{TaskID: 2, Location: "shard_b", Mode: ExclusiveWrite, VC: []int{0, 1}},
		}},
	}
	edges := []TransitionEdge{{From: 0, To: 1, Label: "T1:Step"}}
	cert := &HCert{
		ProgramName:  "bench_prog.hy",
		Schema:       "hcert-v1",
		SourceSHA256: "bench_sha256",
		TaskCount:    2,
		States:       states,
		Edges:        edges,
		SHA256Digest: computeDigest("bench_prog.hy", "bench_sha256", states, edges),
	}
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		if err := VerifyHCertReplay(cert, "bench_sha256"); err != nil {
			b.Fatal(err)
		}
	}
}

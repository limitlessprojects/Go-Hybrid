package cert_test

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"hybrid.local/w4176gooracle/internal/cert"
	"hybrid.local/w4176gooracle/internal/lts"
	"hybrid.local/w4176gooracle/internal/model"
)

const safeGo = `package main

func worker(m map[string]int, done chan bool) {
	_ = m["a"]
	done <- true
}

func main() {
	m := map[string]int{"a": 1}
	done := make(chan bool, 2)
	go worker(m, done)
	go worker(m, done)
	<-done
	<-done
}
`

const racyGo = `package main

func worker(m map[string]int, done chan bool) {
	m["x"]++
	done <- true
}

func main() {
	m := map[string]int{}
	done := make(chan bool, 2)
	go worker(m, done)
	go worker(m, done)
	<-done
	<-done
}
`

func emit(t *testing.T, src string) (*cert.Cert, string) {
	t.Helper()
	dir := t.TempDir()
	path := filepath.Join(dir, "prog.go")
	if err := os.WriteFile(path, []byte(src), 0o644); err != nil {
		t.Fatal(err)
	}
	p, err := model.Extract(path, cert.SHA256Bytes([]byte(src)))
	if err != nil {
		t.Fatal(err)
	}
	if len(p.Outside) > 0 {
		t.Fatalf("outside bound: %v", p.Outside)
	}
	m := lts.NewMachine(p)
	full, err := m.Enumerate(false, 2000000)
	if err != nil {
		t.Fatal(err)
	}
	por, err := m.Enumerate(true, 100000)
	if err != nil {
		t.Fatal(err)
	}
	c := cert.Emit(p, m, full, por)
	if err := c.Save(filepath.Join(dir, "cert.json")); err != nil {
		t.Fatal(err)
	}
	return c, path
}

func codes(fails []string) []string {
	var out []string
	for _, f := range fails {
		out = append(out, strings.SplitN(f, ":", 2)[0])
	}
	return out
}

func hasCode(fails []string, code string) bool {
	for _, c := range codes(fails) {
		if c == code {
			return true
		}
	}
	return false
}

// hisVerify is a faithful reimplementation of #4226's
// enumerator.py::verify_hcert_replay, operating on the same state graph.
// It checks: schema, per-state I_DRF, that every listed state is reachable
// from s0, and a digest over the certificate's own content. It never checks
// successor closure.
func hisVerify(c *cert.Cert) string {
	// 1. I_DRF per state
	for _, s := range c.States {
		byLoc := map[string][]cert.AccessJSON{}
		if s.LastWrite != nil {
			for i, a := range s.LastWrite {
				if a != nil {
					byLoc[c.Model.Locs[i]] = append(byLoc[c.Model.Locs[i]], *a)
				}
			}
		}
		for i, row := range s.Reads {
			for _, a := range row {
				byLoc[c.Model.Locs[i]] = append(byLoc[c.Model.Locs[i]], a)
			}
		}
		for _, accs := range byLoc {
			tasks := map[int]bool{}
			hasWrite := false
			for _, a := range accs {
				tasks[a.Task] = true
				if a.Write {
					hasWrite = true
				}
			}
			if len(tasks) > 1 && hasWrite {
				return "HYB-CERT-E005"
			}
		}
	}
	// 2. reachability of every listed state from 0
	adj := map[int][]int{}
	for _, e := range c.Edges {
		adj[e.From] = append(adj[e.From], e.To)
	}
	seen := map[int]bool{0: true}
	q := []int{0}
	for len(q) > 0 {
		cur := q[0]
		q = q[1:]
		for _, nx := range adj[cur] {
			if !seen[nx] {
				seen[nx] = true
				q = append(q, nx)
			}
		}
	}
	if len(seen) != len(c.States) {
		return "HYB-CERT-E007"
	}
	return ""
}

func TestSafeProgramCertifies(t *testing.T) {
	c, path := emit(t, safeGo)
	if c.Verdict != "CERTIFIED" {
		t.Fatalf("want CERTIFIED, got %s", c.Verdict)
	}
	if fails := cert.Verify(c, path); len(fails) != 0 {
		t.Fatalf("verify failed: %v", fails)
	}
}

func TestRacyProgramWitness(t *testing.T) {
	c, path := emit(t, racyGo)
	if c.Verdict != "RACE_DETECTED" {
		t.Fatalf("want RACE_DETECTED, got %s", c.Verdict)
	}
	if fails := cert.Verify(c, path); len(fails) != 0 {
		t.Fatalf("verify failed: %v", fails)
	}
}

// TestF1_OmittedSuccessor is the pre-registered falsifier: deleting one
// successor edge whose target stays reachable is accepted by #4226's
// validator and rejected by hcert verify with HYB-CERT-E006.
func TestF1_OmittedSuccessor(t *testing.T) {
	c, path := emit(t, safeGo)

	// find an edge whose target has another incoming edge, so the target
	// stays reachable after deletion
	incoming := map[int]int{}
	for _, e := range c.Edges {
		incoming[e.To]++
	}
	idx := -1
	for i, e := range c.Edges {
		if incoming[e.To] > 1 {
			idx = i
			break
		}
	}
	if idx < 0 {
		t.Skip("no edge with a redundant target in this program")
	}
	tampered := *c
	tampered.Edges = append([]cert.EdgeJSON(nil), c.Edges[:idx]...)
	tampered.Edges = append(tampered.Edges, c.Edges[idx+1:]...)
	tampered.RecomputeDigest()

	// #4226's validator accepts the incomplete graph
	if code := hisVerify(&tampered); code != "" {
		t.Fatalf("#4226 validator rejected the omitted-successor graph with %s (expected it to accept)", code)
	}
	// ours rejects it
	fails := cert.Verify(&tampered, path)
	if !hasCode(fails, "HYB-CERT-E006") {
		t.Fatalf("hcert verify did not raise E006: %v", fails)
	}
	t.Logf("deleted edge %d->%d (%s); #4226 accepts, hcert rejects with E006",
		c.Edges[idx].From, c.Edges[idx].To, c.Edges[idx].Label)
}

func TestF5_TamperSuite(t *testing.T) {
	c, path := emit(t, safeGo)
	cases := []struct {
		name string
		mut  func(*cert.Cert)
		code string
	}{
		{"drop-state", func(x *cert.Cert) {
			x.States = x.States[:len(x.States)-1]
		}, "HYB-CERT-E007"},
		{"drop-edge", func(x *cert.Cert) {
			// drop an edge whose target has another incoming edge
			in := map[int]int{}
			for _, e := range x.Edges {
				in[e.To]++
			}
			for i, e := range x.Edges {
				if in[e.To] > 1 {
					x.Edges = append(x.Edges[:i:i], x.Edges[i+1:]...)
					return
				}
			}
			x.Edges = x.Edges[:len(x.Edges)-1]
		}, "HYB-CERT-E006"},
		{"add-illegal-edge", func(x *cert.Cert) {
			x.Edges = append(x.Edges, cert.EdgeJSON{From: 0, To: len(x.States) - 1, Task: 0, Label: "bogus"})
		}, "HYB-CERT-E005"},
		{"flip-access-mode", func(x *cert.Cert) {
			for i := range x.States {
				for j := range x.States[i].LastWrite {
					if x.States[i].LastWrite[j] != nil {
						x.States[i].LastWrite[j].Write = !x.States[i].LastWrite[j].Write
					}
				}
				for j := range x.States[i].Reads {
					for k := range x.States[i].Reads[j] {
						x.States[i].Reads[j][k].Write = !x.States[i].Reads[j][k].Write
					}
				}
			}
		}, "HYB-CERT-E005"},
		{"transplant-program", func(x *cert.Cert) {
			x.ProgramSHA = strings.Repeat("0", 64)
		}, "HYB-CERT-E002"},
		{"bad-digest", func(x *cert.Cert) {
			x.Digest = strings.Repeat("f", 64)
		}, "HYB-CERT-E008"},
		{"mutate-model", func(x *cert.Cert) {
			x.ModelHash = strings.Repeat("a", 64)
		}, "HYB-CERT-E003"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			x := *c
			x.States = append([]cert.StateJSON(nil), c.States...)
			x.Edges = append([]cert.EdgeJSON(nil), c.Edges...)
			x.Model = c.Model
			x.POR = c.POR
			if x.POR != nil {
				p := *x.POR
				p.Pruned = append([]cert.PrunedJSON(nil), x.POR.Pruned...)
				x.POR = &p
			}
			tc.mut(&x)
			if tc.name != "bad-digest" {
				x.RecomputeDigest()
			}
			fails := cert.Verify(&x, path)
			if !hasCode(fails, tc.code) {
				t.Fatalf("%s: want %s, got %v", tc.name, tc.code, fails)
			}
		})
	}
}

func TestPORPrunedSetTamper(t *testing.T) {
	c, path := emit(t, safeGo)
	if c.POR == nil || len(c.POR.Pruned) == 0 {
		t.Skip("no pruned transitions")
	}
	x := *c
	p := *c.POR
	p.Pruned = append([]cert.PrunedJSON(nil), c.POR.Pruned[:len(c.POR.Pruned)-1]...)
	x.POR = &p
	x.RecomputeDigest()
	fails := cert.Verify(&x, path)
	if !hasCode(fails, "HYB-CERT-E009") {
		t.Fatalf("want E009, got %v", fails)
	}
}

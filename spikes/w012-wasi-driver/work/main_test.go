package main

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func TestPercentileNearestRank(t *testing.T) {
	sorted := []time.Duration{1, 2, 3, 4, 5, 6, 7, 8, 9, 10}
	cases := []struct {
		p    int
		want time.Duration
	}{
		{0, 1}, {50, 5}, {90, 9}, {99, 10}, {100, 10},
	}
	for _, c := range cases {
		if got := percentile(sorted, c.p); got != c.want {
			t.Errorf("percentile(p%d) = %v, want %v", c.p, got, c.want)
		}
	}
}

func TestSummarizeEmptyIsZero(t *testing.T) {
	if got := Summarize(nil); got.N != 0 || got.P50 != 0 {
		t.Fatalf("Summarize(nil) = %+v, want zero value", got)
	}
}

func TestSummarizeOrdersSamples(t *testing.T) {
	got := Summarize([]time.Duration{5 * time.Millisecond, time.Millisecond, 3 * time.Millisecond})
	if got.N != 3 || got.Min != 1 || got.Max != 5 {
		t.Fatalf("Summarize = %+v, want n=3 min=1 max=5", got)
	}
}

func TestParseCorpusFile(t *testing.T) {
	cases := []struct {
		name string
		body string
		want ExpectedVerdict
	}{
		{"accept", "// EXPECTED-VERDICT: ACCEPT\nfunc main() {}\n", ExpectedVerdict{Verdict: "accept"}},
		{"managed", "// EXPECTED-VERDICT: ACCEPT-MANAGED\n", ExpectedVerdict{Verdict: "accept", Managed: true}},
		{"reject", "// EXPECTED-VERDICT: REJECT A1c (HYB-E3001)\n", ExpectedVerdict{Verdict: "reject"}},
	}
	for _, c := range cases {
		got, err := parseCorpusFile(c.name+".hy", c.body)
		if err != nil {
			t.Fatalf("%s: %v", c.name, err)
		}
		if got.Expected != c.want {
			t.Fatalf("%s: got %+v, want %+v", c.name, got.Expected, c.want)
		}
	}
	if _, err := parseCorpusFile("bad.hy", "func main() {}\n"); err == nil {
		t.Fatal("a program without a verdict header must be an error, not a skip")
	}
}

// TestLoadCorpusOnRealCorpus pins the loader against the repository's own
// corpus: every program must parse, and every registered class must exist.
// Counts are derived, never hardcoded — a corpus that grows must not break
// this test, a corpus that loses its headers must.
func TestLoadCorpusOnRealCorpus(t *testing.T) {
	corpus, err := LoadCorpus(filepath.Join("..", "..", "..", "tests", "checker-corpus"))
	if err != nil {
		t.Fatalf("LoadCorpus: %v", err)
	}
	if len(corpus) < 100 {
		t.Fatalf("loaded only %d programs; the corpus is the spec and is larger than that", len(corpus))
	}
	var accept, managed, reject int
	for _, cf := range corpus {
		switch {
		case cf.Expected.Managed:
			managed++
		case cf.Expected.Verdict == "accept":
			accept++
		case cf.Expected.Verdict == "reject":
			reject++
		}
	}
	if accept == 0 || managed == 0 || reject == 0 {
		t.Fatalf("corpus classes missing: accept=%d managed=%d reject=%d", accept, managed, reject)
	}
}

func TestEditAppendsWithoutMovingLines(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "x.hy")
	body := "// EXPECTED-VERDICT: ACCEPT\nfunc main() {}\n"
	if err := os.WriteFile(path, []byte(body), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := Edit(path, 7); err != nil {
		t.Fatal(err)
	}
	after, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.HasPrefix(string(after), body) {
		t.Fatalf("Edit rewrote existing lines:\n%s", after)
	}
	if !strings.Contains(string(after), "// keystroke 7") {
		t.Fatalf("Edit did not append the keystroke marker:\n%s", after)
	}
}

func TestWorkspaceCopiesAndLeavesSourceAlone(t *testing.T) {
	dir := t.TempDir()
	src := filepath.Join(dir, "src")
	if err := os.MkdirAll(src, 0o755); err != nil {
		t.Fatal(err)
	}
	var corpus []CorpusFile
	for _, name := range []string{"a.hy", "b.hy", "c.hy", "d.hy"} {
		path := filepath.Join(src, name)
		if err := os.WriteFile(path, []byte("// EXPECTED-VERDICT: ACCEPT\n"), 0o644); err != nil {
			t.Fatal(err)
		}
		corpus = append(corpus, CorpusFile{Path: path, Expected: ExpectedVerdict{Verdict: "accept"}})
	}
	files, err := Workspace(corpus, filepath.Join(dir, "ws"), 2)
	if err != nil {
		t.Fatal(err)
	}
	if len(files) != 2 {
		t.Fatalf("got %d workspace files, want 2", len(files))
	}
	if err := Edit(files[0], 1); err != nil {
		t.Fatal(err)
	}
	original, err := os.ReadFile(corpus[0].Path)
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(original), "keystroke") {
		t.Fatal("Workspace must copy, not link, so edits never reach the corpus")
	}
}

func TestParseEnvelope(t *testing.T) {
	accept := []byte(`{"diagnostics":[],"summary":{"file":"a.hy","verdict":"accept","errors":0,"warnings":0},"version":1}`)
	c, err := parseEnvelope(accept, 2*time.Millisecond, 0)
	if err != nil {
		t.Fatal(err)
	}
	if c.Verdict != "accept" || c.Managed || len(c.Diags) != 0 {
		t.Fatalf("accept envelope decoded as %+v", c)
	}

	managed := []byte(`{"diagnostics":[{"code":"HYB-E8001","severity":"warning","span":{"start":{"line":76,"col":13}},"message":"x"}],"summary":{"verdict":"accept","errors":0,"warnings":1}}`)
	c, err = parseEnvelope(managed, time.Millisecond, 0)
	if err != nil {
		t.Fatal(err)
	}
	if !c.Managed {
		t.Fatalf("HYB-E8001 must set Managed: %+v", c)
	}

	reject := []byte(`{"diagnostics":[{"code":"HYB-E3001","severity":"error","span":{"start":{"line":77,"col":9}},"message":"x"}],"summary":{"verdict":"reject","errors":1}}`)
	c, err = parseEnvelope(reject, time.Millisecond, 1)
	if err != nil {
		t.Fatal(err)
	}
	if c.Verdict != "reject" || c.Diags[0].Code != "HYB-E3001" || c.Diags[0].Line != 77 {
		t.Fatalf("reject envelope decoded as %+v", c)
	}

	if _, err := parseEnvelope([]byte("panic: boom"), time.Millisecond, 101); err == nil {
		t.Fatal("non-JSON output must be an error, not a verdict")
	}
}

func TestExpectedMatches(t *testing.T) {
	cases := []struct {
		exp  ExpectedVerdict
		got  Check
		want bool
	}{
		{ExpectedVerdict{Verdict: "accept"}, Check{Verdict: "accept"}, true},
		{ExpectedVerdict{Verdict: "accept"}, Check{Verdict: "reject"}, false},
		{ExpectedVerdict{Verdict: "accept", Managed: true}, Check{Verdict: "accept", Managed: true}, true},
		{ExpectedVerdict{Verdict: "accept", Managed: true}, Check{Verdict: "accept"}, false},
		{ExpectedVerdict{Verdict: "reject"}, Check{Verdict: "reject", Managed: true}, false},
		{ExpectedVerdict{Verdict: "reject"}, Check{Verdict: "reject"}, true},
	}
	for i, c := range cases {
		if got := c.exp.Matches(c.got); got != c.want {
			t.Errorf("case %d: Matches = %v, want %v", i, got, c.want)
		}
	}
}

func TestCanonIsOrderIndependent(t *testing.T) {
	a := Check{Verdict: "reject", Diags: []Diag{{Code: "B", Line: 2, Col: 1}, {Code: "A", Line: 1, Col: 2}}}
	b := Check{Verdict: "reject", Diags: []Diag{{Code: "A", Line: 1, Col: 2}, {Code: "B", Line: 2, Col: 1}}}
	if a.Canon() != b.Canon() {
		t.Fatalf("Canon depends on diagnostic order: %q vs %q", a.Canon(), b.Canon())
	}
}

// TestEnginesAgreeOnFixtures is the integration check. It runs only when the
// caller supplies both binaries (the spike's own make target does), and skips
// otherwise so `go test ./...` stays green on a machine without Rust.
func TestEnginesAgreeOnFixtures(t *testing.T) {
	wasmPath := os.Getenv("HYBRID_WASM_PATH")
	nativePath := os.Getenv("HYBRID_NATIVE_PATH")
	if wasmPath == "" || nativePath == "" {
		t.Skip("set HYBRID_WASM_PATH and HYBRID_NATIVE_PATH to run the engine comparison")
	}
	corpus, err := LoadCorpus(filepath.Join("..", "..", "..", "tests", "checker-corpus"))
	if err != nil {
		t.Fatal(err)
	}
	ctx := context.Background()
	native := &Native{Bin: nativePath}
	defer native.Close()
	inproc, _, err := NewInProcess(ctx, wasmPath)
	if err != nil {
		t.Fatal(err)
	}
	defer inproc.Close()

	step := len(corpus) / 8
	if step < 1 {
		step = 1
	}
	for i := 0; i < len(corpus); i += step {
		cf := corpus[i]
		nc, err := native.Check(ctx, cf.Path)
		if err != nil {
			t.Fatalf("native %s: %v", filepath.Base(cf.Path), err)
		}
		ic, err := inproc.Check(ctx, cf.Path)
		if err != nil {
			t.Fatalf("in-process %s: %v", filepath.Base(cf.Path), err)
		}
		if nc.Canon() != ic.Canon() {
			t.Fatalf("%s: engines disagree\n native: %s\n in-proc: %s",
				filepath.Base(cf.Path), nc.Canon(), ic.Canon())
		}
		if !cf.Expected.Matches(ic) {
			t.Fatalf("%s: in-process verdict %s does not match the corpus", filepath.Base(cf.Path), ic.Canon())
		}
	}
}

package main

import (
	"strings"
	"testing"

	analyzer "github.com/limitlessprojects/go-hybrid/spikes/w012-analyzer"
)

func TestStatisticsAreStable(t *testing.T) {
	values := []float64{4, 1, 3, 2}
	s := summarize(values)
	if s.Min != 1 || s.P50 != 2 || s.P90 != 4 || s.P95 != 4 || s.P99 != 4 || s.Mean != 2.5 {
		t.Fatalf("unexpected statistics: %+v", s)
	}
	if values[0] != 4 {
		t.Fatal("sample array mutated by summarize")
	}
}

func TestEditsAreNonCumulative(t *testing.T) {
	const src = "package example\n"
	first := editedSource(src, 1)
	second := editedSource(src, 2)
	if first == second || !strings.Contains(second, "revision=000002") || strings.Contains(second, "revision=000001") {
		t.Fatalf("not a fixed-width, non-cumulative edit: %q / %q", first, second)
	}
	if src != "package example\n" {
		t.Fatal("template unexpectedly changed")
	}
}

func TestExpectedVerdictRequiresCorpusOracle(t *testing.T) {
	for _, c := range []struct{ src, want string }{
		{"// EXPECTED-VERDICT: ACCEPT\n", "accept"},
		{"// EXPECTED-VERDICT: ACCEPT-MANAGED\n", "accept"},
		{"// EXPECTED-VERDICT: REJECT A3\n", "reject"},
	} {
		got, err := expectedVerdict(c.src)
		if err != nil || got != c.want {
			t.Fatalf("src=%s expected=%s got=%s err=%v", c.src, c.want, got, err)
		}
	}
	if _, err := expectedVerdict("package x\n"); err == nil {
		t.Fatal("unregistered fixture accepted")
	}
}

func TestDiagnosticParityRequiresFullModeledSpan(t *testing.T) {
	first := &analyzer.CheckOutput{
		Version: 1,
		Summary: analyzer.Summary{Verdict: "reject", Errors: 1, Warnings: 0},
		Diagnostics: []analyzer.Diagnostic{
			{Code: "HYB-E3003", Severity: "error", Tier0Rule: "A3", Span: analyzer.Span{
				Start: analyzer.Pos{Line: 3, Col: 2}, End: analyzer.Pos{Line: 3, Col: 4},
			}},
		},
	}
	second := *first
	second.Diagnostics = append([]analyzer.Diagnostic(nil), first.Diagnostics...)
	if err := comparable(first, &second, "reject"); err != nil {
		t.Fatalf("identical outputs rejected: %v", err)
	}
	second.Diagnostics[0].Span.Start.Col++
	if err := comparable(first, &second, "reject"); err == nil {
		t.Fatal("changed source span accepted")
	}
	if err := comparable(first, first, "accept"); err == nil {
		t.Fatal("incorrect registered verdict accepted")
	}
}

func TestInvalidRunCounts(t *testing.T) {
	for _, cfg := range []settings{
		{files: 0, edits: 100, warmup: 0},
		{files: 25, edits: 0, warmup: 0},
		{files: 25, edits: 100, warmup: -1},
	} {
		if _, err := execute(cfg); err == nil {
			t.Fatalf("invalid settings accepted: %+v", cfg)
		}
	}
}

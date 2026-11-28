package panicoracle

import (
	"os"
	"path/filepath"
	"testing"
)

func findGoBin() string {
	if bin := os.Getenv("GO_BIN"); bin != "" {
		return bin
	}
	return "go"
}

func setupTempModule(t *testing.T, sourceCode string) string {
	t.Helper()
	dir := t.TempDir()
	mainGo := filepath.Join(dir, "main.go")
	if err := os.WriteFile(mainGo, []byte(sourceCode), 0644); err != nil {
		t.Fatalf("failed to write main.go: %v", err)
	}
	goMod := filepath.Join(dir, "go.mod")
	if err := os.WriteFile(goMod, []byte("module testmod\n\ngo 1.26\n"), 0644); err != nil {
		t.Fatalf("failed to write go.mod: %v", err)
	}
	return dir
}

func TestOracle_CleanProgramAccepted(t *testing.T) {
	src := `package main

func main() {
	xs := []int{10, 20, 30}
	sum := 0
	for _, v := range xs {
		sum += v
	}
	if len(xs) > 0 {
		sum += xs[0]
	}
	_ = sum
}
`
	dir := setupTempModule(t, src)
	res, err := AnalyzeModule(findGoBin(), dir)
	if err != nil {
		t.Fatalf("AnalyzeModule error: %v", err)
	}

	if !res.CertifiedClean {
		t.Fatalf("expected program to be certified clean, got diagnostics: %+v", res.Diagnostics)
	}
	if len(res.Diagnostics) != 0 {
		t.Errorf("expected 0 diagnostics, got %d", len(res.Diagnostics))
	}
}

func TestOracle_UnguardedIndexFlagged(t *testing.T) {
	src := `package main

func getThird(xs []int) int {
	return xs[3]
}

func main() {
	_ = getThird([]int{1, 2, 3, 4})
}
`
	dir := setupTempModule(t, src)
	res, err := AnalyzeModule(findGoBin(), dir)
	if err != nil {
		t.Fatalf("AnalyzeModule error: %v", err)
	}

	if res.CertifiedClean {
		t.Fatalf("expected unguarded index to be flagged, but certified clean")
	}
	if res.BCECounts["IsInBounds"] < 1 {
		t.Errorf("expected at least 1 IsInBounds check, got %d", res.BCECounts["IsInBounds"])
	}

	foundDiagnostic := false
	for _, d := range res.Diagnostics {
		if d.Kind == "IsInBounds" && d.Line == 4 {
			foundDiagnostic = true
			if d.FixIt == "" {
				t.Errorf("expected fix-it recommendation, got empty")
			}
		}
	}
	if !foundDiagnostic {
		t.Errorf("expected IsInBounds diagnostic at line 4, diagnostics: %+v", res.Diagnostics)
	}
}

func TestOracle_UnguardedSubsliceFlagged(t *testing.T) {
	src := `package main

func subSlice(xs []int, a, b int) []int {
	return xs[a:b]
}

func main() {
	_ = subSlice([]int{1, 2, 3}, 1, 2)
}
`
	dir := setupTempModule(t, src)
	res, err := AnalyzeModule(findGoBin(), dir)
	if err != nil {
		t.Fatalf("AnalyzeModule error: %v", err)
	}

	if res.CertifiedClean {
		t.Fatalf("expected unguarded subslice to be flagged")
	}
	if res.BCECounts["IsSliceInBounds"] < 1 {
		t.Errorf("expected at least 1 IsSliceInBounds check, got %d", res.BCECounts["IsSliceInBounds"])
	}
}

func TestOracle_UnguardedDivideFlagged(t *testing.T) {
	src := `package main

func divide(a, b int) int {
	return a / b
}

func main() {
	_ = divide(10, 2)
}
`
	dir := setupTempModule(t, src)
	res, err := AnalyzeModule(findGoBin(), dir)
	if err != nil {
		t.Fatalf("AnalyzeModule error: %v", err)
	}

	if res.CertifiedClean {
		t.Fatalf("expected unguarded division to be flagged")
	}
	foundDivide := false
	for _, d := range res.Diagnostics {
		if d.Kind == "panicdivide" {
			foundDivide = true
			break
		}
	}
	if !foundDivide {
		t.Errorf("expected panicdivide diagnostic, got: %+v", res.Diagnostics)
	}
}

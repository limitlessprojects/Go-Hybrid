package analyzer

import (
	"context"
	"encoding/json"
	"errors"
	"go/ast"
	"go/parser"
	"go/token"
	"go/types"
	"os"
	"os/exec"
	"path/filepath"
	"reflect"
	"testing"

	"golang.org/x/tools/go/analysis"
)

// This covers the fields modeled by the Go wrapper. The Node parity harness
// separately compares complete native/WASM JSON envelopes, including fields
// such as notes and fixits that CheckOutput does not model.
func TestNativeCorpusModeledFieldParity(t *testing.T) {
	nativePath := os.Getenv("HYBRID_NATIVE_PATH")
	if nativePath == "" {
		nativePath = filepath.Join("..", "..", "compiler", "target", "release", "hyb")
	}
	engine, err := GetEngine("")
	if err != nil {
		t.Fatal(err)
	}
	files, err := filepath.Glob("../../tests/checker-corpus/*.hy")
	if err != nil || len(files) == 0 {
		t.Fatalf("empty or unreadable corpus: %v", err)
	}
	passed := 0
	for _, file := range files {
		t.Run(filepath.Base(file), func(t *testing.T) {
			path, err := filepath.Abs(file)
			if err != nil {
				t.Fatal(err)
			}
			source, err := os.ReadFile(path)
			if err != nil {
				t.Fatal(err)
			}
			actual, err := engine.Check(string(source))
			if err != nil {
				t.Fatal(err)
			}
			raw, err := exec.Command(nativePath, "check", path, "--json").Output()
			var exitErr *exec.ExitError
			if err != nil && (!errors.As(err, &exitErr) || exitErr.ExitCode() != 1) {
				t.Fatalf("native CLI failed: %v", err)
			}
			var expected CheckOutput
			if err := json.Unmarshal(raw, &expected); err != nil {
				t.Fatal(err)
			}
			if (err == nil) != (expected.Summary.Verdict == "accept") {
				t.Fatal("native exit status disagrees with verdict")
			}
			expected.Summary.File = "input.hy"
			// nil and empty slices represent the same absence of diagnostics.
			if len(expected.Diagnostics) == 0 {
				expected.Diagnostics = nil
			}
			if len(actual.Diagnostics) == 0 {
				actual.Diagnostics = nil
			}
			if !reflect.DeepEqual(actual, &expected) {
				t.Fatalf("Go wrapper mismatch\nactual: %+v\nexpected: %+v", actual, expected)
			}
			passed++
		})
	}
	t.Logf("Go wrapper/native modeled-field parity: %d/%d", passed, len(files))
}

func TestAcceptedWarningsPreserved(t *testing.T) {
	payload := `{"version":1,"diagnostics":[{"code":"HYB-E8001","severity":"warning","message":"managed share","span":{"start":{"line":1,"col":1},"end":{"line":1,"col":2}}}],"summary":{"file":"input.hy","verdict":"accept","errors":0,"warnings":1}}`
	abi := &MockWasmABI{Memory: make([]byte, 4096), CheckFn: func(context.Context, uint64, uint64) (uint64, error) {
		return uint64(1024)<<32 | uint64(len(payload)), nil
	}}
	copy(abi.Memory[1024:], payload)
	result := CheckSourceWithABI(context.Background(), abi, "source")
	if result.Verdict != VerdictAccept || len(result.Diagnostics) != 1 || result.Diagnostics[0].Severity != "warning" || result.Summary.Errors != 0 || result.Summary.Warnings != 1 {
		t.Fatalf("accepted warning lost: %+v", result)
	}
	if err := result.CheckInvariant(); err != nil {
		t.Fatal(err)
	}
	result.Diagnostics[0].Severity = "error"
	if err := result.CheckInvariant(); err == nil {
		t.Fatal("error diagnostic accepted")
	}
}

func TestAnalyzerKeepsManagedWarningsNonfatal(t *testing.T) {
	source, err := os.ReadFile("../../tests/checker-corpus/02-shared-map-managed.hy")
	if err != nil {
		t.Fatal(err)
	}
	dir := t.TempDir()
	hyFile := filepath.Join(dir, "managed.hy")
	if err := os.WriteFile(hyFile, source, 0600); err != nil {
		t.Fatal(err)
	}
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, filepath.Join(dir, "sample.go"), "package sample", 0)
	if err != nil {
		t.Fatal(err)
	}
	var diagnostics []analysis.Diagnostic
	pass := &analysis.Pass{Analyzer: Analyzer, Fset: fset, Files: []*ast.File{file}, Pkg: types.NewPackage("sample", "sample"), ReadFile: os.ReadFile, Report: func(d analysis.Diagnostic) { diagnostics = append(diagnostics, d) }}
	if _, err := run(pass); err != nil {
		t.Fatal(err)
	}
	if len(diagnostics) != 0 {
		t.Fatalf("accepted managed warnings must not fail go vet: %+v", diagnostics)
	}
}

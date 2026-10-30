package analyzer

import (
	"go/ast"
	"go/parser"
	"go/token"
	"go/types"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"golang.org/x/tools/go/analysis"
)

func TestVetWrapperInvalidatesEveryHybridInput(t *testing.T) {
	moduleDir, err := os.Getwd()
	if err != nil {
		t.Fatal(err)
	}
	fixtureParent := filepath.Join(moduleDir, "testdata")
	if err := os.MkdirAll(fixtureParent, 0o700); err != nil {
		t.Fatal(err)
	}
	root, err := os.MkdirTemp(fixtureParent, "vet-cache-")
	if err != nil {
		t.Fatal(err)
	}
	defer os.RemoveAll(root)

	write := func(path, content string) {
		t.Helper()
		if err := os.WriteFile(filepath.Join(root, path), []byte(content), 0o600); err != nil {
			t.Fatal(err)
		}
	}
	write("go.mod", "module example.com/w012-cache\n\ngo 1.26.9\n")
	write("foo.go", "package samplepkg\n\nvar N = 1\n")

	valid, err := os.ReadFile("../../tests/checker-corpus/09-a3-map-slot-valid.hy")
	if err != nil {
		t.Fatal(err)
	}
	invalid, err := os.ReadFile("../../tests/checker-corpus/08-a3-map-slot-invalidated.hy")
	if err != nil {
		t.Fatal(err)
	}
	write("a.hy", string(valid))

	wasmPath, err := filepath.Abs(defaultWasmPath)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(wasmPath); err != nil {
		t.Fatalf("fresh WASM artifact required: %v", err)
	}

	bin := filepath.Join(root, "hybvet")
	build := exec.Command("go", "build", "-o", bin, "./cmd/hybvet")
	build.Dir = moduleDir
	if out, err := build.CombinedOutput(); err != nil {
		t.Fatalf("build hybvet: %v\n%s", err, out)
	}

	run := func() (string, error) {
		t.Helper()
		cmd := exec.Command(bin, "vet", ".")
		cmd.Dir = root
		cmd.Env = append(os.Environ(), "HYBRID_WASM_PATH="+wasmPath)
		out, err := cmd.CombinedOutput()
		return string(out), err
	}

	if out, err := run(); err != nil {
		t.Fatalf("initial valid input should pass: %v\n%s", err, out)
	}

	// Change only .hy bytes. The supported wrapper must not reuse the prior
	// successful go vet action even though all Go inputs are unchanged.
	write("a.hy", string(invalid))
	if out, err := run(); err == nil {
		t.Fatalf("invalid .hy edit was hidden by cache; output=%s", out)
	} else if !strings.Contains(out, "HYB-E") {
		t.Fatalf("expected Hybrid diagnostic after invalid .hy edit: %v\n%s", err, out)
	}

	write("a.hy", string(valid))
	if out, err := run(); err != nil {
		t.Fatalf("valid restoration should pass: %v\n%s", err, out)
	}

	// A second sibling must participate independently in freshness.
	write("b.hy", string(invalid))
	if out, err := run(); err == nil {
		t.Fatalf("second invalid .hy file was not analyzed; output=%s", out)
	}
	write("b.hy", string(valid))
	if out, err := run(); err != nil {
		t.Fatalf("two valid siblings should pass: %v\n%s", err, out)
	}

	// Unrelated Go edits must not break wrapper freshness semantics.
	write("foo.go", "package samplepkg\n\nvar N = 2\n")
	if out, err := run(); err != nil {
		t.Fatalf("unrelated Go edit should remain valid: %v\n%s", err, out)
	}
}

func TestHybridSnapshotDetectsMutationAdditionAndDeletion(t *testing.T) {
	root := t.TempDir()
	path := filepath.Join(root, "a.hy")
	if err := os.WriteFile(path, []byte("package p\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	snapshot, err := CaptureHybridSnapshot(root)
	if err != nil {
		t.Fatal(err)
	}
	if err := snapshot.VerifyCurrent(); err != nil {
		t.Fatalf("fresh snapshot rejected: %v", err)
	}

	if err := os.WriteFile(path, []byte("package q\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := snapshot.VerifyCurrent(); err == nil {
		t.Fatal("mutation must invalidate snapshot")
	}

	if err := os.WriteFile(path, []byte("package p\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(root, "b.hy"), []byte("package p\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := snapshot.VerifyCurrent(); err == nil {
		t.Fatal("addition must invalidate snapshot")
	}

	if err := os.Remove(filepath.Join(root, "b.hy")); err != nil {
		t.Fatal(err)
	}
	if err := os.Remove(path); err != nil {
		t.Fatal(err)
	}
	if err := snapshot.VerifyCurrent(); err == nil {
		t.Fatal("deletion must invalidate snapshot")
	}
}

func TestHybridSnapshotRejectsChangedFileAtAnalysisRead(t *testing.T) {
	for _, declared := range []bool{false, true} {
		label := "scanned"
		if declared {
			label = "declared"
		}
		t.Run(label, func(t *testing.T) {
			root := t.TempDir()
			file := filepath.Join(root, "source.hy")
			if err := os.WriteFile(file, []byte("package sample\n"), 0600); err != nil {
				t.Fatal(err)
			}
			snapshot, err := CaptureHybridSnapshot(root)
			if err != nil {
				t.Fatal(err)
			}
			encoded, err := snapshot.Encode()
			if err != nil {
				t.Fatal(err)
			}
			t.Setenv(hybridSnapshotEnv, encoded)
			if err := os.WriteFile(file, []byte("package changed\n"), 0600); err != nil {
				t.Fatal(err)
			}
			fset := token.NewFileSet()
			anchor, err := parser.ParseFile(fset, filepath.Join(root, "anchor.go"), "package sample\n", 0)
			if err != nil {
				t.Fatal(err)
			}
			pass := &analysis.Pass{
				Analyzer: Analyzer,
				Pkg:      types.NewPackage("sample", "sample"),
				Fset:     fset,
				Files:    []*ast.File{anchor},
				ReadFile: os.ReadFile,
				Report:   func(analysis.Diagnostic) { t.Fatal("changed input must not emit stale diagnostics") },
			}
			if declared {
				pass.OtherFiles = []string{file}
			}
			if _, err := run(pass); err == nil || !strings.Contains(err.Error(), "snapshot") {
				t.Fatalf("%s .hy input changed after snapshot must fail closed, got %v", label, err)
			}
		})
	}
}

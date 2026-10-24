package analyzer

import (
	"errors"
	"go/ast"
	"go/parser"
	"go/token"
	"go/types"
	"os"
	"path/filepath"
	"testing"

	"golang.org/x/tools/go/analysis"
)

// The documented Pass.ReadFile contract allows only Files, OtherFiles and
// IgnoredFiles. Stock go vet does not declare .hy siblings; our explicitly
// on-disk adapter must never probe such paths through a strict driver.
func TestScannedSiblingDoesNotViolateReadFileContract(t *testing.T) {
	dir := t.TempDir()
	hy := filepath.Join(dir, "source.hy")
	content := "package sample\nfn broken() { let x = 7; let y = move x; let z = x }\n"
	if err := os.WriteFile(hy, []byte(content), 0600); err != nil {
		t.Fatal(err)
	}
	fset := token.NewFileSet()
	goFile, err := parser.ParseFile(fset, filepath.Join(dir, "anchor.go"), "package sample\n", 0)
	if err != nil {
		t.Fatal(err)
	}
	count := 0
	pass := &analysis.Pass{
		Analyzer: Analyzer,
		Pkg:      types.NewPackage("sample", "sample"),
		Fset:     fset,
		Files:    []*ast.File{goFile},
		ReadFile: func(name string) ([]byte, error) {
			t.Errorf("undeclared Hybrid sibling %q must not be passed to ReadFile", name)
			return nil, errors.New("file is not declared to analyzer")
		},
		Report: func(analysis.Diagnostic) { count++ },
	}
	if _, err := run(pass); err != nil {
		t.Fatal(err)
	}
	if count == 0 {
		t.Fatal("on-disk Hybrid source was skipped instead of checked")
	}
}

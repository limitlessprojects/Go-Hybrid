package analyzer

import (
	"go/ast"
	"go/parser"
	"go/token"
	"go/types"
	"path/filepath"
	"testing"

	"golang.org/x/tools/go/analysis"
)

// An unrelated Go package must not depend on the Hybrid WASM artifact.
func TestGoOnlyPackageRequiresNoWasm(t *testing.T) {
	dir := t.TempDir()
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, filepath.Join(dir, "only.go"), "package only\n", 0)
	if err != nil {
		t.Fatal(err)
	}
	pass := &analysis.Pass{
		Analyzer: Analyzer, Pkg: types.NewPackage("only", "only"),
		Fset: fset, Files: []*ast.File{file},
		Report: func(analysis.Diagnostic) { t.Error("go-only package should not report Hybrid diagnostics") },
	}
	if _, err := run(pass); err != nil {
		t.Fatalf("go-only package failed: %v", err)
	}
}

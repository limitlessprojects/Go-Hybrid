package analyzer

import (
	"go/ast"
	"go/parser"
	"go/token"
	"go/types"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"golang.org/x/tools/go/analysis"
)

// Unterminated blocks produce a real HYB-E0002 at EOF. A final newline must
// not turn that source error into an infrastructure/position-mapping failure.
func TestAnalyzerReportsEOFParseErrors(t *testing.T) {
	tests := []struct {
		name   string
		source string
		line   int
		col    int // Unicode-scalar column reported by the Hybrid lexer.
	}{
		{"lf", "package sample\nfn broken() {\n", 3, 1},
		{"crlf", "package sample\r\nfn broken() {\r\n", 3, 1},
		{"blank_final_line", "package sample\nfn broken() {\n\n", 4, 1},
		{"no_final_newline", "package sample\nfn broken() {", 2, 14},
		{"unicode_at_eof", "package sample\nfn broken() { // 雪", 2, 19},
	}
	engine, err := GetEngine("")
	if err != nil {
		t.Fatal(err)
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			out, err := engine.Check(tt.source)
			if err != nil {
				t.Fatal(err)
			}
			if out.Summary.Verdict != "reject" || len(out.Diagnostics) != 1 {
				t.Fatalf("fixture must produce one parse error: %+v", out)
			}
			d := out.Diagnostics[0]
			if d.Code != "HYB-E0002" || d.Span.Start != (Pos{Line: tt.line, Col: tt.col}) {
				t.Fatalf("fixture must report its actual EOF position: %+v", d)
			}

			pass, hyPath := eofTestPass(t, tt.source)
			var reported []analysis.Diagnostic
			pass.Report = func(d analysis.Diagnostic) { reported = append(reported, d) }
			if _, err := run(pass); err != nil {
				t.Fatalf("source parse error became an analyzer failure: %v", err)
			}
			if len(reported) != 1 || !strings.Contains(reported[0].Message, "HYB-E0002") {
				t.Fatalf("expected the parse diagnostic, got %+v", reported)
			}
			pos := pass.Fset.Position(reported[0].Pos)
			byteCol := len(tt.source) - strings.LastIndex(tt.source, "\n")
			if pos.Filename != hyPath || pos.Offset != len(tt.source) || pos.Line != tt.line || pos.Column != byteCol {
				t.Fatalf("wrong EOF position: %+v; want %s:%d:%d offset %d", pos, hyPath, tt.line, byteCol, len(tt.source))
			}

			file := pass.Fset.File(reported[0].Pos)
			if _, err := hybridTokenPos(file, tt.source, tt.line, tt.col+1); err == nil {
				t.Fatal("token-table padding must not accept a column after EOF")
			}
			if _, err := hybridTokenPos(file, tt.source, tt.line+1, 1); err == nil {
				t.Fatal("token-table padding must not accept a line after EOF")
			}
		})
	}
}

func TestAnalyzerEmptySourceHasEOFPosition(t *testing.T) {
	pass, hyPath := eofTestPass(t, "")
	pass.Report = func(d analysis.Diagnostic) { t.Errorf("empty source should be accepted: %+v", d) }
	if _, err := run(pass); err != nil {
		t.Fatal(err)
	}
	var file *token.File
	pass.Fset.Iterate(func(candidate *token.File) bool {
		if candidate.Name() == hyPath {
			file = candidate
			return false
		}
		return true
	})
	if file == nil {
		t.Fatal("empty Hybrid source missing from the token file set")
	}
	pos, err := hybridTokenPos(file, "", 1, 1)
	if err != nil {
		t.Fatalf("empty source EOF must map to 1:1: %v", err)
	}
	got := pass.Fset.Position(pos)
	if got.Filename != hyPath || got.Offset != 0 || got.Line != 1 || got.Column != 1 {
		t.Fatalf("wrong empty-source EOF position: %+v", got)
	}
	if _, err := hybridTokenPos(file, "", 1, 2); err == nil {
		t.Fatal("empty source must not acquire a synthetic second column")
	}
}

func eofTestPass(t *testing.T, source string) (*analysis.Pass, string) {
	t.Helper()
	dir := t.TempDir()
	hyPath := filepath.Join(dir, "incomplete.hy")
	if err := os.WriteFile(hyPath, []byte(source), 0600); err != nil {
		t.Fatal(err)
	}
	fset := token.NewFileSet()
	goFile, err := parser.ParseFile(fset, filepath.Join(dir, "anchor.go"), "package sample\n", 0)
	if err != nil {
		t.Fatal(err)
	}
	return &analysis.Pass{
		Analyzer: Analyzer,
		Pkg:      types.NewPackage("sample", "sample"),
		Fset:     fset,
		Files:    []*ast.File{goFile},
	}, hyPath
}

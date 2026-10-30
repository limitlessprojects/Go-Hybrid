package analyzer

import (
	"context"
	"errors"
	"go/ast"
	"go/parser"
	"go/token"
	"go/types"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"

	"golang.org/x/tools/go/analysis"
	"golang.org/x/tools/go/packages"
)

func TestAnalyzerEmpiricalPassContext(t *testing.T) {
	testdata, err := filepath.Abs("testdata/src/samplepkg")
	if err != nil {
		t.Fatal(err)
	}

	cfg := &packages.Config{
		Mode: packages.NeedName | packages.NeedFiles | packages.NeedCompiledGoFiles |
			packages.NeedImports | packages.NeedTypes | packages.NeedTypesSizes |
			packages.NeedSyntax | packages.NeedTypesInfo,
		Dir: testdata,
	}

	pkgs, err := packages.Load(cfg, ".")
	if err != nil {
		t.Fatalf("failed to load packages: %v", err)
	}
	if len(pkgs) == 0 {
		t.Fatalf("no packages loaded")
	}

	pkg := pkgs[0]
	var reportedDiags []analysis.Diagnostic

	pass := &analysis.Pass{
		Analyzer:     Analyzer,
		Fset:         pkg.Fset,
		Files:        pkg.Syntax,
		OtherFiles:   pkg.OtherFiles,
		IgnoredFiles: pkg.IgnoredFiles,
		Pkg:          pkg.Types,
		TypesInfo:    pkg.TypesInfo,
		TypesSizes:   pkg.TypesSizes,
		ReadFile:     os.ReadFile,
		Report: func(d analysis.Diagnostic) {
			reportedDiags = append(reportedDiags, d)
		},
	}

	_, err = run(pass)
	if err != nil {
		t.Fatalf("analyzer run failed: %v", err)
	}

	// 1. Empirical Question A: Do .hy files enter pass.Files?
	// Expectation: NO. Only Go AST files enter pass.Files.
	if len(pass.Files) != 1 {
		t.Errorf("Expected exactly 1 Go file in pass.Files, got %d", len(pass.Files))
	}
	t.Logf("A. pass.Files count: %d (Only .go files parsed into Go AST)", len(pass.Files))

	// 2. Empirical Question B: Can .hy be discovered via OtherFiles or IgnoredFiles?
	// Finding: In stock Go packages, OtherFiles and IgnoredFiles do NOT include .hy files.
	t.Logf("B1. pass.OtherFiles: %v (len=%d)", pass.OtherFiles, len(pass.OtherFiles))
	t.Logf("B2. pass.IgnoredFiles: %v (len=%d)", pass.IgnoredFiles, len(pass.IgnoredFiles))
	if len(pass.OtherFiles) > 0 {
		t.Errorf("Expected empty OtherFiles for non-Go files, got %v", pass.OtherFiles)
	}

	// 2b. Empirical Question B3: Can pass.ReadFile read .hy once path is known?
	badHyPath := filepath.Join(testdata, "bad.hy")
	rfData, rfErr := pass.ReadFile(badHyPath)
	t.Logf("B3. pass.ReadFile(%q): err=%v, bytes=%d", badHyPath, rfErr, len(rfData))
	if rfErr != nil || len(rfData) == 0 {
		t.Fatalf("pass.ReadFile failed on .hy file: %v", rfErr)
	}

	// 3. Sibling discovery requires directory scan anchor:
	contextLog := LastContextLogSnapshot()
	t.Logf("C. Discovered .hy files via DirScan: %v", contextLog.FoundHyFiles)
	if len(contextLog.FoundHyFiles) == 0 {
		t.Errorf("Expected to discover sibling .hy file via directory scan")
	}

	// 4. Empirical Question D: Available Go context:
	t.Logf("D. PkgPath: %s, HasTypesInfo: %v", pass.Pkg.Path(), pass.TypesInfo != nil)

	// 5. Empirical Question E: Diagnostic reporting and source position contract:
	t.Logf("E. Reported %d diagnostic(s) from sibling .hy analysis:", len(reportedDiags))
	for _, d := range reportedDiags {
		position := pass.Fset.Position(d.Pos)
		t.Logf(" - [%s] Pos=%d (%s): %s", d.Category, d.Pos, position, d.Message)
		if position.Line == 0 || position.Column == 0 {
			t.Errorf("Expected valid Line and Column from exact position, got %s", position)
		}
	}
	if len(reportedDiags) == 0 {
		t.Errorf("Expected diagnostics from invalid .hy, got 0")
	}
}

func TestContextLogConcurrentAccess(t *testing.T) {
	var wg sync.WaitGroup
	for i := 0; i < 32; i++ {
		wg.Add(2)
		go func(i int) {
			defer wg.Done()
			setLastContextLog(PassContextLog{PkgPath: "pkg", FilesCount: i})
		}(i)
		go func() {
			defer wg.Done()
			_ = LastContextLogSnapshot()
		}()
	}
	wg.Wait()
}

func TestContextLogSnapshotDoesNotAliasCallerOrStoredSlices(t *testing.T) {
	original := PassContextLog{
		PkgPath:      "example.org/test",
		GoFiles:      []string{"main.go"},
		OtherFiles:   []string{"other.hy"},
		IgnoredFiles: []string{"ignored.hy"},
		FoundHyFiles: []string{"DirScan:other.hy"},
	}
	setLastContextLog(original)

	// The setter must not retain the input slices (which are owned by a pass).
	original.GoFiles[0] = "caller-mutated.go"
	original.OtherFiles[0] = "caller-mutated.hy"
	original.IgnoredFiles[0] = "caller-mutated.hy"
	original.FoundHyFiles[0] = "caller-mutated.hy"

	first := LastContextLogSnapshot()
	if first.GoFiles[0] != "main.go" || first.OtherFiles[0] != "other.hy" ||
		first.IgnoredFiles[0] != "ignored.hy" || first.FoundHyFiles[0] != "DirScan:other.hy" {
		t.Fatalf("input slices leaked into saved observation: %+v", first)
	}

	// The snapshot must not expose shared backing arrays, even to other readers.
	first.GoFiles[0] = "snapshot-mutated.go"
	first.OtherFiles[0] = "snapshot-mutated.hy"
	first.IgnoredFiles[0] = "snapshot-mutated.hy"
	first.FoundHyFiles[0] = "snapshot-mutated.hy"
	second := LastContextLogSnapshot()
	if second.GoFiles[0] != "main.go" || second.OtherFiles[0] != "other.hy" ||
		second.IgnoredFiles[0] != "ignored.hy" || second.FoundHyFiles[0] != "DirScan:other.hy" {
		t.Fatalf("snapshot mutated shared observation: %+v", second)
	}
}

func TestEngineCheckContextIsCancellableAndDoesNotPoisonRuntime(t *testing.T) {
	engine, err := GetEngine("")
	if err != nil {
		t.Fatal(err)
	}
	source, err := os.ReadFile("../../tests/checker-corpus/02-shared-map-managed.hy")
	if err != nil {
		t.Fatal(err)
	}

	cancelled, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := engine.CheckContext(cancelled, string(source)); !errors.Is(err, context.Canceled) {
		t.Fatalf("cancelled request must fail closed: %v", err)
	}
	if _, err := engine.CheckContext(nil, string(source)); err == nil {
		t.Fatal("nil request context should return an error, not panic")
	}

	// A completed request must still be possible using the same compiled runtime.
	out, err := engine.CheckContext(context.Background(), string(source))
	if err != nil {
		t.Fatalf("cancelled request poisoned shared runtime: %v", err)
	}
	if out.Summary.Verdict != "accept" || out.Summary.Warnings < 1 {
		t.Fatalf("unexpected result after cancelling previous check: %+v", out)
	}
}

func TestDeclaredHybridReadFailuresFailClosed(t *testing.T) {
	unreadable := filepath.Join(t.TempDir(), "unreadable.hy")
	for _, tc := range []struct {
		name     string
		readFile func(string) ([]byte, error)
	}{
		{"driver_read_denied", func(string) ([]byte, error) {
			return nil, errors.New("virtual file unavailable")
		}},
		{"no_driver_reader", nil},
	} {
		t.Run(tc.name, func(t *testing.T) {
			pass := &analysis.Pass{
				Analyzer:   Analyzer,
				Pkg:        types.NewPackage("sample", "sample"),
				OtherFiles: []string{unreadable},
				ReadFile:   tc.readFile,
				Report: func(analysis.Diagnostic) {
					t.Error("no diagnostics should be emitted when a declared input cannot be read")
				},
			}
			_, err := run(pass)
			if err == nil || !strings.Contains(err.Error(), "read declared .hy file") {
				t.Fatalf("unreadable declared input must fail closed, got %v", err)
			}
		})
	}
}

func TestDeclaredAndScannedHybridPathsAreCanonicalAndReportedOnce(t *testing.T) {
	dir := t.TempDir()
	hyPath := filepath.Join(dir, "bad.hy")
	src := "package sample\nfn check() { let x = 7; let y = move x; let z = x }\n"
	if err := os.WriteFile(hyPath, []byte(src), 0600); err != nil {
		t.Fatal(err)
	}
	cwd, err := os.Getwd()
	if err != nil {
		t.Fatal(err)
	}
	relative, err := filepath.Rel(cwd, hyPath)
	if err != nil {
		t.Fatal(err)
	}
	engine, err := GetEngine("")
	if err != nil {
		t.Fatal(err)
	}
	out, err := engine.Check(src)
	if err != nil {
		t.Fatal(err)
	}
	if len(out.Diagnostics) == 0 {
		t.Fatal("regression fixture must emit a diagnostic")
	}

	for _, tc := range []struct {
		name     string
		readFile func(string) ([]byte, error)
	}{
		{"driver_file_reader", os.ReadFile},
		{"filesystem_fallback", nil},
	} {
		t.Run(tc.name, func(t *testing.T) {
			fset := token.NewFileSet()
			goFile, err := parser.ParseFile(fset, filepath.Join(dir, "anchor.go"), "package sample\n", 0)
			if err != nil {
				t.Fatal(err)
			}
			var reported []analysis.Diagnostic
			pass := &analysis.Pass{
				Analyzer:   Analyzer,
				Pkg:        types.NewPackage("sample", "sample"),
				Fset:       fset,
				Files:      []*ast.File{goFile},
				OtherFiles: []string{relative, hyPath},
				ReadFile:   tc.readFile,
				Report:     func(d analysis.Diagnostic) { reported = append(reported, d) },
			}
			if _, err := run(pass); err != nil {
				t.Fatal(err)
			}
			if len(reported) != len(out.Diagnostics) {
				t.Fatalf("expected %d diagnostics for one canonical file, got %d", len(out.Diagnostics), len(reported))
			}
			for _, d := range reported {
				if pos := fset.Position(d.Pos); pos.Filename != hyPath {
					t.Fatalf("diagnostic must use canonical source path: %s", pos)
				}
			}
		})
	}
}

func TestSiblingDiagnosticsHaveStableFileOrder(t *testing.T) {
	dir := t.TempDir()
	src := "package sample\nfn check() { let x = 7; let y = move x; let z = x }\n"
	for _, name := range []string{"b.hy", "a.hy"} {
		if err := os.WriteFile(filepath.Join(dir, name), []byte(src), 0600); err != nil {
			t.Fatal(err)
		}
	}
	for i := 0; i < 4; i++ {
		fset := token.NewFileSet()
		goFile, err := parser.ParseFile(fset, filepath.Join(dir, "anchor.go"), "package sample\n", 0)
		if err != nil {
			t.Fatal(err)
		}
		var filenames []string
		pass := &analysis.Pass{
			Analyzer: Analyzer,
			Pkg:      types.NewPackage("sample", "sample"),
			Fset:     fset,
			Files:    []*ast.File{goFile},
			Report: func(d analysis.Diagnostic) {
				filenames = append(filenames, filepath.Base(fset.Position(d.Pos).Filename))
			},
		}
		if _, err := run(pass); err != nil {
			t.Fatal(err)
		}
		if len(filenames) < 2 {
			t.Fatalf("want both source files to produce diagnostics, got %v", filenames)
		}
		firstB := -1
		for index, name := range filenames {
			if name == "b.hy" {
				firstB = index
				break
			}
		}
		if firstB < 1 {
			t.Fatalf("diagnostics not sorted by canonical source name: %v", filenames)
		}
		for _, name := range filenames[firstB:] {
			if name != "b.hy" {
				t.Fatalf("a.hy emitted after b.hy: %v", filenames)
			}
		}
	}
}

func TestAnalyzerDirectInvocation(t *testing.T) {
	engine, err := GetEngine("")
	if err != nil {
		t.Fatalf("failed to get engine: %v", err)
	}

	badHy := `// EXPECTED-VERDICT: reject HYB-E3003
package samplepkg

fn bad_use_after_move() {
    let x = 42
    let y = move x
    let z = x
}
`
	out, err := engine.Check(badHy)
	if err != nil {
		t.Fatalf("WASM engine check failed: %v", err)
	}

	if out.Summary.Verdict != "reject" {
		t.Fatalf("Expected verdict reject, got %s", out.Summary.Verdict)
	}

	var diags []analysis.Diagnostic
	pass := &analysis.Pass{
		Fset: token.NewFileSet(),
		Report: func(d analysis.Diagnostic) {
			diags = append(diags, d)
		},
	}

	// Map to analysis.Diagnostic
	for _, d := range out.Diagnostics {
		pass.Report(analysis.Diagnostic{
			Pos:      token.NoPos,
			Message:  d.Message,
			Category: d.Code,
		})
	}

	if len(diags) == 0 {
		t.Fatalf("Expected reported diagnostics, got 0")
	}

	t.Logf("Successfully mapped %d diagnostics to analysis.Pass:", len(diags))
	for _, d := range diags {
		t.Logf(" - [%s] %s", d.Category, d.Message)
	}
}

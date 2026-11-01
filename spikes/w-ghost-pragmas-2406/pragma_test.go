package ghostpragmas

import (
	"go/ast"
	"go/parser"
	"go/token"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

func TestCommentPragma_ASTAssociationLimit(t *testing.T) {
	// Snippet using comment pragma on argument
	src := `package main

func process(val int) {}

func main() {
	x := 42
	//hy:move x
	process(/*hy:move*/ x)
}
`
	fset := token.NewFileSet()
	f, err := parser.ParseFile(fset, "test.go", src, parser.ParseComments)
	if err != nil {
		t.Fatalf("ParseFile error: %v", err)
	}

	var callNode *ast.CallExpr
	ast.Inspect(f, func(n ast.Node) bool {
		if c, ok := n.(*ast.CallExpr); ok {
			callNode = c
			return false
		}
		return true
	})

	if callNode == nil {
		t.Fatalf("expected CallExpr, got nil")
	}

	// Go AST does NOT attach CommentGroup to CallExpr or Ident
	// Verify that the call argument expression has no Doc or Comment field in the standard Go AST struct
	arg := callNode.Args[0]
	if _, ok := arg.(*ast.Ident); !ok {
		t.Errorf("expected arg to be *ast.Ident, got %T", arg)
	}

	// The comment is only floating in f.Comments
	foundInFileComments := false
	for _, cg := range f.Comments {
		for _, c := range cg.List {
			if strings.Contains(c.Text, "hy:move") {
				foundInFileComments = true
			}
		}
	}

	if !foundInFileComments {
		t.Errorf("expected comment to exist in File.Comments list")
	}
}

func TestGhostFunctions_FirstClassASTBinding(t *testing.T) {
	src := `package main

type HyPkg struct{}
func (HyPkg) Move(v any) any { return v }
var hy HyPkg

func process(val any) {}

func main() {
	x := 42
	process(hy.Move(x))
}
`
	res, err := ParsePragmasFromSource(src)
	if err != nil {
		t.Fatalf("ParsePragmasFromSource error: %v", err)
	}

	if res.AstCallCount != 1 {
		t.Errorf("expected 1 AstCallCount, got %d", res.AstCallCount)
	}
	if len(res.Directives) != 1 {
		t.Fatalf("expected 1 Directive, got %d", len(res.Directives))
	}
	d := res.Directives[0]
	if d.Kind != LinearMove || d.Target != "x" || !d.IsGhost {
		t.Errorf("unexpected directive: %+v", d)
	}
}

func TestGofmt_Fidelity(t *testing.T) {
	src := `package main

func main() {
	x := 10
	//hy:move x
	_ = x
}
`
	formatted, err := FormatSource(src)
	if err != nil {
		t.Fatalf("FormatSource error: %v", err)
	}
	if !strings.Contains(formatted, "//hy:move x") {
		t.Errorf("comment pragma dropped during formatting: %s", formatted)
	}
}

func TestGhostAPI_ZeroRuntimeOverhead(t *testing.T) {
	goBin := os.Getenv("GO_BIN")
	if goBin == "" {
		goBin = "go"
	}

	src := `package main

// Move identity inline elision test
func Move[T any](x T) T { return x }

func compute(a, b int) int {
	return Move(a) + Move(b)
}

func main() {
	_ = compute(10, 20)
}
`
	dir := t.TempDir()
	mainGo := filepath.Join(dir, "main.go")
	if err := os.WriteFile(mainGo, []byte(src), 0644); err != nil {
		t.Fatalf("WriteFile failed: %v", err)
	}
	goMod := filepath.Join(dir, "go.mod")
	if err := os.WriteFile(goMod, []byte("module ghosttest\n\ngo 1.26\n"), 0644); err != nil {
		t.Fatalf("WriteFile go.mod failed: %v", err)
	}

	cmd := exec.Command(goBin, "build", "-gcflags=-m -S", "-o", os.DevNull, ".")
	cmd.Dir = dir
	out, _ := cmd.CombinedOutput()
	outStr := string(out)

	// Verify that Move is inlined by cmd/compile
	if !strings.Contains(outStr, "can inline Move") && !strings.Contains(outStr, "inlining call to Move") {
		t.Logf("Compiler optimization output:\n%s", outStr)
		t.Errorf("expected Move[T] to be inlined by cmd/compile")
	}
}

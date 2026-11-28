package ghostpragmas

import (
	"bytes"
	"fmt"
	"go/ast"
	"go/parser"
	"go/printer"
	"go/token"
	"strings"
)

// AffineKind denotes the ownership capability token.
type AffineKind int

const (
	LinearMove AffineKind = iota
	BorrowView
	ManagedBlock
	TaskSpawn
)

// Directive represents an extracted capability annotation.
type Directive struct {
	Kind     AffineKind
	Target   string
	Line     int
	Col      int
	RawText  string
	IsGhost  bool // true if ghost function call, false if comment pragma
}

// ExtractionResult records the extracted capabilities and any dissociation warnings.
type ExtractionResult struct {
	Directives    []Directive
	Dissociated   []string
	AstCallCount  int
	CommentCount  int
}

// ParsePragmasFromSource analyzes source code using go/parser and checks whether
// comment pragmas vs ghost function calls can be deterministically bound to AST nodes.
func ParsePragmasFromSource(src string) (*ExtractionResult, error) {
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, "sample.go", src, parser.ParseComments)
	if err != nil {
		return nil, fmt.Errorf("go/parser failed: %w", err)
	}

	res := &ExtractionResult{}

	// 1. Scan Comments for //hy: or /*hy:
	for _, cg := range file.Comments {
		for _, c := range cg.List {
			text := strings.TrimSpace(c.Text)
			if strings.HasPrefix(text, "//hy:") || strings.HasPrefix(text, "/*hy:") {
				res.CommentCount++
				pos := fset.Position(c.Pos())
				directive := Directive{
					Line:    pos.Line,
					Col:     pos.Column,
					RawText: text,
					IsGhost: false,
				}
				if strings.Contains(text, "move") {
					directive.Kind = LinearMove
				} else if strings.Contains(text, "task") {
					directive.Kind = TaskSpawn
				} else if strings.Contains(text, "managed") {
					directive.Kind = ManagedBlock
				}
				res.Directives = append(res.Directives, directive)
			}
		}
	}

	// 2. Scan AST for Ghost Calls: hy.Move(...), hy.Task(...)
	ast.Inspect(file, func(n ast.Node) bool {
		if call, ok := n.(*ast.CallExpr); ok {
			if sel, ok := call.Fun.(*ast.SelectorExpr); ok {
				if id, ok := sel.X.(*ast.Ident); ok && id.Name == "hy" {
					res.AstCallCount++
					pos := fset.Position(call.Pos())
					targetName := ""
					if len(call.Args) > 0 {
						var buf bytes.Buffer
						_ = printer.Fprint(&buf, fset, call.Args[0])
						targetName = buf.String()
					}
					var k AffineKind
					switch sel.Sel.Name {
					case "Move":
						k = LinearMove
					case "Task":
						k = TaskSpawn
					case "Managed":
						k = ManagedBlock
					default:
						k = BorrowView
					}
					res.Directives = append(res.Directives, Directive{
						Kind:     k,
						Target:   targetName,
						Line:     pos.Line,
						Col:      pos.Column,
						RawText:  fmt.Sprintf("hy.%s(%s)", sel.Sel.Name, targetName),
						IsGhost:  true,
					})
				}
			}
		}
		return true
	})

	return res, nil
}

// FormatSource formats source using go/printer to test comment stability across gofmt.
func FormatSource(src string) (string, error) {
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, "src.go", src, parser.ParseComments)
	if err != nil {
		return "", err
	}
	var buf bytes.Buffer
	cfg := printer.Config{Mode: printer.UseSpaces | printer.TabIndent, Tabwidth: 8}
	if err := cfg.Fprint(&buf, fset, file); err != nil {
		return "", err
	}
	return buf.String(), nil
}

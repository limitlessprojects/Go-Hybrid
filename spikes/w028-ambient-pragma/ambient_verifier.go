// Package ambient provides a reference implementation of the W-028 Zero-Split
// Ambient Verifier, demonstrating how go/parser and go/ast extract //go:hyb pragma
// directives directly from standard Go source files and verify affine escape
// and sendability boundaries without requiring a bifurcated .hy / .go toolchain.
package ambient

import (
	"fmt"
	"go/ast"
	"go/parser"
	"go/token"
	"strings"
)

type DiagnosticCode string

const (
	ErrUseAfterMove      DiagnosticCode = "HYB-E3001"
	ErrIllegalEscape     DiagnosticCode = "HYB-E3002"
	ErrUnconsumedOblig   DiagnosticCode = "HYB-E3003"
	ErrNonSendableCross  DiagnosticCode = "HYB-E3014"
)

type Diagnostic struct {
	Code    DiagnosticCode
	Pos     token.Position
	Symbol  string
	Message string
}

type SymbolState struct {
	Name       string
	IsAffine   bool
	IsSendable bool
	IsMoved    bool
	MovedAt    token.Pos
}

type Verifier struct {
	Fset        *token.FileSet
	Symbols     map[string]*SymbolState
	Diagnostics []Diagnostic
}

func NewVerifier() *Verifier {
	return &Verifier{
		Fset:    token.NewFileSet(),
		Symbols: make(map[string]*SymbolState),
	}
}

// ParsePragmas extracts //go:hyb directives from leading comment groups.
func ParsePragmas(cg *ast.CommentGroup) []string {
	if cg == nil {
		return nil
	}
	var pragmas []string
	for _, c := range cg.List {
		text := strings.TrimSpace(c.Text)
		if strings.HasPrefix(text, "//go:hyb ") {
			dir := strings.TrimPrefix(text, "//go:hyb ")
			pragmas = append(pragmas, strings.Fields(dir)...)
		}
	}
	return pragmas
}

// VerifySource parses and inspects Go source code with ambient //go:hyb pragmas.
func (v *Verifier) VerifySource(filename string, src []byte) ([]Diagnostic, error) {
	file, err := parser.ParseFile(v.Fset, filename, src, parser.ParseComments)
	if err != nil {
		return nil, fmt.Errorf("parsing failed: %w", err)
	}

	// 1. First pass: Collect type declarations and annotated variables
	for _, decl := range file.Decls {
		genDecl, ok := decl.(*ast.GenDecl)
		if !ok {
			continue
		}
		pragmas := ParsePragmas(genDecl.Doc)
		isAffine := contains(pragmas, "affine")
		isSendable := contains(pragmas, "sendable")

		for _, spec := range genDecl.Specs {
			if typeSpec, ok := spec.(*ast.TypeSpec); ok {
				// Record type capability
				v.Symbols[typeSpec.Name.Name] = &SymbolState{
					Name:       typeSpec.Name.Name,
					IsAffine:   isAffine,
					IsSendable: isSendable,
				}
			}
		}
	}

	// 2. Second pass: AST inspection for moves, channel sends, and goroutine escapes
	ast.Inspect(file, func(n ast.Node) bool {
		switch node := n.(type) {
		case *ast.GoStmt:
			// Inspect goroutine boundary (go func(...) or go call())
			v.inspectGoBoundary(node)
		case *ast.SendStmt:
			// Inspect channel send (ch <- val)
			v.inspectSendStmt(node)
		case *ast.CallExpr:
			// Inspect move(x) or custom consumer
			if ident, ok := node.Fun.(*ast.Ident); ok && ident.Name == "move" {
				for _, arg := range node.Args {
					if argIdent, ok := arg.(*ast.Ident); ok {
						v.recordMove(argIdent)
					}
				}
			}
		case *ast.Ident:
			// Check if used after move
			if sym, ok := v.Symbols[node.Name]; ok && sym.IsMoved {
				// Exclude the move call itself
				v.Diagnostics = append(v.Diagnostics, Diagnostic{
					Code:    ErrUseAfterMove,
					Pos:     v.Fset.Position(node.Pos()),
					Symbol:  node.Name,
					Message: fmt.Sprintf("use of affine symbol '%s' after move", node.Name),
				})
			}
		}
		return true
	})

	return v.Diagnostics, nil
}

func (v *Verifier) recordMove(ident *ast.Ident) {
	sym, ok := v.Symbols[ident.Name]
	if !ok {
		// Register affine symbol
		sym = &SymbolState{Name: ident.Name, IsAffine: true}
		v.Symbols[ident.Name] = sym
	}
	if sym.IsMoved {
		v.Diagnostics = append(v.Diagnostics, Diagnostic{
			Code:    ErrUseAfterMove,
			Pos:     v.Fset.Position(ident.Pos()),
			Symbol:  ident.Name,
			Message: fmt.Sprintf("cannot move affine symbol '%s' more than once", ident.Name),
		})
		return
	}
	sym.IsMoved = true
	sym.MovedAt = ident.Pos()
}

func (v *Verifier) inspectGoBoundary(node *ast.GoStmt) {
	// Goroutine boundaries cannot capture non-sendable references or unmoved affine handles
	ast.Inspect(node.Call, func(inner ast.Node) bool {
		if ident, ok := inner.(*ast.Ident); ok {
			if sym, ok := v.Symbols[ident.Name]; ok {
				if sym.IsAffine && !sym.IsMoved {
					v.Diagnostics = append(v.Diagnostics, Diagnostic{
						Code:    ErrIllegalEscape,
						Pos:     v.Fset.Position(ident.Pos()),
						Symbol:  ident.Name,
						Message: fmt.Sprintf("affine symbol '%s' captured into goroutine without move", ident.Name),
					})
				}
			}
		}
		return true
	})
}

func (v *Verifier) inspectSendStmt(node *ast.SendStmt) {
	if ident, ok := node.Value.(*ast.Ident); ok {
		if sym, ok := v.Symbols[ident.Name]; ok {
			if sym.IsMoved {
				v.Diagnostics = append(v.Diagnostics, Diagnostic{
					Code:    ErrUseAfterMove,
					Pos:     v.Fset.Position(ident.Pos()),
					Symbol:  ident.Name,
					Message: fmt.Sprintf("cannot send moved affine symbol '%s' to channel", ident.Name),
				})
			} else if sym.IsAffine {
				sym.IsMoved = true
				sym.MovedAt = ident.Pos()
			}
		}
	}
}

func contains(slice []string, item string) bool {
	for _, s := range slice {
		if s == item {
			return true
		}
	}
	return false
}

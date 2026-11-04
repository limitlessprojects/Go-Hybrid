package decider

import (
	"go/ast"
	"go/parser"
	"go/token"
	"go/types"
	"strings"
)

type Diagnostic struct {
	Code     string `json:"code"`
	Severity string `json:"severity"`
	File     string `json:"file"`
	Line     int    `json:"line"`
	Col      int    `json:"col"`
	Message  string `json:"message"`
	Rule     string `json:"rule"`
}

type Decider struct {
	fset     *token.FileSet
	importer *CachedImporter
}

func NewDecider(importer *CachedImporter) *Decider {
	return &Decider{
		fset:     token.NewFileSet(),
		importer: importer,
	}
}

// ClassifyGoError maps a go/types error message to the matching Hybrid diagnostic code.
func ClassifyGoError(msg string) (code string, rule string) {
	lower := strings.ToLower(msg)

	switch {
	case strings.Contains(lower, "cannot compare") ||
		strings.Contains(lower, "operator ==") ||
		strings.Contains(lower, "can only be compared to nil"):
		return "HYB-E2006", "BUILDABLE"

	case strings.Contains(lower, "negative shift count"):
		return "HYB-E6005", "BUILDABLE"

	case strings.Contains(lower, "invalid array length") ||
		strings.Contains(lower, "array length must be non-negative"):
		return "HYB-E6004", "BUILDABLE"

	case strings.Contains(lower, "redeclared in this block"):
		return "HYB-E1003", "BUILDABLE"

	default:
		// Default to HYB-E1002 (unbuildable declaration: arity, return mismatch, unused var, invalid types)
		return "HYB-E1002", "BUILDABLE"
	}
}

// CheckSource parses and type-checks emitted Go source code containing //line directives,
// back-projecting any errors to the original .hy file coordinates.
func (d *Decider) CheckSource(filename string, src []byte) ([]Diagnostic, error) {
	fileFset := token.NewFileSet()
	fileNode, err := parser.ParseFile(fileFset, filename, src, parser.AllErrors)
	if err != nil {
		pos := fileFset.Position(fileNode.Pos())
		code, rule := ClassifyGoError(err.Error())
		return []Diagnostic{{
			Code:     code,
			Severity: "error",
			File:     pos.Filename,
			Line:     pos.Line,
			Col:      pos.Column,
			Message:  err.Error(),
			Rule:     rule,
		}}, nil
	}

	var diags []Diagnostic
	conf := types.Config{
		Importer: d.importer,
		Error: func(typeErr error) {
			tErr, ok := typeErr.(types.Error)
			if !ok {
				diags = append(diags, Diagnostic{
					Code:     "HYB-E1002",
					Severity: "error",
					File:     filename,
					Line:     1,
					Col:      1,
					Message:  typeErr.Error(),
					Rule:     "BUILDABLE",
				})
				return
			}

			pos := fileFset.Position(tErr.Pos)
			code, rule := ClassifyGoError(tErr.Msg)
			diags = append(diags, Diagnostic{
				Code:     code,
				Severity: "error",
				File:     pos.Filename,
				Line:     pos.Line,
				Col:      pos.Column,
				Message:  tErr.Msg,
				Rule:     rule,
			})
		},
	}

	info := &types.Info{}
	pkgName := fileNode.Name.Name
	_, _ = conf.Check(pkgName, fileFset, []*ast.File{fileNode}, info)
	return diags, nil
}

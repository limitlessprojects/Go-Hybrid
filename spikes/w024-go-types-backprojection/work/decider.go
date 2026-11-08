package decider

import (
	"go/ast"
	"go/parser"
	"go/scanner"
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
	importer *CachedImporter
}

func NewDecider(importer *CachedImporter) *Decider {
	return &Decider{importer: importer}
}

// ClassifyGoError deterministically maps go/types & go/parser errors to Hybrid diagnostic codes.
func ClassifyGoError(msg string) (code string, rule string) {
	lower := strings.ToLower(msg)
	switch {
	case strings.Contains(lower, "cannot compare") ||
		strings.Contains(lower, "operator ==") ||
		strings.Contains(lower, "operator !=") ||
		strings.Contains(lower, "can only be compared to nil") ||
		strings.Contains(lower, "incomparable map key type") ||
		strings.Contains(lower, "invalid map key type"):
		return "HYB-E2006", "BUILDABLE"

	case strings.Contains(lower, "negative shift count") ||
		strings.Contains(lower, "shift count type") ||
		strings.Contains(lower, "must be integer"):
		return "HYB-E6005", "BUILDABLE"

	case strings.Contains(lower, "invalid array length") ||
		strings.Contains(lower, "array bound must be non-negative") ||
		strings.Contains(lower, "array length must be non-negative"):
		return "HYB-E6004", "BUILDABLE"

	case strings.Contains(lower, "redeclared in this block") ||
		strings.Contains(lower, "no new variables on left side of :="):
		return "HYB-E1003", "BUILDABLE"

	default:
		return "HYB-E1002", "BUILDABLE"
	}
}

// CheckSource parses and type-checks span-injected Go code in memory and back-projects
// all syntax and type errors to their exact .hy coordinates.
func (d *Decider) CheckSource(filename string, src []byte) ([]Diagnostic, error) {
	fset := token.NewFileSet()
	fileNode, parseErr := parser.ParseFile(fset, filename, src, parser.AllErrors)
	if parseErr != nil {
		var diags []Diagnostic
		if errList, ok := parseErr.(scanner.ErrorList); ok && len(errList) > 0 {
			for _, e := range errList {
				code, rule := ClassifyGoError(e.Msg)
				diags = append(diags, Diagnostic{
					Code:     code,
					Severity: "error",
					File:     e.Pos.Filename,
					Line:     e.Pos.Line,
					Col:      e.Pos.Column,
					Message:  e.Msg,
					Rule:     rule,
				})
			}
			return diags, nil
		}
		code, rule := ClassifyGoError(parseErr.Error())
		return []Diagnostic{{
			Code:     code,
			Severity: "error",
			File:     filename,
			Line:     1,
			Col:      1,
			Message:  parseErr.Error(),
			Rule:     rule,
		}}, nil
	}

	var diags []Diagnostic
	conf := types.Config{
		Importer: d.importer,
		Error: func(err error) {
			if tErr, ok := err.(types.Error); ok {
				pos := fset.Position(tErr.Pos)
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
			} else {
				diags = append(diags, Diagnostic{
					Code:     "HYB-E1002",
					Severity: "error",
					File:     filename,
					Line:     1,
					Col:      1,
					Message:  err.Error(),
					Rule:     "BUILDABLE",
				})
			}
		},
	}

	info := &types.Info{}
	_, _ = conf.Check(fileNode.Name.Name, fset, []*ast.File{fileNode}, info)
	return diags, nil
}

// Package oracle implements the executable Stage-1/Stage-2 go/types fact
// extractor and span-mapping verifier for Wildcard Proposal #3941
// (Pipeline Inversion: parse -> in-memory emit -> go/types oracle -> Tier-0 hy-check).
package oracle

import (
	"go/ast"
	"go/importer"
	"go/parser"
	"go/token"
	"go/types"
	"sync"
)

// HySpan identifies an original .hy source location attached to an emitted Go line.
type HySpan struct {
	File     string
	Line     int
	Col      int
	NodeID   int
	HintCode string
}

// TypeFact is the exact ground-truth structural classification extracted from
// go/types.Type after generic instantiation and alias expansion.
type TypeFact struct {
	GoType       string
	MayShare     bool
	HoldsClosure bool
	IsSendable   bool
}

// MappedDiag represents a go/types error projected back onto a .hy span.
type MappedDiag struct {
	Code    string
	HyFile  string
	HyLine  int
	HyCol   int
	Message string
}

// CachedImporter memoizes stdlib export packages across warm invocations
// (Falsifier P4 resident worker simulation).
type CachedImporter struct {
	mu   sync.Mutex
	base types.Importer
	pkgs map[string]*types.Package
}

func NewCachedImporter() *CachedImporter {
	return &CachedImporter{
		base: importer.Default(),
		pkgs: make(map[string]*types.Package),
	}
}

func (c *CachedImporter) Import(path string) (*types.Package, error) {
	c.mu.Lock()
	if pkg, ok := c.pkgs[path]; ok {
		c.mu.Unlock()
		return pkg, nil
	}
	c.mu.Unlock()

	pkg, err := c.base.Import(path)
	if err != nil {
		return nil, err
	}
	c.mu.Lock()
	c.pkgs[path] = pkg
	c.mu.Unlock()
	return pkg, nil
}

// ClassifyType computes exact MayShare and HoldsClosure facts over an
// arbitrarily nested or generic-instantiated types.Type with cycle termination.
func ClassifyType(t types.Type) TypeFact {
	mayShare := checkMayShare(t, make(map[types.Type]bool))
	holdsClosure := checkHoldsClosure(t, make(map[types.Type]bool))
	goTy := "<nil>"
	if t != nil {
		goTy = t.String()
	}
	return TypeFact{
		GoType:       goTy,
		MayShare:     mayShare,
		HoldsClosure: holdsClosure,
		IsSendable:   !mayShare && !holdsClosure,
	}
}

func checkMayShare(t types.Type, seen map[types.Type]bool) bool {
	if t == nil || seen[t] {
		return false
	}
	seen[t] = true
	switch u := t.Underlying().(type) {
	case *types.Pointer, *types.Slice, *types.Map, *types.Chan, *types.Interface:
		return true
	case *types.Array:
		return checkMayShare(u.Elem(), seen)
	case *types.Struct:
		for i := 0; i < u.NumFields(); i++ {
			if checkMayShare(u.Field(i).Type(), seen) {
				return true
			}
		}
	}
	return false
}

func checkHoldsClosure(t types.Type, seen map[types.Type]bool) bool {
	if t == nil || seen[t] {
		return false
	}
	seen[t] = true
	switch u := t.Underlying().(type) {
	case *types.Signature:
		return true
	case *types.Pointer:
		return checkHoldsClosure(u.Elem(), seen)
	case *types.Slice:
		return checkHoldsClosure(u.Elem(), seen)
	case *types.Array:
		return checkHoldsClosure(u.Elem(), seen)
	case *types.Map:
		return checkHoldsClosure(u.Key(), seen) || checkHoldsClosure(u.Elem(), seen)
	case *types.Chan:
		return checkHoldsClosure(u.Elem(), seen)
	case *types.Struct:
		for i := 0; i < u.NumFields(); i++ {
			if checkHoldsClosure(u.Field(i).Type(), seen) {
				return true
			}
		}
	}
	return false
}

// CheckResult bundles extracted variable TypeFacts and reverse-mapped diagnostics.
type CheckResult struct {
	VarFacts map[string]TypeFact
	Diags    []MappedDiag
}

// CheckBuffer parses an in-memory Go buffer, runs go/types with imp, extracts
// TypeFacts for all local/package value definitions, and maps type errors via lineMap.
func CheckBuffer(goSrc string, lineMap map[int]HySpan, imp types.Importer) (*CheckResult, error) {
	fset := token.NewFileSet()
	file, parseErr := parser.ParseFile(fset, "inmem.hy.go", goSrc, parser.AllErrors)
	if file == nil {
		return nil, parseErr
	}
	info := &types.Info{
		Types: make(map[ast.Expr]types.TypeAndValue),
		Defs:  make(map[*ast.Ident]types.Object),
		Uses:  make(map[*ast.Ident]types.Object),
	}
	var diags []MappedDiag
	conf := types.Config{
		Importer: imp,
		Error: func(err error) {
			te, ok := err.(types.Error)
			if !ok {
				return
			}
			pos := fset.Position(te.Pos)
			span, hasSpan := lineMap[pos.Line]
			if !hasSpan {
				span = HySpan{File: "inmem.hy", Line: pos.Line, Col: pos.Column, HintCode: "HYB-E1002"}
			}
			code := span.HintCode
			if code == "" {
				code = "HYB-E1002"
			}
			diags = append(diags, MappedDiag{
				Code:    code,
				HyFile:  span.File,
				HyLine:  span.Line,
				HyCol:   span.Col,
				Message: te.Msg,
			})
		},
	}
	_, _ = conf.Check("main", fset, []*ast.File{file}, info)

	varFacts := make(map[string]TypeFact)
	for ident, obj := range info.Defs {
		if v, ok := obj.(*types.Var); ok && !v.IsField() {
			varFacts[ident.Name] = ClassifyType(v.Type())
		}
	}
	return &CheckResult{
		VarFacts: varFacts,
		Diags:    diags,
	}, nil
}

// SignatureParamMutates reports whether a pure go/types.Signature mutability rule
// conservatively treats paramTy as mutable reference storage (Falsifier P3 probe).
func SignatureParamMutates(paramTy types.Type) bool {
	if paramTy == nil {
		return false
	}
	switch paramTy.Underlying().(type) {
	case *types.Pointer, *types.Slice, *types.Map, *types.Interface:
		return true
	}
	return false
}

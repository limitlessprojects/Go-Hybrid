package jsonbinding

import (
	"go/ast"
	"go/parser"
	"go/token"
	"testing"

	"hybrid.program/hybridge"
)

func TestAbiWordSmallClassOnShallowStack(t *testing.T) {
	var ranInWindow bool
	var walk func(int) byte
	walk = func(depth int) byte {
		var pad [128]byte
		pad[0] = byte(depth)
		h := hybridge.SPHeadroomForTest()
		if h > hybridge.SmallGuard+256 && h < 3900 {
			before := hybridge.DeepPathCount.Load()
			w := hybridge.Cross1S(FnAbiWord, 0)
			afterSmall := hybridge.DeepPathCount.Load()
			if w != abiWordExpected {
				t.Fatalf("Cross1S(FnAbiWord) = %#x, want %#x", w, uint64(abiWordExpected))
			}
			if afterSmall != before {
				t.Fatalf("Cross1S(FnAbiWord) took deep fallback at headroom %d B", h)
			}
			wDefault := hybridge.Cross1(FnAbiWord, 0)
			afterDefault := hybridge.DeepPathCount.Load()
			if wDefault != abiWordExpected {
				t.Fatalf("Cross1(FnAbiWord) = %#x, want %#x", wDefault, uint64(abiWordExpected))
			}
			if afterDefault <= afterSmall {
				t.Fatalf("expected Cross1(FnAbiWord) to require deep fallback at headroom %d B", h)
			}
			ranInWindow = true
			return pad[0]
		}
		if depth > 0 {
			pad[1] = walk(depth - 1)
		}
		return pad[0] ^ pad[1]
	}
	_ = walk(64)
	if !ranInWindow {
		t.Fatal("did not reach (SmallGuard, 4096) shallow-stack disagreement window")
	}
}

func TestBrokerInitGuardClassContract(t *testing.T) {
	fset := token.NewFileSet()
	f, err := parser.ParseFile(fset, "broker.go", nil, 0)
	if err != nil {
		t.Fatalf("ParseFile(broker.go): %v", err)
	}
	var foundCross1S bool
	ast.Inspect(f, func(n ast.Node) bool {
		call, ok := n.(*ast.CallExpr)
		if !ok {
			return true
		}
		sel, ok := call.Fun.(*ast.SelectorExpr)
		if !ok {
			return true
		}
		pkg, ok := sel.X.(*ast.Ident)
		if ok && pkg.Name == "hybridge" && sel.Sel.Name == "Cross1S" {
			foundCross1S = true
		}
		if ok && pkg.Name == "hybridge" && sel.Sel.Name == "Cross1" {
			t.Fatal("broker.go must use hybridge.Cross1S (SMALL guard class) for FnAbiWord")
		}
		return true
	})
	if !foundCross1S {
		t.Fatal("broker.go init() did not call hybridge.Cross1S")
	}
}

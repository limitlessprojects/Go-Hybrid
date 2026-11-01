package regexbinding

import (
	"errors"
	"go/ast"
	"go/parser"
	"go/token"
	"runtime"
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

func TestPinStringStackPinnerAndZeroAllocEmpty(t *testing.T) {
	allocs := testing.AllocsPerRun(100, func() {
		var p runtime.Pinner
		ptr, n := pinString("", &p)
		p.Unpin()
		if ptr != 0 || n != 0 {
			t.Fatalf("pinString empty = (%#x, %d), want (0, 0)", ptr, n)
		}
	})
	if allocs != 0 {
		t.Fatalf("pinString empty allocs = %v, want 0", allocs)
	}
}

func TestDecodeMatchStatusStrictContract(t *testing.T) {
	if got, err := decodeMatchStatus(0); err != nil || got {
		t.Fatalf("decodeMatchStatus(0) = (%v, %v), want (false, nil)", got, err)
	}
	if got, err := decodeMatchStatus(1); err != nil || !got {
		t.Fatalf("decodeMatchStatus(1) = (%v, %v), want (true, nil)", got, err)
	}
	for _, invalid := range []int64{-1, -2, 2, 3, int64(uint32(0xffffffff))} {
		if got, err := decodeMatchStatus(invalid); !errors.Is(err, ErrRegex) || got {
			t.Fatalf("decodeMatchStatus(%d) = (%v, %v), want (false, ErrRegex)", invalid, got, err)
		}
	}
}

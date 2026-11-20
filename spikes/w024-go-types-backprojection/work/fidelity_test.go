package decider

import (
	"strings"
	"testing"
)

func TestFalsifierF1_SpanAttributionFidelity(t *testing.T) {
	importer := NewCachedImporter()
	if err := importer.WarmStdlib("fmt"); err != nil {
		t.Fatalf("warm stdlib: %v", err)
	}
	d := NewDecider(importer)

	tests := []struct {
		name         string
		emittedSrc   string
		expectedCode string
		expectedFile string
		expectedLine int
		expectedCol  int
	}{
		{
			// In Go line directives, `//line file:line:1` sets byte 0 of the next line to (line, col=1).
			// Byte offsets on `\t_ = add(1)`: col 1='\t', ..., col 9='(', col 10='1', col 11=')'.
			// go/types places "not enough arguments" at the closing ')' (col 11).
			name: "call-arity-mismatch",
			emittedSrc: `package main
//line add.hy:1:1
func add(a int, b int) int {
//line add.hy:2:1
	return a + b
}
//line add.hy:5:1
func main() {
//line add.hy:6:1
	_ = add(1)
}
`,
			expectedCode: "HYB-E1002",
			expectedFile: "add.hy",
			expectedLine: 6,
			expectedCol:  11,
		},
		{
			// On `\treturn "not-an-int"`, string literal begins at col 9 (`//line ret.hy:2:1`).
			name: "return-type-mismatch",
			emittedSrc: `package main
//line ret.hy:1:1
func bad() int {
//line ret.hy:2:1
	return "not-an-int"
}
`,
			expectedCode: "HYB-E1002",
			expectedFile: "ret.hy",
			expectedLine: 2,
			expectedCol:  9,
		},
		{
			// On `\treturn 1`, go/types anchors `not enough return values` at the return expression `1` (col 9).
			name: "return-value-count",
			emittedSrc: `package main
//line ret2.hy:1:1
func pair() (int, int) {
//line ret2.hy:4:1
	return 1
}
`,
			expectedCode: "HYB-E1002",
			expectedFile: "ret2.hy",
			expectedLine: 4,
			expectedCol:  9,
		},
		{
			// Unused variable `x` starts at col 2 on `\tx := 5`.
			name: "unused-binding",
			emittedSrc: `package main
//line bind.hy:1:1
func unused() {
//line bind.hy:3:1
	x := 5
}
`,
			expectedCode: "HYB-E1002",
			expectedFile: "bind.hy",
			expectedLine: 3,
			expectedCol:  2,
		},
		{
			// `a == b` is a BinaryExpr starting at `a` (col 9 on `\treturn a == b`).
			name: "incomparable-equality-slice",
			emittedSrc: `package main
//line cmp.hy:1:1
func testCompare(a []int, b []int) bool {
//line cmp.hy:2:1
	return a == b
}
`,
			expectedCode: "HYB-E2006",
			expectedFile: "cmp.hy",
			expectedLine: 2,
			expectedCol:  9,
		},
		{
			// Negative shift `1 << -1` anchors at `-1` (col 14 on `\treturn 1 << -1`).
			name: "negative-shift",
			emittedSrc: `package main
//line shift.hy:1:1
func testShift() int {
//line shift.hy:2:1
	return 1 << -1
}
`,
			expectedCode: "HYB-E6005",
			expectedFile: "shift.hy",
			expectedLine: 2,
			expectedCol:  14,
		},
		{
			// `var a [-1]int` anchors at `-1` (col 9 on `\tvar a [-1]int`).
			name: "invalid-array-len",
			emittedSrc: `package main
//line arr.hy:1:1
func testArray() {
//line arr.hy:2:1
	var a [-1]int
	_ = a
}
`,
			expectedCode: "HYB-E6004",
			expectedFile: "arr.hy",
			expectedLine: 2,
			expectedCol:  9,
		},
		{
			// Redeclared identifier `x` starts at col 6 on `\tvar x string`.
			name: "redeclaration-block",
			emittedSrc: `package main
//line redecl.hy:1:1
func testRedecl() {
	var x int
//line redecl.hy:3:1
	var x string
	_, _ = x, x
}
`,
			expectedCode: "HYB-E1003",
			expectedFile: "redecl.hy",
			expectedLine: 3,
			expectedCol:  6,
		},
		{
			// Token-granular inline `/*line file:line:col*/` prevents coordinate bleed from
			// synthetic wrapper code emitted around a .hy expression on the same Go line.
			name: "synthetic-wrapper-inline-isolation",
			emittedSrc: `package main
func tryWrapper() int {
	__hy_tmp := func() int { /*line wrap.hy:14:9*/return "bad" }(); return __hy_tmp
}
`,
			expectedCode: "HYB-E1002",
			expectedFile: "wrap.hy",
			expectedLine: 14,
			expectedCol:  16,
		},
		{
			// Syntax/scanner errors must map cleanly via scanner.ErrorList without panicking
			// even when parser.ParseFile returns a partial or nil *ast.File.
			name: "syntax-error-scanner-backprojection",
			emittedSrc: `package main
func broken() {
//line syn.hy:8:1
	var x = ;
}
`,
			expectedCode: "HYB-E1002",
			expectedFile: "syn.hy",
			expectedLine: 8,
			expectedCol:  10,
		},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			diags, err := d.CheckSource("emitted.go", []byte(tc.emittedSrc))
			if err != nil {
				t.Fatalf("unexpected checker runtime error: %v", err)
			}
			if len(diags) == 0 {
				t.Fatalf("expected diagnostic, got 0")
			}

			got := diags[0]
			if got.Code != tc.expectedCode {
				t.Errorf("code mismatch: got %s, want %s (msg: %s)", got.Code, tc.expectedCode, got.Message)
			}
			if got.File != tc.expectedFile {
				t.Errorf("file mismatch: got %s, want %s", got.File, tc.expectedFile)
			}
			if got.Line != tc.expectedLine {
				t.Errorf("line mismatch: got %d, want %d", got.Line, tc.expectedLine)
			}
			if got.Col != tc.expectedCol {
				t.Errorf("col mismatch: got %d, want %d (msg: %s)", got.Col, tc.expectedCol, got.Message)
			}
		})
	}
}

func TestParseNilFileSafety(t *testing.T) {
	d := NewDecider(NewCachedImporter())
	// Missing package clause causes parser.ParseFile to return nil *ast.File + scanner.ErrorList.
	diags, err := d.CheckSource("broken.go", []byte("func bad() {}\n"))
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(diags) == 0 {
		t.Fatalf("expected diagnostic for missing package clause, got 0")
	}
	if diags[0].File != "broken.go" || !strings.Contains(diags[0].Message, "expected 'package'") {
		t.Fatalf("unexpected diagnostic: %+v", diags[0])
	}
}

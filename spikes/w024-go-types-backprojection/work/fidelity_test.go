package decider

import (
	"testing"
)

func TestFalsifierF1_SpanAttributionFidelity(t *testing.T) {
	importer := NewCachedImporter()
	_ = importer.WarmStdlib("fmt")
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
			name: "call-arity-mismatch",
			emittedSrc: `package main
//line add.hy:1:1
func add(a int, b int) int {
//line add.hy:2:5
	return a + b
}
//line add.hy:5:1
func main() {
//line add.hy:6:10
	_ = add(1)
}
`,
			expectedCode: "HYB-E1002",
			expectedFile: "add.hy",
			expectedLine: 6,
			expectedCol:  10,
		},
		{
			name: "return-type-mismatch",
			emittedSrc: `package main
//line ret.hy:1:1
func bad() int {
//line ret.hy:2:12
	return "not-an-int"
}
`,
			expectedCode: "HYB-E1002",
			expectedFile: "ret.hy",
			expectedLine: 2,
			expectedCol:  12,
		},
		{
			name: "incomparable-equality-slice",
			emittedSrc: `package main
//line cmp.hy:1:1
func testCompare(a []int, b []int) bool {
//line cmp.hy:2:9
	return a == b
}
`,
			expectedCode: "HYB-E2006",
			expectedFile: "cmp.hy",
			expectedLine: 2,
			expectedCol:  9,
		},
		{
			name: "negative-shift",
			emittedSrc: `package main
//line shift.hy:1:1
func testShift() int {
//line shift.hy:2:9
	return 1 << -1
}
`,
			expectedCode: "HYB-E6005",
			expectedFile: "shift.hy",
			expectedLine: 2,
			expectedCol:  9,
		},
		{
			name: "invalid-array-len",
			emittedSrc: `package main
//line arr.hy:1:1
func testArray() {
//line arr.hy:2:8
	var a [-1]int
	_ = a
}
`,
			expectedCode: "HYB-E6004",
			expectedFile: "arr.hy",
			expectedLine: 2,
			expectedCol:  8,
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
				t.Errorf("col mismatch: got %d, want %d", got.Col, tc.expectedCol)
			}
		})
	}
}

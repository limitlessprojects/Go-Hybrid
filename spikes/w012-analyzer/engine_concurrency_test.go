package analyzer

import (
	"context"
	"fmt"
	"go/token"
	"os"
	"sync"
	"testing"
)

// A single Wazero Runtime can serve concurrent checks only if each disposable
// guest has an anonymous module name. A named module fails on overlapping
// instantiation even though sequential checks and the race detector may pass.
func TestConcurrentDisposableGuests(t *testing.T) {
	wasmBytes, err := os.ReadFile(defaultWasmPath)
	if err != nil {
		t.Fatalf("fresh WASM artifact required: %v", err)
	}
	engine, err := NewWasmEngine(context.Background(), wasmBytes)
	if err != nil {
		t.Fatal(err)
	}
	defer engine.Close()
	acceptBytes, err := os.ReadFile("../../tests/checker-corpus/09-a3-map-slot-valid.hy")
	if err != nil {
		t.Fatal(err)
	}
	rejectBytes, err := os.ReadFile("../../tests/checker-corpus/08-a3-map-slot-invalidated.hy")
	if err != nil {
		t.Fatal(err)
	}
	const workers, iterations = 12, 40
	start := make(chan struct{})
	failures := make(chan error, workers)
	var wg sync.WaitGroup
	for worker := 0; worker < workers; worker++ {
		wg.Add(1)
		go func(worker int) {
			defer wg.Done()
			<-start
			for i := 0; i < iterations; i++ {
				for _, tc := range []struct {
					source string
					want   AnalysisVerdict
				}{
					{string(acceptBytes), VerdictAccept},
					{string(rejectBytes), VerdictReject},
				} {
					res := engine.CheckSource(context.Background(), tc.source)
					if err := res.CheckInvariant(); err != nil {
						failures <- fmt.Errorf("worker %d iteration %d: %w", worker, i, err)
						return
					}
					if res.Verdict != tc.want {
						failures <- fmt.Errorf("worker %d iteration %d: got %s, want %s (%v)", worker, i, res.Verdict, tc.want, res.Failure)
						return
					}
				}
			}
		}(worker)
	}
	close(start)
	wg.Wait()
	close(failures)
	for err := range failures {
		t.Error(err)
	}
}

func TestHybridUnicodeDiagnosticPosition(t *testing.T) {
	content := "αβx\nplain\r\n"
	fset := token.NewFileSet()
	file := fset.AddFile("unicode.hy", -1, len(content))
	file.SetLinesForContent([]byte(content))
	for _, tc := range []struct{ line, col, want int }{
		{1, 1, 0}, {1, 2, len("α")}, {1, 3, len("αβ")},
		{1, 4, len("αβx")}, {2, 3, len("αβx\npl")},
		{2, 6, len("αβx\nplain")},
	} {
		pos, err := hybridTokenPos(file, content, tc.line, tc.col)
		if err != nil {
			t.Fatalf("%d:%d: %v", tc.line, tc.col, err)
		}
		if got := file.Offset(pos); got != tc.want {
			t.Fatalf("%d:%d mapped to byte %d; want %d", tc.line, tc.col, got, tc.want)
		}
	}
	for _, tc := range []struct{ line, col int }{{1, 5}, {2, 7}, {3, 2}, {0, 1}, {1, 0}} {
		if _, err := hybridTokenPos(file, content, tc.line, tc.col); err == nil {
			t.Errorf("out-of-range %d:%d was accepted", tc.line, tc.col)
		}
	}
}

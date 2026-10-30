package analyzer

import (
	"os"
	"path/filepath"
	"testing"
)

// This test exercises two independent runtimes sharing only a disk compilation
// cache. It validates semantics and cache error handling, not a timing target.
func TestWasmCompilationCachePreservesCheckerVerdicts(t *testing.T) {
	cacheDir := t.TempDir()
	accept, err := os.ReadFile("../../tests/checker-corpus/09-a3-map-slot-valid.hy")
	if err != nil {
		t.Fatal(err)
	}
	reject, err := os.ReadFile("../../tests/checker-corpus/08-a3-map-slot-invalidated.hy")
	if err != nil {
		t.Fatal(err)
	}
	for run := 0; run < 2; run++ {
		engine, err := newEngine(defaultWasmPath, cacheDir)
		if err != nil {
			t.Fatalf("runtime #%d open: %v", run, err)
		}
		for _, tc := range []struct {
			src     string
			verdict string
		}{
			{string(accept), "accept"}, {string(reject), "reject"},
		} {
			out, checkErr := engine.Check(tc.src)
			if checkErr != nil || out.Summary.Verdict != tc.verdict {
				t.Fatalf("runtime #%d verdict expected %s, output=%+v, err=%v", run, tc.verdict, out, checkErr)
			}
		}
		if err := engine.Close(); err != nil {
			t.Fatalf("close runtime #%d: %v", run, err)
		}
	}
}

func TestWasmCompilationCacheDirectoryFailureIsFailClosed(t *testing.T) {
	file := filepath.Join(t.TempDir(), "not-a-cache-directory")
	if err := os.WriteFile(file, []byte("occupied"), 0600); err != nil {
		t.Fatal(err)
	}
	if engine, err := newEngine(defaultWasmPath, file); err == nil {
		if engine != nil {
			_ = engine.Close()
		}
		t.Fatal("cache configuration error silently fell back to noncached runtime")
	}
}

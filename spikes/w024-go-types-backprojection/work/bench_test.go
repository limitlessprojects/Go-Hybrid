package decider

import (
	"testing"
)

var benchmarkSource = []byte(`package main

import "fmt"

//line test.hy:1:1
type Job struct {
	ID   int
	Name string
}

//line test.hy:6:1
func process(j Job) int {
//line test.hy:7:5
	if j.ID < 0 {
		return 0
	}
//line test.hy:10:5
	return j.ID * 42
}

//line test.hy:13:1
func main() {
//line test.hy:14:5
	j := Job{ID: 1, Name: "task"}
//line test.hy:15:5
	res := process(j)
//line test.hy:16:5
	fmt.Println(res)
}
`)

func BenchmarkWarmCheckLatency(b *testing.B) {
	importer := NewCachedImporter()
	if err := importer.WarmStdlib("fmt", "os", "sync", "time", "errors"); err != nil {
		b.Fatalf("failed to warm stdlib: %v", err)
	}

	dec := NewDecider(importer)

	// Ensure warm-up
	_, _ = dec.CheckSource("test.hy.go", benchmarkSource)

	b.ResetTimer()
	b.ReportAllocs()

	for i := 0; i < b.N; i++ {
		diags, err := dec.CheckSource("test.hy.go", benchmarkSource)
		if err != nil || len(diags) > 0 {
			b.Fatalf("unexpected failure: err=%v diags=%v", err, diags)
		}
	}
}

package decider

import "testing"

var benchSrc = []byte(`package main

import "fmt"

//line app.hy:1:1
type Task struct {
	ID    int
	Title string
}

//line app.hy:7:1
func execute(t Task) int {
	if t.ID < 0 {
		return 0
	}
	return t.ID * 10
}

//line app.hy:15:1
func main() {
	t := Task{ID: 7, Title: "run"}
	v := execute(t)
	fmt.Println(v)
}
`)

func BenchmarkWarmCheckLatency(b *testing.B) {
	imp := NewCachedImporter()
	if err := imp.WarmStdlib("fmt", "os", "sync", "time", "errors"); err != nil {
		b.Fatalf("warm stdlib failed: %v", err)
	}
	dec := NewDecider(imp)
	_, _ = dec.CheckSource("app.hy.go", benchSrc)

	b.ResetTimer()
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		diags, err := dec.CheckSource("app.hy.go", benchSrc)
		if err != nil || len(diags) > 0 {
			b.Fatalf("unexpected: err=%v diags=%v", err, diags)
		}
	}
}

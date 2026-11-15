package decider

import (
	"fmt"
	"strings"
	"testing"
)

var benchmarkSmallSource = []byte(`package main

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

var benchmarkMediumSource = buildMediumBenchmarkSource(24)

func buildMediumBenchmarkSource(fnCount int) []byte {
	var sb strings.Builder
	sb.WriteString("package main\n\nimport (\n\t\"errors\"\n\t\"fmt\"\n\t\"time\"\n)\n\n")
	sb.WriteString("//line medium.hy:1:1\ntype Record struct {\n\tID int\n\tLabel string\n\tCreated time.Time\n}\n\n")
	for i := 0; i < fnCount; i++ {
		line := 10 + i*9
		fmt.Fprintf(&sb, "//line medium.hy:%d:1\n", line)
		fmt.Fprintf(&sb, "func worker%d(r Record, limit int) (int, error) {\n", i)
		fmt.Fprintf(&sb, "//line medium.hy:%d:2\n", line+1)
		fmt.Fprintf(&sb, "\tif limit <= 0 {\n\t\treturn 0, errors.New(\"invalid limit\")\n\t}\n")
		fmt.Fprintf(&sb, "//line medium.hy:%d:2\n", line+4)
		fmt.Fprintf(&sb, "\tacc := r.ID + %d\n", i)
		fmt.Fprintf(&sb, "\tfor k := 0; k < limit; k++ {\n\t\tacc += k\n\t}\n")
		fmt.Fprintf(&sb, "//line medium.hy:%d:2\n", line+7)
		fmt.Fprintf(&sb, "\treturn acc, nil\n}\n\n")
	}
	sb.WriteString("func main() {\n\tr := Record{ID: 1, Label: \"bench\", Created: time.Unix(0, 0)}\n")
	sb.WriteString("\tv, err := worker0(r, 3)\n\tif err != nil {\n\t\tfmt.Println(err)\n\t\treturn\n\t}\n\tfmt.Println(v)\n}\n")
	return []byte(sb.String())
}

func runWarmBenchmark(b *testing.B, filename string, src []byte) {
	b.Helper()
	importer := NewCachedImporter()
	if err := importer.WarmStdlib("fmt", "os", "sync", "time", "errors"); err != nil {
		b.Fatalf("failed to warm stdlib: %v", err)
	}

	dec := NewDecider(importer)

	// Ensure warm-up
	if diags, err := dec.CheckSource(filename, src); err != nil || len(diags) > 0 {
		b.Fatalf("warm-up failure: err=%v diags=%v", err, diags)
	}

	b.ResetTimer()
	b.ReportAllocs()

	for i := 0; i < b.N; i++ {
		diags, err := dec.CheckSource(filename, src)
		if err != nil || len(diags) > 0 {
			b.Fatalf("unexpected failure: err=%v diags=%v", err, diags)
		}
	}
}

func BenchmarkWarmCheckLatency(b *testing.B) {
	b.Run("Small_30LOC", func(b *testing.B) {
		runWarmBenchmark(b, "test.hy.go", benchmarkSmallSource)
	})
	b.Run("Medium_250LOC", func(b *testing.B) {
		runWarmBenchmark(b, "medium.hy.go", benchmarkMediumSource)
	})
}

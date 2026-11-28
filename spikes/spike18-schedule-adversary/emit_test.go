package periodic

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"testing"
	"time"
)

// This interpreter deliberately uses growing/sliced FIFO lists, not emitted
// rings or the certificate checker's token-accounting implementation. Golden
// formulas below also catch shared misunderstandings of actor semantics.
const referenceTest = `
import (
    "encoding/json"
    "fmt"
    "strings"
    "testing"
    "unsafe"
)
type referenceInput struct {
    Graph struct {
        Actors []struct { Op string; Factor uint64 }
        Edges []struct { From, To, Produce, Consume int; Initial []uint64 }
    }
    Certificate struct { Schedule []int }
}
type interpreter struct {
    in referenceInput
    queues [][]uint64
    sources, sinks []uint64
}
func newInterpreter(t *testing.T) *interpreter {
    var in referenceInput
    if err := json.Unmarshal([]byte(inputJSON), &in); err != nil { t.Fatal(err) }
    r := &interpreter{in:in, queues:make([][]uint64,len(in.Graph.Edges)), sources:make([]uint64,len(in.Graph.Actors)), sinks:make([]uint64,len(in.Graph.Actors))}
    for i, e := range in.Graph.Edges { r.queues[i] = append([]uint64(nil),e.Initial...) }
    return r
}
func (r *interpreter) step() []uint64 {
    for _, a := range r.in.Certificate.Schedule {
        var sum uint64
        for i, e := range r.in.Graph.Edges {
            if e.To != a { continue }
            if len(r.queues[i]) < e.Consume { panic("reference underflow") }
            for _, x := range r.queues[i][:e.Consume] { sum += x }
            r.queues[i] = r.queues[i][e.Consume:]
        }
        switch actor := r.in.Graph.Actors[a]; actor.Op {
        case "source": r.sources[a]++; sum = r.sources[a]
        case "scale": sum *= actor.Factor
        case "sink": r.sinks[a] += sum
        }
        for i, e := range r.in.Graph.Edges {
            if e.From == a {
                for j:=0; j<e.Produce; j++ { r.queues[i] = append(r.queues[i],sum) }
            }
        }
    }
    var out []uint64
    for i,a := range r.in.Graph.Actors { if a.Op == "sink" { out = append(out,r.sinks[i]) } }
    return out
}
func mustPanic(t *testing.T, fragment string, f func()) {
    t.Helper()
    defer func() {
        p:=recover()
        if p==nil || !strings.Contains(fmt.Sprint(p),fragment) { t.Fatalf("panic=%v; want %s",p,fragment) }
    }()
    f()
}
var allocationResult [1]uint64
func TestStepAllocations(t *testing.T) {
    k:=New()
    allocs:=testing.AllocsPerRun(10000,func(){ allocationResult = k.Step() })
    if allocs != 0 { t.Fatalf("allocations/Step = %g",allocs) }
    t.Logf("allocations/Step = %g (10000 measured runs)",allocs)
}
`

func generatedTests(t *testing.T, in Input, name string) []byte {
	t.Helper()
	var b bytes.Buffer
	b.WriteString("package kernel\n")
	b.WriteString(referenceTest)
	data, err := json.Marshal(in)
	if err != nil {
		t.Fatal(err)
	}
	fmt.Fprintf(&b, "const inputJSON = %q\n", data)
	b.WriteString("func TestOutputsAndFIFOValues(t *testing.T) { k:=New(); r:=newInterpreter(t); for n:=uint64(1);n<=100;n++ { got:=k.Step(); want:=r.step();\n")
	golden := map[string]string{
		"pipeline":  "3*n*(n+1)/2",
		"multirate": "9*n*n+3*n",
		"join":      "n*(n+1)",
		"feedback":  "(uint64(1) << uint(n+1))-uint64(2)",
	}
	formula, ok := golden[name]
	if !ok {
		t.Fatal("missing independent hand-derived formula")
	}
	fmt.Fprintf(&b, "golden := %s\n", formula)
	b.WriteString("if len(want)!=1 || got[0]!=want[0] || got[0]!=golden { t.Fatalf(\"period %d: kernel=%v interpreter=%v golden=%d\",n,got,want,golden) }\n")
	for i, cap := range in.Certificate.Capacities {
		fmt.Fprintf(&b, "if k.n%d != len(r.queues[%d]) { t.Fatal(\"FIFO marking mismatch\") }; for j,v:= range r.queues[%d] { if k.q%d[(k.r%d+j)%%%d] != v { t.Fatal(\"FIFO value/order mismatch\") } }\n", i, i, i, i, i, cap)
	}
	b.WriteString("} }\nfunc TestQueueGuardNegativeControls(t *testing.T) {\n")
	for i, e := range in.Graph.Edges {
		fmt.Fprintf(&b, "t.Run(%q,func(t *testing.T) { k:=New(); k.n%d=0; mustPanic(t,\"FIFO underflow\",func(){k.pop%d()}) })\n", e.Name+"/underflow", i, i)
		fmt.Fprintf(&b, "t.Run(%q,func(t *testing.T) { k:=New(); k.n%d=%d; mustPanic(t,\"FIFO overflow\",func(){k.push%d(1)}) })\n", e.Name+"/overflow", i, in.Certificate.Capacities[i], i)
	}
	b.WriteString("}\n")
	b.WriteString("func TestPayloadAccounting(t *testing.T) { k:=New(); var arrays uintptr\n")
	for i := range in.Graph.Edges {
		fmt.Fprintf(&b, "arrays += unsafe.Sizeof(k.q%d)\n", i)
	}
	fmt.Fprintf(&b, "if uint64(arrays) != PayloadBytes || PayloadBytes != %d { t.Fatalf(\"payload: arrays=%%d declared=%%d\", arrays,PayloadBytes) }; t.Logf(\"FIFO arrays=%%d bytes; complete Kernel=%%d bytes\",arrays,unsafe.Sizeof(*k)) }\n", in.Certificate.PayloadBytes)
	return b.Bytes()
}

func TestGeneratedKernels(t *testing.T) {
	for _, name := range []string{"pipeline", "multirate", "join", "feedback"} {
		t.Run(name, func(t *testing.T) {
			in := fixture(t, name)
			code, err := Emit(in.Graph, in.Certificate, "kernel")
			if err != nil {
				t.Fatal(err)
			}
			dir := t.TempDir()
			for filename, contents := range map[string][]byte{
				"go.mod":         []byte("module generated-kernel\n\ngo 1.26.9\n"),
				"kernel.go":      code,
				"kernel_test.go": generatedTests(t, in, name),
			} {
				if err := os.WriteFile(filepath.Join(dir, filename), contents, 0600); err != nil {
					t.Fatal(err)
				}
			}
			ctx, cancel := context.WithTimeout(context.Background(), 2*time.Minute)
			defer cancel()
			cmd := exec.CommandContext(ctx, "go", "test", "-v", "-count=1", "./...")
			cmd.Dir = dir
			cmd.Env = append(os.Environ(), "GOTOOLCHAIN=local", "GOWORK=off")
			out, err := cmd.CombinedOutput()
			t.Logf("generated kernel test output:\n%s", out)
			if err != nil {
				t.Fatalf("generated kernel failed: %v", err)
			}
		})
	}
}

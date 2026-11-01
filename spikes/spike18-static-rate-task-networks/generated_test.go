package staticnetwork
import "testing"

func TestKernel0Reference(t *testing.T) {
k := NewKernel0()
for range 50 { k.Step() }
if k.Next != 50 || k.Checksum != 3825 { t.Fatalf("reference mismatch: next=%d checksum=%d", k.Next, k.Checksum) }
if k.n0 != 0 { t.Fatal("periodic marking differs") }
if k.n1 != 0 { t.Fatal("periodic marking differs") }
}
func TestKernel0NoStepAllocations(t *testing.T) {
k := NewKernel0()
allocs := testing.AllocsPerRun(10000, func() { k.Step() })
if allocs != 0 { t.Fatalf("allocations/step: %v", allocs) }
t.Logf("allocations/step: %v", allocs)
}

func TestKernel1Reference(t *testing.T) {
k := NewKernel1()
for range 50 { k.Step() }
if k.Next != 150 || k.Checksum != 22650 { t.Fatalf("reference mismatch: next=%d checksum=%d", k.Next, k.Checksum) }
if k.n0 != 0 { t.Fatal("periodic marking differs") }
if k.n1 != 0 { t.Fatal("periodic marking differs") }
}
func TestKernel1NoStepAllocations(t *testing.T) {
k := NewKernel1()
allocs := testing.AllocsPerRun(10000, func() { k.Step() })
if allocs != 0 { t.Fatalf("allocations/step: %v", allocs) }
t.Logf("allocations/step: %v", allocs)
}

func TestKernel2Reference(t *testing.T) {
k := NewKernel2()
for range 50 { k.Step() }
if k.Next != 100 || k.Checksum != 5050 { t.Fatalf("reference mismatch: next=%d checksum=%d", k.Next, k.Checksum) }
if k.n0 != 0 { t.Fatal("periodic marking differs") }
if k.n1 != 0 { t.Fatal("periodic marking differs") }
if k.n2 != 0 { t.Fatal("periodic marking differs") }
}
func TestKernel2NoStepAllocations(t *testing.T) {
k := NewKernel2()
allocs := testing.AllocsPerRun(10000, func() { k.Step() })
if allocs != 0 { t.Fatalf("allocations/step: %v", allocs) }
t.Logf("allocations/step: %v", allocs)
}

func TestKernel3Reference(t *testing.T) {
k := NewKernel3()
for range 50 { k.Step() }
if k.Next != 0 || k.Checksum != 0 { t.Fatalf("reference mismatch: next=%d checksum=%d", k.Next, k.Checksum) }
if k.n0 != 0 { t.Fatal("periodic marking differs") }
if k.n1 != 1 { t.Fatal("periodic marking differs") }
if k.pop1() != 1125899906842624 { t.Fatal("feedback token differs") }
}
func TestKernel3NoStepAllocations(t *testing.T) {
k := NewKernel3()
allocs := testing.AllocsPerRun(10000, func() { k.Step() })
if allocs != 0 { t.Fatalf("allocations/step: %v", allocs) }
t.Logf("allocations/step: %v", allocs)
}

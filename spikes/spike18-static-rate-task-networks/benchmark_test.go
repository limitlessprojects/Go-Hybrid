package staticnetwork

import "testing"

var result uint64

func BenchmarkStaticPipeline(b *testing.B) {
	k := NewKernel0()
	b.ReportAllocs()
	for b.Loop() {
		k.Step()
	}
	result = k.Checksum
}

// Persistent worker: setup is excluded for both implementations. This measures
// one input -> triple -> sink round trip, not startup or parallel throughput.
func BenchmarkChannelPipeline(b *testing.B) {
	in := make(chan uint64)
	out := make(chan uint64)
	done := make(chan struct{})
	go func() {
		defer close(done)
		for n := range in {
			out <- n * 3
		}
	}()
	var next, checksum uint64
	b.ReportAllocs()
	b.ResetTimer()
	for b.Loop() {
		next++
		in <- next
		checksum += <-out
	}
	b.StopTimer()
	close(in)
	<-done
	result = checksum
}

func BenchmarkPlainSequential(b *testing.B) {
	var next, checksum uint64
	b.ReportAllocs()
	for b.Loop() {
		next++
		checksum += next * 3
	}
	result = checksum
}

func TestPersistentChannelMatchesStatic(t *testing.T) {
	in, out := make(chan uint64), make(chan uint64)
	done := make(chan struct{})
	go func() {
		defer close(done)
		for n := range in {
			out <- n * 3
		}
	}()
	k := NewKernel0()
	var sum uint64
	for n := uint64(1); n <= 10000; n++ {
		in <- n
		sum += <-out
		k.Step()
	}
	close(in)
	<-done
	if k.Checksum != sum || sum != 150015000 {
		t.Fatalf("static=%d channel=%d", k.Checksum, sum)
	}
}

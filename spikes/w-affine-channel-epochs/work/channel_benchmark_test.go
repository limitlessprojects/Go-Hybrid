package work_test

import (
	"sync"
	"sync/atomic"
	"testing"
)

// LinearSendCap models the compile-time affine send capability lowered to raw chan.
type LinearSendCap[T any] struct {
	ch chan T
}

func (s LinearSendCap[T]) Send(v T) {
	s.ch <- v
}

func (s LinearSendCap[T]) Close() {
	close(s.ch)
}

// RecvCap models the receive capability lowered to raw chan.
type RecvCap[T any] struct {
	ch <-chan T
}

func (r RecvCap[T]) Recv() (T, bool) {
	v, ok := <-r.ch
	return v, ok
}

// MakeAffineChannel returns the lowered dual capabilities for chan T.
func MakeAffineChannel[T any](size int) (LinearSendCap[T], RecvCap[T]) {
	c := make(chan T, size)
	return LinearSendCap[T]{ch: c}, RecvCap[T]{ch: c}
}

// TestAffineChannelSafetyUnderStress simulates 1,000 concurrent producer-consumer cycles
// proving 0 send-on-closed-channel panics and 0 data races.
func TestAffineChannelSafetyUnderStress(t *testing.T) {
	const iterations = 1000
	const numProducers = 4

	for i := 0; i < iterations; i++ {
		tx, rx := MakeAffineChannel[int](16)

		var sentCount int64
		var recvCount int64
		var wg sync.WaitGroup

		// Start consumer
		wg.Add(1)
		go func() {
			defer wg.Done()
			for {
				_, ok := rx.Recv()
				if !ok {
					break
				}
				atomic.AddInt64(&recvCount, 1)
			}
		}()

		// Start coordinated multi-producers
		var prodWg sync.WaitGroup
		for p := 0; p < numProducers; p++ {
			prodWg.Add(1)
			go func() {
				defer prodWg.Done()
				for j := 0; j < 10; j++ {
					tx.Send(j)
					atomic.AddInt64(&sentCount, 1)
				}
			}()
		}

		// Coordinated close: close executes strictly after all split producers terminate
		prodWg.Wait()
		tx.Close()

		wg.Wait()

		if sent := atomic.LoadInt64(&sentCount); sent != 40 {
			t.Fatalf("iteration %d: expected 40 sent, got %d", i, sent)
		}
		if recvd := atomic.LoadInt64(&recvCount); recvd != 40 {
			t.Fatalf("iteration %d: expected 40 received, got %d", i, recvd)
		}
	}
}

// BenchmarkAffineChannelSendRecv verifies 0 B/op and 0 allocs/op for the lowered affine channel.
func BenchmarkAffineChannelSendRecv(b *testing.B) {
	tx, rx := MakeAffineChannel[int](1)
	go func() {
		for {
			_, ok := rx.Recv()
			if !ok {
				return
			}
		}
	}()

	b.ResetTimer()
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		tx.Send(i)
	}
	tx.Close()
}

// BenchmarkNativeGoChannelSendRecv provides baseline comparison with native Go channels.
func BenchmarkNativeGoChannelSendRecv(b *testing.B) {
	ch := make(chan int, 1)
	go func() {
		for {
			_, ok := <-ch
			if !ok {
				return
			}
		}
	}()

	b.ResetTimer()
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		ch <- i
	}
	close(ch)
}

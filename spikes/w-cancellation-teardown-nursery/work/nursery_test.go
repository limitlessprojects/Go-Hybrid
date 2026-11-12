package work

import (
	"context"
	"math/rand"
	"runtime"
	"testing"
	"time"
)

func TestNurseryZeroLeakUnderRandomizedCancellation(t *testing.T) {
	initialGoroutines := runtime.NumGoroutine()

	for iter := 0; iter < 1000; iter++ {
		parent := context.Background()
		nursery := NewTaskNursery(parent)
		ch := NewCooperativeChannel[int](16)

		// Spawn 4 worker tasks
		for w := 0; w < 4; w++ {
			nursery.Spawn(func(ctx context.Context, witness *CancelWitness) {
				for {
					v, ok := ch.Recv(ctx)
					if !ok {
						ch.Drain()
						witness.Abort()
						return
					}
					_ = v
				}
			})
		}

		// Feeder task
		nursery.Spawn(func(ctx context.Context, witness *CancelWitness) {
			for i := 0; i < 50; i++ {
				if !ch.Send(ctx, i) {
					break
				}
			}
			witness.Complete()
		})

		// Random cancellation timing
		if rand.Float32() < 0.5 {
			time.Sleep(time.Duration(rand.Intn(200)) * time.Microsecond)
			nursery.Cancel()
		} else {
			time.Sleep(500 * time.Microsecond)
			ch.Close()
		}

		nursery.Wait()
		if nursery.ActiveCount() != 0 {
			t.Fatalf("iteration %d: active tasks remaining = %d", iter, nursery.ActiveCount())
		}
	}

	// Settle goroutines
	time.Sleep(10 * time.Millisecond)
	finalGoroutines := runtime.NumGoroutine()
	diff := finalGoroutines - initialGoroutines
	if diff > 2 {
		t.Fatalf("goroutine leak detected: diff = %d (initial %d, final %d)", diff, initialGoroutines, finalGoroutines)
	}
}

func BenchmarkNurseryTeardown(b *testing.B) {
	b.ReportAllocs()
	parent := context.Background()

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		nursery := NewTaskNursery(parent)
		nursery.Spawn(func(ctx context.Context, witness *CancelWitness) {
			witness.Complete()
		})
		nursery.Wait()
	}
}

func BenchmarkCooperativeDrain(b *testing.B) {
	b.ReportAllocs()
	ch := NewCooperativeChannel[int](32)

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		for j := 0; j < 16; j++ {
			ch.ch <- j
		}
		ch.Drain()
	}
}

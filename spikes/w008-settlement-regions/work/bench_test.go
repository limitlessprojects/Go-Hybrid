package w008

import (
	"context"
	"sync"
	"testing"
)

func BenchmarkBaselineWaitGroup(b *testing.B) {
	for i := 0; i < b.N; i++ {
		var wg sync.WaitGroup
		wg.Add(2)
		go func() {
			wg.Done()
		}()
		go func() {
			wg.Done()
		}()
		wg.Wait()
	}
}

func BenchmarkSettlementRegion_Transfer(b *testing.B) {
	ctx := context.Background()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		region := NewRegion(ctx)
		r1 := newResource("res-1")
		r2 := newResource("res-2")

		slot1, _ := RegisterHandle(region, "h1", r1, func(r *MockResource) { r.release() })
		slot2, _ := RegisterHandle(region, "h2", r2, func(r *MockResource) { r.release() })

		region.SpawnTask(func(c context.Context) {
			h, ok := slot1.TryAcquire()
			if ok {
				TransferResult(region, h)
			}
		})
		region.SpawnTask(func(c context.Context) {
			h, ok := slot2.TryAcquire()
			if ok {
				TransferResult(region, h)
			}
		})

		_, _ = region.WaitSettled()
	}
}

func BenchmarkSettlementRegion_CancelRelease(b *testing.B) {
	ctx := context.Background()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		region := NewRegion(ctx)
		r1 := newResource("res-1")
		slot1, _ := RegisterHandle(region, "h1", r1, func(r *MockResource) { r.release() })

		region.Cancel() // immediate cancel
		region.SpawnTask(func(c context.Context) {
			slot1.TryAcquire()
		})
		_, _ = region.WaitSettled()
	}
}

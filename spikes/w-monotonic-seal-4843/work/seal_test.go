package wseal

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"sync"
	"testing"
)

type routeRgn struct{}
type sliceRgn struct{}

// TestP1_ZeroCopyBroadcastAndSenderReadAfterSend verifies Hypothesis P1 (and
// resolves SPEC-LANGUAGE.md §13.2 D1 / corpus #19):
//  1. `SealMap` and `SealSlice` consume `Own[..., Live] -> Own[..., Moved]`
//     with 0 allocations and yield `SealedMap` / `SealedSlice` (Class 4b).
//  2. Unlike `move agg` (where receiver mutation makes sender's `len(agg)` a
//     data race in corpus #19), `SealedMap` can be broadcast to 16 concurrent
//     worker tasks while the sender simultaneously reads `.Len()` and `.Get()`.
func TestP1_ZeroCopyBroadcastAndSenderReadAfterSend(t *testing.T) {
	own := NewOwnMap[routeRgn, string, int](64)
	for i := 0; i < 64; i++ {
		PutMap(&own, fmt.Sprintf("/v1/route/%d", i), i*10+1)
	}

	movedOwn, snap := SealMap(own)
	_ = movedOwn

	const workers = 16
	const readsPerWorker = 4000
	ch := make(chan SealedMap[string, int], workers)
	var wg sync.WaitGroup

	for w := 0; w < workers; w++ {
		wg.Add(1)
		go func(id int) {
			defer wg.Done()
			tbl := <-ch
			for i := 0; i < readsPerWorker; i++ {
				key := fmt.Sprintf("/v1/route/%d", (id+i)&63)
				got, ok := tbl.Get(key)
				want := ((id+i)&63)*10 + 1
				if !ok || got != want {
					t.Errorf("worker %d Get(%q) = (%d, %v), want (%d, true)", id, key, got, ok, want)
					return
				}
			}
		}(w)
	}

	// Broadcast the exact same SealedMap value across all 16 tasks without cloning.
	for w := 0; w < workers; w++ {
		ch <- snap
	}
	close(ch)

	// Corpus #19 twin: sender continues reading `.Len()` and `.Get()` concurrently
	// with the 16 receiver tasks — 100% sound because zero writer exists anywhere.
	for i := 0; i < 1000; i++ {
		if snap.Len() != 64 {
			t.Fatalf("sender snap.Len() = %d, want 64", snap.Len())
		}
		if v, ok := snap.Get("/v1/route/7"); !ok || v != 71 {
			t.Fatalf("sender snap.Get(/v1/route/7) = (%d, %v), want (71, true)", v, ok)
		}
	}

	wg.Wait()
}

// TestP1_NegativeWitnessRejectionByGoBuild verifies that post-seal writes on
// consumed `Moved` handles and direct mutation attempts on `SealedMap` /
// `SealedSlice` are rejected at compile time by `go build`.
func TestP1_NegativeWitnessRejectionByGoBuild(t *testing.T) {
	negFiles := []string{
		filepath.Join("negative", "uam_write_after_seal_map.go"),
		filepath.Join("negative", "uam_write_after_seal_slice.go"),
		filepath.Join("negative", "mutate_sealed_map_projection.go"),
	}
	for _, rel := range negFiles {
		cmd := exec.Command("go", "build", "-o", filepath.Join(t.TempDir(), "neg.bin"), rel)
		out, err := cmd.CombinedOutput()
		if err == nil {
			t.Fatalf("expected `go build %s` to fail type-checking, but it succeeded", rel)
		}
		if len(out) == 0 {
			t.Fatalf("expected compiler diagnostic for %s, got empty output", rel)
		}
	}
}

// TestP2_ZeroAllocsOnSealTransition verifies that both `SealMap` and `SealSlice`
// execute in O(1) with strictly 0 heap allocations (`0 allocs/op`).
func TestP2_ZeroAllocsOnSealTransition(t *testing.T) {
	rawMap := NewOwnMap[routeRgn, int, int](1024)
	for i := 0; i < 1024; i++ {
		PutMap(&rawMap, i, i*3)
	}
	rawSlice := NewOwnSlice[sliceRgn, int](256, 512)
	for i := 0; i < 256; i++ {
		SetSlice(&rawSlice, i, i)
	}

	mapAllocs := testing.AllocsPerRun(100, func() {
		m := OwnMap[routeRgn, int, int, Live]{data: rawMap.data}
		_, s := SealMap(m)
		if s.Len() != 1024 {
			t.Fatal("bad map len")
		}
	})
	if mapAllocs != 0 {
		t.Fatalf("SealMap allocated %.2f times, want 0", mapAllocs)
	}

	sliceAllocs := testing.AllocsPerRun(100, func() {
		s := OwnSlice[sliceRgn, int, Live]{data: rawSlice.data}
		_, sealed := SealSlice(s)
		if sealed.Len() != 256 || sealed.Cap() != 256 {
			t.Fatalf("SealSlice len/cap = %d/%d, want 256/256", sealed.Len(), sealed.Cap())
		}
	})
	if sliceAllocs != 0 {
		t.Fatalf("SealSlice allocated %.2f times, want 0", sliceAllocs)
	}
}

// TestP3_Row9_SealedForeignCallbackGuaranteesZeroMutation verifies Hypothesis P3
// (flipping GUARANTEES.md Rows 5 & 9): a foreign callback (modeling `http.Handler`)
// that captures a `SealedMap` serves concurrent requests with zero write paths.
func TestP3_Row9_SealedForeignCallbackGuaranteesZeroMutation(t *testing.T) {
	own := NewOwnMap[routeRgn, string, string](8)
	PutMap(&own, "/health", "200 OK")
	PutMap(&own, "/metrics", "200 METRICS")
	_, sealedRoutes := SealMap(own)

	dispatch := RegisterSealedHandler(sealedRoutes, func(tbl SealedMap[string, string], path string) (string, bool) {
		return tbl.Get(path)
	})

	var wg sync.WaitGroup
	for g := 0; g < 12; g++ {
		wg.Add(1)
		go func(id int) {
			defer wg.Done()
			for i := 0; i < 2000; i++ {
				resp, ok := dispatch("/health")
				if !ok || resp != "200 OK" {
					t.Errorf("dispatch(/health) = (%q, %v), want (200 OK, true)", resp, ok)
					return
				}
			}
		}(g)
	}
	wg.Wait()
}

// TestF1_Falsifier_UnclippedSliceAppendCorruptsSharedCapacityAndRemedyR1 verifies
// Declared Falsifier F1 + Remedy R1:
//   - F1 (Falsifier): Sealing a slice with spare capacity (`len=2, cap=8`) WITHOUT
//     3-index capacity clipping lets a seemingly pure derived `append` overwrite
//     slot 2 of the shared backing array, silently corrupting an earlier reader's
//     derived slice (`readerA[2]` flips from 111 to 222!).
//   - R1 (Remedy): `SealSlice` enforces `s[:len(s):len(s)]` (`cap == len`), forcing
//     every `DeriveAppend` onto an isolated backing array (`readerA[2] == 111`,
//     `readerB[2] == 222`, and 8 concurrent `DeriveAppend` workers never collide).
func TestF1_Falsifier_UnclippedSliceAppendCorruptsSharedCapacityAndRemedyR1(t *testing.T) {
	// 1. Falsifier F1: unclipped slice (`len=2, cap=8`) silently clobbers spare capacity.
	backing := make([]int, 2, 8)
	backing[0], backing[1] = 10, 20
	naive := NaiveUnclippedSealSlice(backing)

	readerA := naive.DeriveAppendUnsafe(111)
	readerB := naive.DeriveAppendUnsafe(222)
	if readerA[2] != 222 || readerB[2] != 222 {
		t.Fatalf("F1 falsifier did not fire: expected readerB append to overwrite readerA[2] to 222, got readerA[2]=%d", readerA[2])
	}

	// 2. Remedy R1: `SealSlice` clips capacity (`s[:n:n]`), eliminating spare-cap writes.
	own := NewOwnSlice[sliceRgn, int](2, 8)
	SetSlice(&own, 0, 10)
	SetSlice(&own, 1, 20)
	_, clipped := SealSlice(own)

	if clipped.Cap() != clipped.Len() {
		t.Fatalf("R1 failed: expected Cap()==Len()==2, got Cap()=%d Len()=%d", clipped.Cap(), clipped.Len())
	}
	safeA := clipped.DeriveAppend(111)
	safeB := clipped.DeriveAppend(222)
	if safeA[2] != 111 || safeB[2] != 222 {
		t.Fatalf("R1 failed: safeA[2]=%d (want 111), safeB[2]=%d (want 222)", safeA[2], safeB[2])
	}

	// Stress 8 concurrent readers deriving independent slices from `clipped`.
	var wg sync.WaitGroup
	for w := 0; w < 8; w++ {
		wg.Add(1)
		go func(id int) {
			defer wg.Done()
			for i := 0; i < 1000; i++ {
				derived := clipped.DeriveAppend(id*10000 + i)
				if derived[0] != 10 || derived[1] != 20 || derived[2] != id*10000+i {
					t.Errorf("worker %d derived corruption: %v", id, derived)
					return
				}
			}
		}(w)
	}
	wg.Wait()
}

// TestF2_Falsifier_PreSealInteriorAliasMutatesSealedStorage verifies Declared
// Falsifier F2 (4th independent convergence on the phantom-witness / `may_share`
// boundary after #370, #507, and W-022 #545):
// Consuming the outer `OwnSlice[..., Live] -> Moved` witness alone does NOT stop
// a pre-existing interior sub-slice alias (`leaked := raw[:1]`) from mutating
// the sealed backing array; `freeze.rs` `storage_component` (`aliases U may_share`)
// write-freeze (Remedy R2) is mandatory.
func TestF2_Falsifier_PreSealInteriorAliasMutatesSealedStorage(t *testing.T) {
	own, leakedSub := NewOwnSliceWithPreSealSubslice[sliceRgn, int]([]int{42, 99})
	_, sealed := SealSlice(own)

	if sealed.At(0) != 42 {
		t.Fatalf("initial sealed.At(0) = %d, want 42", sealed.At(0))
	}
	// Mutating through the pre-seal interior sub-slice bypasses the outer witness!
	leakedSub[0] = 7777
	if sealed.At(0) != 7777 {
		t.Fatalf("F2 falsifier did not fire: expected leaked pre-seal sub-slice to mutate sealed.At(0) to 7777, got %d", sealed.At(0))
	}
}

// TestF3_Falsifier_StatefulInteriorClosureLaundersMutationThroughSealedStruct
// verifies Declared Falsifier F3: sealing a struct that carries an interior
// stateful closure (`func() int`) without recursive representation-purity
// checking (`is_sealable(T)` / `HYB-E3015`, Remedy R3) launders mutable captured
// state under an ostensibly immutable `SealedValue` wrapper.
func TestF3_Falsifier_StatefulInteriorClosureLaundersMutationThroughSealedStruct(t *testing.T) {
	hiddenCounter := 0
	cfg := UncheckedClosureConfig{
		Name: "prod",
		NextID: func() int {
			hiddenCounter++
			return hiddenCounter
		},
	}
	naiveSealed := NaiveUncheckedSealStruct(cfg)
	first := naiveSealed.Value().NextID()
	second := naiveSealed.Value().NextID()
	if first == second {
		t.Fatalf("F3 falsifier did not fire: expected stateful interior closure to mutate across reads (%d vs %d)", first, second)
	}
}

package w4843sealmove

import (
	"fmt"
	"reflect"
	"sync"
	"testing"
)

// P1: sealing is O(1)/zero-copy and the resulting witness broadcasts across
// many concurrent readers while the original "sender" goroutine keeps
// reading too (SPEC-LANGUAGE.md D1's missing zero-copy twin of corpus #19).
func TestP1_SealTransitionBroadcastsAcrossWorkersSenderStillReads(t *testing.T) {
	const n = 1000
	m := make(map[int]int, n)
	for i := 0; i < n; i++ {
		m[i] = i * 2
	}
	sealed := SealMap(m)

	const workers = 16
	var wg sync.WaitGroup
	errs := make(chan error, workers)
	for w := 0; w < workers; w++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for i := 0; i < n; i++ {
				v, ok := sealed.Get(i)
				if !ok || v != i*2 {
					errs <- fmt.Errorf("worker lookup mismatch at key %d: got (%d,%v)", i, v, ok)
					return
				}
			}
		}()
	}

	// The sender keeps reading the witness concurrently with the workers —
	// this is exactly the sender-side read that `move` bans (corpus #19)
	// and that `seal move` is meant to restore.
	for i := 0; i < n; i++ {
		if v, ok := sealed.Get(i); !ok || v != i*2 {
			t.Fatalf("sender mismatch at key %d: got (%d,%v)", i, v, ok)
		}
	}

	wg.Wait()
	close(errs)
	for e := range errs {
		t.Fatal(e)
	}
}

// P1 (encapsulation half): the Read-Only Projection Surface rule
// (HYB-E3006 in the proposal) means `sealed[T]` must not expose any
// mutating method — this is what makes "reject post-seal mutation" a
// go-build-time property instead of a runtime check.
func TestP1_SealedTypesExposeNoMutationMethods(t *testing.T) {
	forbidden := map[string]bool{"Set": true, "Delete": true, "Put": true, "Append": true}

	for _, typ := range []reflect.Type{
		reflect.TypeOf(SealedMap[int, int]{}),
		reflect.TypeOf(SealedSlice[int]{}),
	} {
		for i := 0; i < typ.NumMethod(); i++ {
			name := typ.Method(i).Name
			if forbidden[name] {
				t.Fatalf("%s exposes mutation method %s", typ, name)
			}
		}
	}
}

// F1 (declared falsifier, FIRES AS PREDICTED): sealing a slice without
// clipping spare capacity lets two independent "read-only-looking" appends
// through different aliases silently clobber each other in the shared
// backing array.
func TestF1_NaiveUnclippedSealSliceAppendSpareCapacityRace(t *testing.T) {
	backing := make([]int, 2, 8)
	backing[0], backing[1] = 1, 2

	readerA := naiveSealSlice(backing)
	readerB := naiveSealSlice(backing)

	// Both appends look read-only from each call site's point of view —
	// neither mutates its own header — but both write into the same spare
	// capacity slot (index 2) because naiveSealSlice never clipped cap==len.
	grownA := append(readerA.data, 111)
	grownB := append(readerB.data, 222)

	if grownA[2] == 111 {
		t.Fatalf("F1 did not fire: expected readerA's index 2 to have been clobbered by readerB's append, got %d", grownA[2])
	}
	if got := grownA[2]; got != grownB[2] || got != 222 {
		t.Fatalf("F1 did not fire as predicted: grownA[2]=%d grownB[2]=%d, want both 222", grownA[2], grownB[2])
	}
}

// F1 remedy R1: SealSlice's cap==len clip means the same two "read-only"
// appends on the sealed witnesses can never alias the same backing array,
// because any growth must allocate.
func TestF1Remedy_ClippedSealSliceAppendNeverAliases(t *testing.T) {
	backing := make([]int, 2, 8)
	backing[0], backing[1] = 1, 2

	sealedA := SealSlice(backing)
	sealedB := SealSlice(backing)

	grownA := append(sealedA.data, 111)
	grownB := append(sealedB.data, 222)

	if &grownA[0] == &grownB[0] {
		t.Fatalf("R1 did not hold: clipped seal still aliased a shared backing array")
	}
	if grownA[2] != 111 || grownB[2] != 222 {
		t.Fatalf("R1 did not hold: got grownA[2]=%d grownB[2]=%d, want 111 and 222 independently", grownA[2], grownB[2])
	}
	// The original sealed witnesses are untouched by either append.
	if sealedA.Len() != 2 || sealedB.Len() != 2 {
		t.Fatalf("sealed witnesses must stay at their sealed length, got %d and %d", sealedA.Len(), sealedB.Len())
	}
}

// F2 (declared falsifier, FIRES AS PREDICTED): a pre-seal sub-slice alias
// that survives the outer `seal move` witness can still mutate "sealed"
// data through the shared backing array. R1's capacity clip is NOT a
// defense here — the alias was taken before sealing and already points at
// the same backing array that SealSlice clips a *view* of; clipping the
// returned header does not revoke writes through `leaked`. This is why the
// real remedy (R2, modeled in checker/seal_checker.py) must be a *static*
// rejection of `seal move x` whenever x's storage_component has a live
// alias/may_share entry — it cannot be recovered at the runtime-shape layer
// alone.
func TestF2_PreSealSubsliceAliasSurvivesSealAndStillMutates(t *testing.T) {
	buf := make([]int, 4, 4)
	buf[0] = 42

	leaked := buf[:1] // alias taken BEFORE the seal — this is what R2 must reject statically.
	sealed := SealSlice(buf)

	if got := sealed.At(0); got != 42 {
		t.Fatalf("precondition broken: expected sealed.At(0)==42 before mutation, got %d", got)
	}

	leaked[0] = 7777 // "read-only" sealed.At(0) observes a write it never performed.

	if got := sealed.At(0); got != 7777 {
		t.Fatalf("F2 did not fire as predicted: expected the pre-seal alias to still mutate sealed data (got %d, want 7777) — if this fails, the runtime shape alone is (incorrectly) sufficient and R2's static alias-freeze would be unnecessary", got)
	}
}

// F3 (declared falsifier, FIRES AS PREDICTED): a struct whose interior is a
// closure over mutable state is NOT read-only just because the outer value
// was handed to `sealed[T]` by naive field copy — calling the closure can
// still mutate the captured state.
func TestF3_InteriorClosureLaundersMutationThroughNaiveSeal(t *testing.T) {
	counter := 0
	cfg := struct {
		NextID func() int
	}{
		NextID: func() int {
			counter++
			return counter
		},
	}

	naivelySealed := cfg // naive "seal" = plain copy; the closure's captured `counter` is still shared.
	_ = naivelySealed.NextID()
	_ = naivelySealed.NextID()

	if counter != 2 {
		t.Fatalf("F3 did not fire as predicted: expected calling the interior closure through the naively-sealed copy to mutate captured state (counter=%d, want 2)", counter)
	}
}

// F3 remedy R3: IsSealableType / SealValue reject any type whose
// representation recursively contains a func/chan/interface/pointer field,
// so the construct in the test above is never admitted into sealed[T] in
// the first place.
func TestF3Remedy_SealValueRejectsInteriorClosure(t *testing.T) {
	cfg := struct {
		NextID func() int
	}{NextID: func() int { return 1 }}

	if _, err := SealValue(cfg); err != ErrNotSealable {
		t.Fatalf("expected SealValue to reject an interior closure with ErrNotSealable, got %v", err)
	}

	type PlainConfig struct {
		Name    string
		Weights []int
		Routes  map[string]int
	}
	plain := PlainConfig{Name: "x", Weights: []int{1, 2, 3}, Routes: map[string]int{"/a": 1}}
	sealedPlain, err := SealValue(plain)
	if err != nil {
		t.Fatalf("expected a plain data-only struct to be sealable, got error: %v", err)
	}
	if sealedPlain.Name != "x" {
		t.Fatalf("SealValue must preserve the value, got %+v", sealedPlain)
	}
}

// P3 (GUARANTEES.md Rows 5 & 9 flip, runtime half): a sealed capture handed
// to a foreign callback boundary has zero write paths — the capture can be
// read concurrently from many invocations with no race. (The admission-side
// half — rejecting a *non*-const, *non*-sealed capture at a foreign-call
// boundary with HYB-E3013 — is modeled in checker/seal_checker.py, since
// that is a static check, not a runtime one.)
func TestP3_SealedForeignCallbackCaptureHasNoWritePath(t *testing.T) {
	routes := map[string]int{"/x": 200, "/admin": 403}
	sealedRoutes := SealMap(routes)

	handler := func() int {
		v, _ := sealedRoutes.Get("/admin")
		return v
	}

	var wg sync.WaitGroup
	for i := 0; i < 12; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for j := 0; j < 2000; j++ {
				if got := handler(); got != 403 {
					t.Errorf("handler observed %d, want 403", got)
					return
				}
			}
		}()
	}
	wg.Wait()
}

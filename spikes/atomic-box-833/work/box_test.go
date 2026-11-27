package boxspike

import (
	"os"
	"sync"
	"sync/atomic"
	"testing"
)

type buffer struct {
	id        int
	data      []byte
	version   byte
	mutations int
}

// Each worker has exactly one private buffer; one more is in the cell. Only
// Swap transfers access. Check payload integrity and the final ownership census.
func TestBufferExchangeMillion(t *testing.T) {
	const workers, perWorker = 32, 31250
	var cell Box[buffer]
	cell.Swap(&buffer{id: workers, data: make([]byte, 64)})
	finished := make(chan *buffer, workers+1)
	var corrupted atomic.Bool
	var wg sync.WaitGroup
	for worker := 0; worker < workers; worker++ {
		wg.Add(1)
		go func(id int) {
			defer wg.Done()
			owned := &buffer{id: id, data: make([]byte, 64)}
			for step := 0; step < perWorker; step++ {
				for _, value := range owned.data {
					if value != owned.version {
						corrupted.Store(true)
					}
				}
				// Sixteen producers write; sixteen consumers verify/read. Both
				// mutate their private accounting field after acquisition.
				if id < workers/2 {
					owned.version++
					for i := range owned.data {
						owned.data[i] = owned.version
					}
				}
				owned.mutations++
				owned = cell.Swap(owned)
			}
			finished <- owned
		}(worker)
	}
	wg.Wait()
	finished <- cell.Swap(nil)
	close(finished)
	seen, total := make(map[int]bool), 0
	for owned := range finished {
		if owned == nil || owned.id < 0 || owned.id > workers || seen[owned.id] {
			t.Fatalf("lost or duplicate ownership: %v", owned)
		}
		seen[owned.id] = true
		total += owned.mutations
		for _, value := range owned.data {
			if value != owned.version {
				corrupted.Store(true)
			}
		}
	}
	if corrupted.Load() || len(seen) != workers+1 || total != workers*perWorker {
		t.Fatalf("corrupted=%v buffers=%d mutations=%d", corrupted.Load(), len(seen), total)
	}
	t.Logf("16 producers + 16 consumers: %d exchanges; %d distinct final owners", total, len(seen))
}

func TestCASOwnershipOutcomes(t *testing.T) {
	var cell Box[map[string]int]
	initial, replacement, wrong := map[string]int{"count": 1}, map[string]int{}, map[string]int{}
	if cell.Swap(&initial) != nil {
		t.Fatal("nonempty zero value")
	}
	if ok, acquired := cell.CompareAndSwap(&wrong, &replacement); ok || acquired != nil {
		t.Fatal("failed CAS acquired a pointee")
	}
	replacement["count"] = 2 // failure retained ownership
	if ok, acquired := cell.CompareAndSwap(&initial, &replacement); !ok || acquired != &initial {
		t.Fatal("successful CAS did not return the displaced identity")
	} else {
		(*acquired)["count"]++
	}
	if old := cell.Swap(nil); old != &replacement || (*old)["count"] != 2 {
		t.Fatal("replacement lost or damaged")
	}
	if ok, acquired := cell.CompareAndSwap(nil, nil); !ok || acquired != nil {
		t.Fatal("empty CAS result")
	}
}

func TestCASOnlyWinnerAcquires(t *testing.T) {
	const workers = 64
	var cell Box[map[string]int]
	initial := map[string]int{}
	expected := &initial // retain identity only, never access until successful CAS
	cell.Swap(expected)
	start := make(chan struct{})
	var wins atomic.Int32
	var wg sync.WaitGroup
	for i := 0; i < workers; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			next := map[string]int{}
			<-start
			ok, acquired := cell.CompareAndSwap(expected, &next)
			if ok {
				wins.Add(1)
				(*acquired)["winner"]++
			} else if acquired != nil {
				t.Error("loser acquired a pointer")
			} else {
				next["retained"]++
			}
		}()
	}
	close(start)
	wg.Wait()
	if wins.Load() != 1 || initial["winner"] != 1 {
		t.Fatalf("CAS wins=%d, acquired mutations=%d", wins.Load(), initial["winner"])
	}
	cell.Swap(nil)
}

func TestSelfExchangeRejected(t *testing.T) {
	for _, method := range []string{"swap", "cas"} {
		t.Run(method, func(t *testing.T) {
			var cell Box[int]
			value := 1
			cell.Swap(&value)
			defer func() {
				if recover() == nil {
					t.Fatal("self-exchange returned a pointer still in the box")
				}
			}()
			if method == "swap" {
				cell.Swap(&value)
			} else {
				cell.CompareAndSwap(&value, &value)
			}
		})
	}
}

type node struct {
	Payload map[string]int
	Next    *node
}

// This is a deliberately invalid ownership input to the raw Go wrapper. A
// standard Treiber pop needs next to refer into the graph already in the cell.
// It therefore cannot satisfy the proposed requirement of an exclusively owned
// replacement graph. No Hybrid acceptance or compiler counterexample is claimed.
func TestTreiberPopOverlapsReplacement(t *testing.T) {
	tail := &node{Payload: map[string]int{}}
	head := &node{Payload: map[string]int{}, Next: tail}
	var cell Box[node]
	cell.Swap(head)
	// Deriving this next from a shared head would itself need the forbidden Load
	// and a read of Next. Here the initial graph makes the overlap explicit.
	ok, acquired := cell.CompareAndSwap(head, tail)
	remaining := cell.Swap(nil)
	if !ok || acquired != head || acquired.Next != remaining {
		t.Fatal("two-node Treiber graph witness changed")
	}
	t.Log("returned head.Next aliases the published tail; exclusive input precondition was violated")
}

type stackState struct{ head *node }

func tryPop(cell *Box[stackState]) (*node, bool) {
	owned := cell.Swap(nil)
	if owned == nil {
		return nil, false
	}
	result := owned.head
	if result != nil {
		owned.head = result.Next
		result.Next = nil
	}
	cell.Swap(owned)
	return result, true
}

func TestTakeFallbackHasNonProgressCycle(t *testing.T) {
	var cell Box[stackState]
	cell.Swap(&stackState{head: &node{Payload: map[string]int{}}})
	owned := cell.Swap(nil) // task A is suspended here
	// Run a real peer, with no sleeps or dependence on scheduler fairness.
	done := make(chan int)
	go func() {
		completed := 0
		for attempt := 0; attempt < 10000; attempt++ {
			if _, ok := tryPop(&cell); ok {
				completed++
			}
		}
		done <- completed
	}()
	if completed := <-done; completed != 0 {
		t.Fatalf("peer completed %d operations while sole owner was suspended", completed)
	}
	if cell.Swap(nil) != nil {
		t.Fatal("retry cycle changed the empty cell state")
	}
	cell.Swap(owned) // task A resumes
	if result, ok := tryPop(&cell); !ok || result == nil {
		t.Fatal("fallback did not recover after owner resumed")
	}
	t.Log("10000 peer steps, 0 completions; same nil state on every step; completion requires owner resumption")
}

// Opt-in, intentionally raceful negative control. This is not in the no-Load
// subset. The harness must see a real race report, not treat arbitrary failure
// as a passed falsifier. Read and write remain concurrent after close(start).
func TestUncheckedTreiberLoadRace(t *testing.T) {
	if os.Getenv("HYB_ATOMIC_BOX_NEGATIVE") != "1" {
		t.Skip("intentional race: run separately with HYB_ATOMIC_BOX_NEGATIVE=1")
	}
	tail := &node{Payload: map[string]int{}}
	head := &node{Payload: map[string]int{}, Next: tail}
	var cell Box[node]
	cell.Swap(head)
	loaded, start := make(chan struct{}), make(chan struct{})
	finished := make(chan int)
	go func() {
		observed := cell.p.Load() // deliberately restore the forbidden raw Load
		close(loaded)
		<-start
		observedLinks := 0
		for i := 0; i < 1000000; i++ {
			if observed.Next != nil {
				observedLinks++
			}
		}
		finished <- observedLinks
	}()
	<-loaded
	ok, acquired := cell.CompareAndSwap(head, tail)
	if !ok {
		t.Fatal("controlled pop failed")
	}
	close(start)
	for i := 0; i < 1000000; i++ {
		acquired.Next = nil
	}
	t.Logf("unchecked observations of a live link: %d", <-finished)
}

var benchmarkResult *int

func BenchmarkSwap(b *testing.B) {
	var cell Box[int]
	left, right := 1, 2
	cell.Swap(&left)
	owned := &right
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		owned = cell.Swap(owned)
	}
	benchmarkResult = owned
}

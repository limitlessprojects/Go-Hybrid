package hyregion_test

import (
	"sync"
	"sync/atomic"
	"testing"
	"unsafe"
)

const (
	SlabHeaderSize       = 64
	DefaultSlabSize      = 256 << 10 // 256 KiB
	DefaultGuardPageSize = 4096
)

type SlabHandle struct {
	SlabID uint32
	Epoch  uint32
}

type HyRegionSlab struct {
	slabID    uint32
	epoch     atomic.Uint32
	bumpOff   uint32
	spFloor   uint32
	highWater uint32
	backing   []byte
}

func NewHyRegionSlab(id uint32) *HyRegionSlab {
	s := &HyRegionSlab{
		slabID:    id,
		bumpOff:   SlabHeaderSize,
		spFloor:   DefaultSlabSize,
		highWater: SlabHeaderSize,
		backing:   make([]byte, DefaultSlabSize),
	}
	s.epoch.Store(1)
	return s
}

func (s *HyRegionSlab) Handle() SlabHandle {
	return SlabHandle{SlabID: s.slabID, Epoch: s.epoch.Load()}
}

func (s *HyRegionSlab) BumpWriteBytes(h SlabHandle, data []byte) (uint32, bool) {
	if h.SlabID != s.slabID || h.Epoch != s.epoch.Load() {
		return 0, false
	}
	aligned := (s.bumpOff + 7) &^ 7
	next := aligned + uint32(len(data))
	if next+DefaultGuardPageSize > s.spFloor {
		return 0, false
	}
	copy(s.backing[aligned:next], data)
	s.bumpOff = next
	if next > s.highWater {
		s.highWater = next
	}
	return aligned, true
}

func (s *HyRegionSlab) DirectSlice(h SlabHandle, off, length uint32) ([]byte, bool) {
	if h.SlabID != s.slabID || h.Epoch != s.epoch.Load() {
		return nil, false
	}
	ptr := unsafe.Add(unsafe.Pointer(&s.backing[0]), off)
	return unsafe.Slice((*byte)(ptr), length), true
}

func (s *HyRegionSlab) TransferDetach(h SlabHandle) (SlabHandle, bool) {
	if h.SlabID != s.slabID {
		return SlabHandle{}, false
	}
	if !s.epoch.CompareAndSwap(h.Epoch, h.Epoch+1) {
		return SlabHandle{}, false
	}
	return SlabHandle{SlabID: s.slabID, Epoch: h.Epoch + 1}, true
}

func (s *HyRegionSlab) Release(h SlabHandle) bool {
	if !s.epoch.CompareAndSwap(h.Epoch, h.Epoch+1) {
		return false
	}
	s.bumpOff = SlabHeaderSize
	s.spFloor = DefaultSlabSize
	return true
}

func TestRaceFreeRegionTransferAcrossTasks(t *testing.T) {
	slab := NewHyRegionSlab(1)
	h0 := slab.Handle()
	off, ok := slab.BumpWriteBytes(h0, []byte("hybrid-zero-copy-payload"))
	if !ok {
		t.Fatal("bump write failed")
	}

	ch := make(chan SlabHandle, 1)
	h1, ok := slab.TransferDetach(h0)
	if !ok {
		t.Fatal("transfer detach failed")
	}
	// Stale handle h0 is immediately rejected
	if _, ok := slab.DirectSlice(h0, off, 24); ok {
		t.Fatal("stale sender handle must be rejected after TransferDetach")
	}
	ch <- h1

	var wg sync.WaitGroup
	wg.Add(1)
	go func() {
		defer wg.Done()
		rh := <-ch
		view, ok := slab.DirectSlice(rh, off, 24)
		if !ok || string(view) != "hybrid-zero-copy-payload" {
			t.Errorf("unexpected view: %v %q", ok, string(view))
		}
		view[0] = 'H'
		slab.Release(rh)
	}()
	wg.Wait()
}

type mutexHandleTable struct {
	mu    sync.Mutex
	slots []string
}

func (t *mutexHandleTable) allocAndRead(payload string) string {
	t.mu.Lock()
	id := len(t.slots)
	t.slots = append(t.slots, payload)
	t.mu.Unlock()

	t.mu.Lock()
	out := string([]byte(t.slots[id]))
	t.slots[id] = ""
	t.mu.Unlock()
	return out
}

func BenchmarkHandleTableMutexCopy(b *testing.B) {
	ht := &mutexHandleTable{slots: make([]string, 0, 1024)}
	payload := "hybrid-json-field-value-benchmark-payload"
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		_ = ht.allocAndRead(payload)
		if len(ht.slots) > 512 {
			ht.slots = ht.slots[:0]
		}
	}
}

func BenchmarkHyRegionBumpAndDirectSlice(b *testing.B) {
	slab := NewHyRegionSlab(1)
	payload := []byte("hybrid-json-field-value-benchmark-payload")
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		h := slab.Handle()
		off, _ := slab.BumpWriteBytes(h, payload)
		view, _ := slab.DirectSlice(h, off, uint32(len(payload)))
		if view[0] != 'h' {
			b.Fatal("unexpected byte")
		}
		slab.Release(h)
	}
}

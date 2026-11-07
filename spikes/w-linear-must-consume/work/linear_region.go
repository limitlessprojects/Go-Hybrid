package linearregion

import (
	"fmt"
	"sync"
	"sync/atomic"
)

// Packet represents a fixed-capacity buffer processed per task iteration.
type Packet struct {
	Slots [32]int64
	Len   int
}

// Reset zeroes the active slice prefix for deterministic reuse at loop back-edges.
func (p *Packet) Reset() {
	for i := 0; i < p.Len; i++ {
		p.Slots[i] = 0
	}
	p.Len = 0
}

// LinearHandle models an own-handle with an exactly-once must-consume obligation (Row 13).
type LinearHandle struct {
	id       uint64
	consumed uint32
	payload  int64
}

func AcquireHandle(id uint64, payload int64) LinearHandle {
	return LinearHandle{id: id, payload: payload}
}

func (h *LinearHandle) ConsumeClose() (int64, error) {
	if !atomic.CompareAndSwapUint32(&h.consumed, 0, 1) {
		return 0, fmt.Errorf("HYB-E3003: double consume on handle %d", h.id)
	}
	return h.payload, nil
}

func (h *LinearHandle) IsConsumed() bool {
	return atomic.LoadUint32(&h.consumed) == 1
}

// RunLinearStackLoop executes N iterations using an escape-certified stack Packet
// reset at each loop back-edge (0 heap allocations, 0 sync.Pool atomic/interface overhead).
func RunLinearStackLoop(iterations int) int64 {
	var pkt Packet
	var acc int64
	for i := 0; i < iterations; i++ {
		pkt.Reset()
		pkt.Len = 8
		for j := 0; j < pkt.Len; j++ {
			pkt.Slots[j] = int64(i + j)
			acc += pkt.Slots[j]
		}
	}
	return acc
}

var packetPool = sync.Pool{
	New: func() any {
		return new(Packet)
	},
}

// RunSyncPoolLoop executes the same N iterations using Go's sync.Pool (Row 10 baseline).
func RunSyncPoolLoop(iterations int) int64 {
	var acc int64
	for i := 0; i < iterations; i++ {
		pkt := packetPool.Get().(*Packet)
		pkt.Reset()
		pkt.Len = 8
		for j := 0; j < pkt.Len; j++ {
			pkt.Slots[j] = int64(i + j)
			acc += pkt.Slots[j]
		}
		packetPool.Put(pkt)
	}
	return acc
}

// FalsifierF2InterfaceSink demonstrates the exact boundary where Go's cmd/compile
// escape analysis forces a heap allocation even when a value is logically loop-local:
// passing &pkt through a dynamic interface method or fmt.Sprint sink escapes to heap.
type FormatterSink interface {
	 Observe(v any) int
}

func RunFalsifierF2InterfaceEscape(iterations int, sink FormatterSink) int {
	var pkt Packet
	total := 0
	for i := 0; i < iterations; i++ {
		pkt.Reset()
		pkt.Slots[0] = int64(i)
		pkt.Len = 1
		total += sink.Observe(&pkt)
	}
	return total
}

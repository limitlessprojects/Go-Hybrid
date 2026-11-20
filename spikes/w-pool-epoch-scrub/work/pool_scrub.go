package poolscrub

import "sync"

// Packet models a composite struct in Hybrid containing:
// - Scalar fields (ScalarID)
// - Capacity-retainable interior slice (Payload []int)
// - Capacity-retainable interior map (Meta map[string]int)
// - Non-resettable pointer/reference slot (ForeignPtr *int)
type Packet struct {
	ScalarID   int
	Payload    []int
	Meta       map[string]int
	ForeignPtr *int
}

// __hy_scrub_Packet is the compiler-synthesized O4.11b Epoch Scrubbing Thunk
// emitted by hy-emit immediately prior to returning a moved Packet to sync.Pool.
//
// Guarantees:
// 1. Primitive fields are zeroed.
// 2. Exclusively owned slices are reset to len=0 while preserving backing capacity
//    via 3-index clipping: p.Payload = p.Payload[:0:cap(p.Payload)] (0 allocs/op).
// 3. Exclusively owned maps are cleared via Go's builtin clear() (0 allocs/op).
// 4. Interior pointer/channel/closure slots are hard-zeroed to nil, severing
//    cross-epoch sub-graph aliasing by construction.
func __hy_scrub_Packet(p *Packet) {
	p.ScalarID = 0
	if len(p.Payload) > 0 {
		clear(p.Payload)
		p.Payload = p.Payload[:0:cap(p.Payload)]
	}
	if len(p.Meta) > 0 {
		clear(p.Meta)
	}
	p.ForeignPtr = nil
}

// TypedPoolPacket represents the pointer-backed lowering of Hybrid's curated
// `sync.Pool[Packet]`. Storing *Packet inside sync.Pool avoids interface-boxing
// heap allocations on Put/Get (Falsifier F2: 0 B/op, 0 allocs/op).
type TypedPoolPacket struct {
	raw sync.Pool
}

// NewTypedPoolPacket constructs a curated pool with pre-sized interior buffers.
func NewTypedPoolPacket(initialSliceCap int) *TypedPoolPacket {
	return &TypedPoolPacket{
		raw: sync.Pool{
			New: func() any {
				return &Packet{
					Payload: make([]int, 0, initialSliceCap),
					Meta:    make(map[string]int, 8),
				}
			},
		},
	}
}

// Get acquires an isolated Packet from the pool.
func (tp *TypedPoolPacket) Get() *Packet {
	return tp.raw.Get().(*Packet)
}

// PutScrubbed lowers `pool.Put(move pkt)` under Rule O4.11b:
// runs the compiler-synthesized scrubber before returning the pointer to sync.Pool.
func (tp *TypedPoolPacket) PutScrubbed(p *Packet) {
	if p == nil {
		return
	}
	__hy_scrub_Packet(p)
	tp.raw.Put(p)
}

// PutUnscrubbed models naive outer-only linear pooling (W-022 / W-024 without O4.11b)
// where the outer handle is consumed, but interior slices/maps/pointers retain dirty state.
func (tp *TypedPoolPacket) PutUnscrubbed(p *Packet) {
	if p == nil {
		return
	}
	tp.raw.Put(p)
}

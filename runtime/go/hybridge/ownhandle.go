package hybridge

import (
	"errors"
	"sync"
)

var (
	ErrArenaOOM       = errors.New("ownhandle: arena allocation exceeds capacity")
	ErrInvalidPointer = errors.New("ownhandle: invalid or out-of-bounds pointer access")
)

const ReservedNullHeader = 64 // Wasm address 0 is NULL; offsets < 64 are reserved

type OwnHandle struct {
	mem []byte
	off int
}

var handlePool = sync.Pool{
	New: func() any {
		return &OwnHandle{
			mem: make([]byte, 65536),
			off: ReservedNullHeader,
		}
	},
}

func GetOwnHandle(capacity int) *OwnHandle {
	h := handlePool.Get().(*OwnHandle)
	if capacity > len(h.mem) {
		h.mem = make([]byte, capacity)
	}
	h.off = ReservedNullHeader
	return h
}

func ReleaseOwnHandle(h *OwnHandle) {
	if h != nil {
		h.Reset()
		handlePool.Put(h)
	}
}

func NewOwnHandle(initialCap int) *OwnHandle {
	if initialCap <= ReservedNullHeader {
		initialCap = 65536
	}
	return &OwnHandle{
		mem: make([]byte, initialCap),
		off: ReservedNullHeader,
	}
}

func (h *OwnHandle) Alloc(size int) (uint32, []byte, error) {
	if size < 0 {
		return 0, nil, ErrArenaOOM
	}
	if h.off+size > len(h.mem) {
		newCap := len(h.mem) * 2
		if newCap < h.off+size {
			newCap = h.off + size + 65536
		}
		newMem := make([]byte, newCap)
		copy(newMem, h.mem[:h.off])
		h.mem = newMem
	}
	ptr := uint32(h.off)
	slice := h.mem[h.off : h.off+size]
	h.off += size
	return ptr, slice, nil
}

func (h *OwnHandle) SubSlice(ptr uint32, length int) ([]byte, error) {
	uPtr := int(ptr)
	if uPtr < ReservedNullHeader || length < 0 || uPtr+length > len(h.mem) {
		return nil, ErrInvalidPointer
	}
	return h.mem[uPtr : uPtr+length], nil
}

func (h *OwnHandle) Memory() []byte {
	return h.mem
}

func (h *OwnHandle) Reset() {
	h.off = ReservedNullHeader
}

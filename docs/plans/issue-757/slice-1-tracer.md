# Slice 1 Tracer Bullet - Architecture & Call-Stack Trace Design

## 1. Six-Tier Call-Stack Trace Tree

Data path: `Go caller → OwnHandle arena alloc → wasm2go transpiled export → result back to Go`

```
Project: Go-Hybrid
├── Folder: runtime/go/sha3binding
│   └── File: sha3.go
│       └── Struct: DigestService
│           └── Method: Digest256(data []byte) ([32]byte, error)
│               └── Variable State: data_len=64, arena_offset_before=0
├── Folder: runtime/go/hybridge
│   └── File: ownhandle.go
│       └── Struct: OwnHandle
│           └── Method: Alloc(size int) (ptr uint32, buf []byte, err error)
│               └── Variable State: mem_cap=65536, req_size=64, new_offset=64, ptr=0x1000
├── Folder: runtime/go/hybridge/wasm
│   └── File: hyrt_wasm.go
│       └── Struct: WasmModule
│           └── Method: HySha3Digest256(dataPtr uint32, dataLen uint32, outPtr uint32) int32
│               └── Variable State: mem[dataPtr..dataPtr+dataLen] read, mem[outPtr..outPtr+32] written, status=0
├── Folder: runtime/go/hybridge
│   └── File: ownhandle.go
│       └── Struct: OwnHandle
│           └── Method: Reset()
│               └── Variable State: arena_offset_after=0, retained_capacity=65536
└── Folder: runtime/go/sha3binding
    └── File: sha3.go
        └── Struct: DigestService
            └── Method: Digest256(data []byte) ([32]byte, error)
                └── Variable State: return_digest=[32]byte{...}, err=nil
```

---

## 2. OwnHandle Interface Designs (Deep Module Principle)

### Design A: Explicit Multi-Call Arena Handle (Shallow Surface)
```go
type Arena struct {
    mem []byte
    off int
}

type SliceHandle struct {
    Ptr uint32
    Len int
}

type OwnHandle struct {
    arena *Arena
}

func NewOwnHandle(capacity int) *OwnHandle
func (h *OwnHandle) AllocSlice(size int) (SliceHandle, []byte, error)
func (h *OwnHandle) Borrow(sh SliceHandle) []byte
func (h *OwnHandle) Consume()
```
*Critique*: Exposes inner types (`Arena`, `SliceHandle`) to the caller, forcing the caller to keep track of pointers, lengths, and arena states across multiple calls. Deeply leaks implementation details.

### Design B: Unified Encapsulated Arena Handle (Deep Module - Selected)
```go
package hybridge

type OwnHandle struct {
    mem []byte
    off int
}

func NewOwnHandle(initialCap int) *OwnHandle
func (h *OwnHandle) Alloc(size int) (ptr uint32, buf []byte, err error)
func (h *OwnHandle) SubSlice(ptr uint32, len int) ([]byte, error)
func (h *OwnHandle) Memory() []byte
func (h *OwnHandle) Reset()
```
*Rationale for Selecting Design B*:
- **Simpler Caller Surface**: The caller only interacts with `OwnHandle`.
- **Zero Allocations & Zero Copy**: `Alloc` returns a sub-slice directly pointing into `h.mem`, so inputs can be copied directly into Wasm linear memory. `SubSlice` returns zero-copy slice views over output data.
- **O(1) Arena Reset**: `Reset()` simply sets `h.off = 0`, keeping the underlying `[]byte` slice allocated for recycling via `sync.Pool`.

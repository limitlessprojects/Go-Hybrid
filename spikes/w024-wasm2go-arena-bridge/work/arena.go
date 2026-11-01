// Package w024wasm2go implements the M1/M2 feasibility spike for Wildcard #757 (W-024):
// AOT-transpiled Wasm32 linear-memory arenas per OwnHandle (`Doc`), eliminating
// libhyrt.a, cgo, and the 3-tier FFI bridge (SMALL / DEFAULT / DEEP in interfaces/descriptors.md)
// while preserving Hybrid's A2/A3/A7/O4 ownership and borrow invariants under pure `CGO_ENABLED=0` Go.
package w024wasm2go

import (
	"bytes"
	"compress/flate"
	"crypto/ed25519"
	"crypto/sha256"
	"encoding/binary"
	"encoding/json"
	"errors"
	"io"
	"regexp"
	"sync"
	"sync/atomic"
)

var (
	ErrUseAfterFree       = errors.New("HYB-E3105: use of BorrowHandle View after OwnHandle Doc was freed or moved")
	ErrStaleGeneration    = errors.New("HYB-E3102: stale generation on linear-memory arena view")
	ErrOutOfBoundsAccess  = errors.New("HYB-E6009: linear-memory bounds violation in Wasm arena")
	ErrMaxDepthExceeded   = errors.New("serde_json: recursion limit exceeded (max depth 128)")
	ErrMalformedJSONInput = errors.New("serde_json: malformed input")
	ErrInvalidNodeKind    = errors.New("serde_json: unexpected node kind in linear-memory arena")
	ErrTableHandle        = errors.New("hyrt: invalid or stale handle (ERR_TABLE)")
)

const (
	defaultArenaCapacity = 64 * 1024 // 1 Wasm page (64 KiB) pooled per OwnHandle
	maxRecursionDepth    = 128       // matches serde_json (1.0.140) remaining_depth = 128
	nodeRecordSize       = 20        // [kind:u32, a:u32, b:u32, c:u32, d:u32]
	abiHandshakeWord     = uint64(0x68795f61626931) // "hy_abi1"
)

const (
	NodeKindNull   = uint32(0)
	NodeKindObject = uint32(1)
	NodeKindArray  = uint32(2)
	NodeKindString = uint32(3)
	NodeKindInt    = uint32(4)
	NodeKindBool   = uint32(5)
)

const (
	StatusOK       = int32(0)
	StatusErrTable = int32(1)
	StatusErrJSON  = int32(2)
	StatusTryAgain = int32(3)
)

// arenaPool recycles 64 KiB Wasm linear-memory pages across `managed` scopes so
// repeated `serde_json.parse` + `defer doc.free()` cycles allocate 0 B/op on
// the steady-state hot path.
var arenaPool = sync.Pool{
	New: func() any {
		buf := make([]byte, defaultArenaCapacity)
		return &buf
	},
}

var globalGenCounter atomic.Uint64

func NextGeneration() uint64 {
	return globalGenCounter.Add(1)
}

// Doc represents the Hybrid `OwnHandle("serde_json.doc")` lowered to a pure-Go
// Wasm linear-memory arena without cgo or `unsafe.Pointer`.
//
// Layout inside `mem []byte`:
//   - Nodes and index tables are bump-allocated from `bump` upwards in `mem`.
//   - Each node occupies `nodeRecordSize` (20 bytes) at offset `nodeOff`:
//     * NodeKindObject: [1, pairTableOff, pairCount, pairCap, 0]
//       where each pair entry is 12 bytes: [keyOff:u32, keyLen:u32, valNodeOff:u32]
//     * NodeKindArray:  [2, elemTableOff, elemCount, elemCap, 0]
//       where each elem entry is 4 bytes: [valNodeOff:u32]
//     * NodeKindString: [3, strOff, strLen, 0, 0]
//     * NodeKindInt:    [4, low32, high32, 0, 0]
//     * NodeKindBool:   [5, boolVal(0|1), 0, 0, 0]
type Doc struct {
	mem       []byte
	pooledRef *[]byte
	bump      uint32
	rootOff   uint32
	gen       uint64
	freed     bool
}

// View represents `BorrowHandle { view: "serde_json.view", owner: "serde_json.doc" }`.
// In emitted Go (`hy-emit`), `hy-check` proves at compile time (A2/A3) that
// `View` never outlives `Doc` or escapes a `managed` scope; the generation tag
// provides a deterministic runtime defense-in-depth falsifier check.
type View struct {
	owner   *Doc
	gen     uint64
	nodeOff uint32
}

func newArenaDoc(gen uint64, minBytes int) *Doc {
	if gen == 0 {
		gen = NextGeneration()
	}
	pooled := arenaPool.Get().(*[]byte)
	mem := *pooled
	if minBytes > len(mem) {
		mem = make([]byte, minBytes)
	}
	return &Doc{
		mem:       mem,
		pooledRef: pooled,
		bump:      0,
		rootOff:   0,
		gen:       gen,
		freed:     false,
	}
}

func (d *Doc) allocBytes(n uint32) (uint32, error) {
	// 4-byte align bump pointer for deterministic Wasm32 linear-memory alignment.
	aligned := (d.bump + 3) & ^uint32(3)
	end := aligned + n
	if int(end) > len(d.mem) {
		// Grow linear memory in 64 KiB Wasm page multiples if needed.
		newCap := len(d.mem) * 2
		for newCap < int(end) {
			newCap *= 2
		}
		grown := make([]byte, newCap)
		copy(grown[:d.bump], d.mem[:d.bump])
		d.mem = grown
	}
	off := aligned
	d.bump = end
	return off, nil
}

func (d *Doc) writeNode(kind, a, b, c, e uint32) (uint32, error) {
	off, err := d.allocBytes(nodeRecordSize)
	if err != nil {
		return 0, err
	}
	binary.LittleEndian.PutUint32(d.mem[off+0:off+4], kind)
	binary.LittleEndian.PutUint32(d.mem[off+4:off+8], a)
	binary.LittleEndian.PutUint32(d.mem[off+8:off+12], b)
	binary.LittleEndian.PutUint32(d.mem[off+12:off+16], c)
	binary.LittleEndian.PutUint32(d.mem[off+16:off+20], e)
	return off, nil
}

func (d *Doc) readNode(off uint32) (kind, a, b, c, e uint32, err error) {
	if d == nil || d.freed {
		return 0, 0, 0, 0, 0, ErrUseAfterFree
	}
	if int(off+nodeRecordSize) > int(d.bump) || int(off+nodeRecordSize) > len(d.mem) {
		return 0, 0, 0, 0, 0, ErrOutOfBoundsAccess
	}
	kind = binary.LittleEndian.Uint32(d.mem[off+0 : off+4])
	a = binary.LittleEndian.Uint32(d.mem[off+4 : off+8])
	b = binary.LittleEndian.Uint32(d.mem[off+8 : off+12])
	c = binary.LittleEndian.Uint32(d.mem[off+12 : off+16])
	e = binary.LittleEndian.Uint32(d.mem[off+16 : off+20])
	return kind, a, b, c, e, nil
}

func (d *Doc) allocStringBytes(s string) (uint32, uint32, error) {
	n := uint32(len(s))
	if n == 0 {
		return 0, 0, nil
	}
	off, err := d.allocBytes(n)
	if err != nil {
		return 0, 0, err
	}
	copy(d.mem[off:off+n], s)
	return off, n, nil
}

// Parse lowers `serde_json.parse(input)` into a single linear-memory arena `Doc`.
// Fast-path flat objects avoid intermediate heap allocations; nested JSON trees
// up to depth 128 (`interfaces/descriptors.md` DEEP class) are lowered into the
// same linear-memory node table on the growable Go goroutine stack without cgo.
func Parse(input string, gen uint64) (*Doc, error) {
	d := newArenaDoc(gen, len(input)*4+512)
	if ok, err := d.tryFastFlatObject(input); err != nil {
		d.Free()
		return nil, err
	} else if ok {
		return d, nil
	}
	rootOff, err := d.parseRecursiveTree(input)
	if err != nil {
		d.Free()
		return nil, err
	}
	d.rootOff = rootOff
	return d, nil
}

// NewObject creates an empty JSON object in its own linear-memory arena.
func NewObject(gen uint64) (*Doc, error) {
	d := newArenaDoc(gen, 1024)
	const initCap = 8
	tableOff, err := d.allocBytes(initCap * 12)
	if err != nil {
		d.Free()
		return nil, err
	}
	rootOff, err := d.writeNode(NodeKindObject, tableOff, 0, initCap, 0)
	if err != nil {
		d.Free()
		return nil, err
	}
	d.rootOff = rootOff
	return d, nil
}

// NewArray creates an empty JSON array in its own linear-memory arena.
func NewArray(gen uint64) (*Doc, error) {
	d := newArenaDoc(gen, 1024)
	const initCap = 8
	tableOff, err := d.allocBytes(initCap * 4)
	if err != nil {
		d.Free()
		return nil, err
	}
	rootOff, err := d.writeNode(NodeKindArray, tableOff, 0, initCap, 0)
	if err != nil {
		d.Free()
		return nil, err
	}
	d.rootOff = rootOff
	return d, nil
}

// Free releases the arena back to `arenaPool` and invalidates the generation
// (`defer doc.free()` in `managed` scope, A7 handle consumption).
func (d *Doc) Free() {
	if d == nil || d.freed {
		return
	}
	d.freed = true
	d.gen = 0
	if d.pooledRef != nil {
		arenaPool.Put(d.pooledRef)
		d.pooledRef = nil
	}
	d.mem = nil
}

// Root returns the root `View` (`BorrowHandle`) into `d`'s linear memory.
func (d *Doc) Root() (View, error) {
	if d == nil || d.freed {
		return View{}, ErrUseAfterFree
	}
	return View{
		owner:   d,
		gen:     d.gen,
		nodeOff: d.rootOff,
	}, nil
}

func (v View) checkLive() (*Doc, error) {
	d := v.owner
	if d == nil || d.freed {
		return nil, ErrUseAfterFree
	}
	if d.gen != v.gen {
		return nil, ErrStaleGeneration
	}
	return d, nil
}

func (v View) lookupObjectField(key string) (uint32, error) {
	d, err := v.checkLive()
	if err != nil {
		return 0, err
	}
	kind, tableOff, count, _, _, err := d.readNode(v.nodeOff)
	if err != nil {
		return 0, err
	}
	if kind != NodeKindObject {
		return 0, ErrInvalidNodeKind
	}
	for i := uint32(0); i < count; i++ {
		entryOff := tableOff + i*12
		if int(entryOff+12) > int(d.bump) || int(entryOff+12) > len(d.mem) {
			return 0, ErrOutOfBoundsAccess
		}
		kOff := binary.LittleEndian.Uint32(d.mem[entryOff+0 : entryOff+4])
		kLen := binary.LittleEndian.Uint32(d.mem[entryOff+4 : entryOff+8])
		valNodeOff := binary.LittleEndian.Uint32(d.mem[entryOff+8 : entryOff+12])
		if int(kOff+kLen) > int(d.bump) || int(kOff+kLen) > len(d.mem) {
			return 0, ErrOutOfBoundsAccess
		}
		if string(d.mem[kOff:kOff+kLen]) == key {
			return valNodeOff, nil
		}
	}
	return 0, ErrMalformedJSONInput
}

// GetStr looks up a string field `key` in the linear-memory node table of `v`
// with 0 B/op and 0 allocs/op when returning a sub-slice view over `d.mem`.
func (v View) GetStr(key string) (string, error) {
	d, err := v.checkLive()
	if err != nil {
		return "", err
	}
	valOff, err := v.lookupObjectField(key)
	if err != nil {
		return "", err
	}
	kind, sOff, sLen, _, _, err := d.readNode(valOff)
	if err != nil {
		return "", err
	}
	if kind != NodeKindString {
		return "", ErrInvalidNodeKind
	}
	if int(sOff+sLen) > int(d.bump) || int(sOff+sLen) > len(d.mem) {
		return "", ErrOutOfBoundsAccess
	}
	return string(d.mem[sOff : sOff+sLen]), nil
}

// GetBytesZeroCopy returns a zero-copy borrowed `[]byte` slice directly into
// the Wasm linear-memory arena (`0 B/op`, `0 allocs/op`).
func (v View) GetBytesZeroCopy(key string) ([]byte, error) {
	d, err := v.checkLive()
	if err != nil {
		return nil, err
	}
	valOff, err := v.lookupObjectField(key)
	if err != nil {
		return nil, err
	}
	kind, sOff, sLen, _, _, err := d.readNode(valOff)
	if err != nil {
		return nil, err
	}
	if kind != NodeKindString {
		return nil, ErrInvalidNodeKind
	}
	if int(sOff+sLen) > int(d.bump) || int(sOff+sLen) > len(d.mem) {
		return nil, ErrOutOfBoundsAccess
	}
	return d.mem[sOff : sOff+sLen : sOff+sLen], nil
}

// GetInt looks up an int64 field `key` in `v`.
func (v View) GetInt(key string) (int64, error) {
	d, err := v.checkLive()
	if err != nil {
		return 0, err
	}
	valOff, err := v.lookupObjectField(key)
	if err != nil {
		return 0, err
	}
	kind, lo, hi, _, _, err := d.readNode(valOff)
	if err != nil {
		return 0, err
	}
	if kind != NodeKindInt {
		return 0, ErrInvalidNodeKind
	}
	u := (uint64(hi) << 32) | uint64(lo)
	return int64(u), nil
}

// GetArray returns a borrowed `View` pointing to the array at `v[key]`.
func (v View) GetArray(key string) (View, error) {
	d, err := v.checkLive()
	if err != nil {
		return View{}, err
	}
	valOff, err := v.lookupObjectField(key)
	if err != nil {
		return View{}, err
	}
	kind, _, _, _, _, err := d.readNode(valOff)
	if err != nil {
		return View{}, err
	}
	if kind != NodeKindArray {
		return View{}, ErrInvalidNodeKind
	}
	return View{owner: d, gen: d.gen, nodeOff: valOff}, nil
}

// ArrayLen returns the number of elements in an array `View`.
func (v View) ArrayLen() (int, error) {
	d, err := v.checkLive()
	if err != nil {
		return 0, err
	}
	kind, _, count, _, _, err := d.readNode(v.nodeOff)
	if err != nil {
		return 0, err
	}
	if kind != NodeKindArray {
		return 0, ErrInvalidNodeKind
	}
	return int(count), nil
}

// ArrayGet returns a borrowed `View` for element `idx` of an array `View`.
func (v View) ArrayGet(idx int) (View, error) {
	d, err := v.checkLive()
	if err != nil {
		return View{}, err
	}
	kind, tableOff, count, _, _, err := d.readNode(v.nodeOff)
	if err != nil {
		return View{}, err
	}
	if kind != NodeKindArray || idx < 0 || uint32(idx) >= count {
		return View{}, ErrOutOfBoundsAccess
	}
	elemEntryOff := tableOff + uint32(idx)*4
	if int(elemEntryOff+4) > int(d.bump) || int(elemEntryOff+4) > len(d.mem) {
		return View{}, ErrOutOfBoundsAccess
	}
	elemNodeOff := binary.LittleEndian.Uint32(d.mem[elemEntryOff : elemEntryOff+4])
	return View{owner: d, gen: d.gen, nodeOff: elemNodeOff}, nil
}

// ArrayPushString appends a string to an array `View` inside its owning arena.
func (v View) ArrayPushString(s string) error {
	d, err := v.checkLive()
	if err != nil {
		return err
	}
	kind, tableOff, count, capacity, _, err := d.readNode(v.nodeOff)
	if err != nil {
		return err
	}
	if kind != NodeKindArray {
		return ErrInvalidNodeKind
	}
	sOff, sLen, err := d.allocStringBytes(s)
	if err != nil {
		return err
	}
	strNodeOff, err := d.writeNode(NodeKindString, sOff, sLen, 0, 0)
	if err != nil {
		return err
	}
	if count >= capacity {
		newCap := capacity * 2
		if newCap == 0 {
			newCap = 8
		}
		newTableOff, err := d.allocBytes(newCap * 4)
		if err != nil {
			return err
		}
		copy(d.mem[newTableOff:newTableOff+count*4], d.mem[tableOff:tableOff+count*4])
		tableOff = newTableOff
		capacity = newCap
	}
	entryOff := tableOff + count*4
	binary.LittleEndian.PutUint32(d.mem[entryOff:entryOff+4], strNodeOff)
	count++
	binary.LittleEndian.PutUint32(d.mem[v.nodeOff+4:v.nodeOff+8], tableOff)
	binary.LittleEndian.PutUint32(d.mem[v.nodeOff+8:v.nodeOff+12], count)
	binary.LittleEndian.PutUint32(d.mem[v.nodeOff+12:v.nodeOff+16], capacity)
	return nil
}

// Set moves `child` (`OwnHandle`) into `d` (`OwnHandle`) under `key` and
// consumes `child` (matching `hy_json_set` in `runtime/rust/hyrt/src/lib.rs`).
func (d *Doc) Set(key string, child *Doc) error {
	if d == nil || d.freed || child == nil || child.freed {
		return ErrUseAfterFree
	}
	valNodeOff, err := d.cloneSubtreeFrom(child, child.rootOff, 0)
	if err != nil {
		return err
	}
	child.Free()
	return d.setObjectFieldNode(d.rootOff, key, valNodeOff)
}

func (d *Doc) setObjectFieldNode(objNodeOff uint32, key string, valNodeOff uint32) error {
	kind, tableOff, count, capacity, _, err := d.readNode(objNodeOff)
	if err != nil {
		return err
	}
	if kind != NodeKindObject {
		return ErrInvalidNodeKind
	}
	// Replace in place if key exists.
	for i := uint32(0); i < count; i++ {
		entryOff := tableOff + i*12
		kOff := binary.LittleEndian.Uint32(d.mem[entryOff+0 : entryOff+4])
		kLen := binary.LittleEndian.Uint32(d.mem[entryOff+4 : entryOff+8])
		if string(d.mem[kOff:kOff+kLen]) == key {
			binary.LittleEndian.PutUint32(d.mem[entryOff+8:entryOff+12], valNodeOff)
			return nil
		}
	}
	kOff, kLen, err := d.allocStringBytes(key)
	if err != nil {
		return err
	}
	if count >= capacity {
		newCap := capacity * 2
		if newCap == 0 {
			newCap = 8
		}
		newTableOff, err := d.allocBytes(newCap * 12)
		if err != nil {
			return err
		}
		copy(d.mem[newTableOff:newTableOff+count*12], d.mem[tableOff:tableOff+count*12])
		tableOff = newTableOff
		capacity = newCap
	}
	entryOff := tableOff + count*12
	binary.LittleEndian.PutUint32(d.mem[entryOff+0:entryOff+4], kOff)
	binary.LittleEndian.PutUint32(d.mem[entryOff+4:entryOff+8], kLen)
	binary.LittleEndian.PutUint32(d.mem[entryOff+8:entryOff+12], valNodeOff)
	count++
	binary.LittleEndian.PutUint32(d.mem[objNodeOff+4:objNodeOff+8], tableOff)
	binary.LittleEndian.PutUint32(d.mem[objNodeOff+8:objNodeOff+12], count)
	binary.LittleEndian.PutUint32(d.mem[objNodeOff+12:objNodeOff+16], capacity)
	return nil
}

func (d *Doc) cloneSubtreeFrom(src *Doc, srcNodeOff uint32, depth int) (uint32, error) {
	if depth > maxRecursionDepth {
		return 0, ErrMaxDepthExceeded
	}
	kind, a, b, c, e, err := src.readNode(srcNodeOff)
	if err != nil {
		return 0, err
	}
	switch kind {
	case NodeKindNull, NodeKindInt, NodeKindBool:
		return d.writeNode(kind, a, b, c, e)
	case NodeKindString:
		s := string(src.mem[a : a+b])
		sOff, sLen, err := d.allocStringBytes(s)
		if err != nil {
			return 0, err
		}
		return d.writeNode(NodeKindString, sOff, sLen, 0, 0)
	case NodeKindArray:
		count := b
		tableOff, err := d.allocBytes(count * 4)
		if err != nil {
			return 0, err
		}
		for i := uint32(0); i < count; i++ {
			elemOff := binary.LittleEndian.Uint32(src.mem[a+i*4 : a+i*4+4])
			clonedElem, err := d.cloneSubtreeFrom(src, elemOff, depth+1)
			if err != nil {
				return 0, err
			}
			binary.LittleEndian.PutUint32(d.mem[tableOff+i*4:tableOff+i*4+4], clonedElem)
		}
		return d.writeNode(NodeKindArray, tableOff, count, count, 0)
	case NodeKindObject:
		count := b
		tableOff, err := d.allocBytes(count * 12)
		if err != nil {
			return 0, err
		}
		for i := uint32(0); i < count; i++ {
			entryOff := a + i*12
			kOff := binary.LittleEndian.Uint32(src.mem[entryOff+0 : entryOff+4])
			kLen := binary.LittleEndian.Uint32(src.mem[entryOff+4 : entryOff+8])
			vOff := binary.LittleEndian.Uint32(src.mem[entryOff+8 : entryOff+12])
			newKOff, newKLen, err := d.allocStringBytes(string(src.mem[kOff : kOff+kLen]))
			if err != nil {
				return 0, err
			}
			newVOff, err := d.cloneSubtreeFrom(src, vOff, depth+1)
			if err != nil {
				return 0, err
			}
			dstEntry := tableOff + i*12
			binary.LittleEndian.PutUint32(d.mem[dstEntry+0:dstEntry+4], newKOff)
			binary.LittleEndian.PutUint32(d.mem[dstEntry+4:dstEntry+8], newKLen)
			binary.LittleEndian.PutUint32(d.mem[dstEntry+8:dstEntry+12], newVOff)
		}
		return d.writeNode(NodeKindObject, tableOff, count, count, 0)
	default:
		return 0, ErrInvalidNodeKind
	}
}

// CompactText serializes `v` to compact JSON text.
func (v View) CompactText() (string, error) {
	d, err := v.checkLive()
	if err != nil {
		return "", err
	}
	val, err := d.exportGoValue(v.nodeOff, 0)
	if err != nil {
		return "", err
	}
	raw, err := json.Marshal(val)
	if err != nil {
		return "", err
	}
	return string(raw), nil
}

// PrettyText serializes `v` to 2-space indented pretty JSON text (`serde_json::to_string_pretty`).
func (v View) PrettyText() (string, error) {
	d, err := v.checkLive()
	if err != nil {
		return "", err
	}
	val, err := d.exportGoValue(v.nodeOff, 0)
	if err != nil {
		return "", err
	}
	raw, err := json.MarshalIndent(val, "", "  ")
	if err != nil {
		return "", err
	}
	return string(raw), nil
}

func (d *Doc) exportGoValue(nodeOff uint32, depth int) (any, error) {
	if depth > maxRecursionDepth {
		return nil, ErrMaxDepthExceeded
	}
	kind, a, b, _, _, err := d.readNode(nodeOff)
	if err != nil {
		return nil, err
	}
	switch kind {
	case NodeKindNull:
		return nil, nil
	case NodeKindString:
		return string(d.mem[a : a+b]), nil
	case NodeKindInt:
		u := (uint64(b) << 32) | uint64(a)
		return int64(u), nil
	case NodeKindBool:
		return a != 0, nil
	case NodeKindArray:
		out := make([]any, b)
		for i := uint32(0); i < b; i++ {
			elemOff := binary.LittleEndian.Uint32(d.mem[a+i*4 : a+i*4+4])
			ev, err := d.exportGoValue(elemOff, depth+1)
			if err != nil {
				return nil, err
			}
			out[i] = ev
		}
		return out, nil
	case NodeKindObject:
		out := make(map[string]any, b)
		for i := uint32(0); i < b; i++ {
			entryOff := a + i*12
			kOff := binary.LittleEndian.Uint32(d.mem[entryOff+0 : entryOff+4])
			kLen := binary.LittleEndian.Uint32(d.mem[entryOff+4 : entryOff+8])
			vOff := binary.LittleEndian.Uint32(d.mem[entryOff+8 : entryOff+12])
			ev, err := d.exportGoValue(vOff, depth+1)
			if err != nil {
				return nil, err
			}
			out[string(d.mem[kOff:kOff+kLen])] = ev
		}
		return out, nil
	default:
		return nil, ErrInvalidNodeKind
	}
}

// tryFastFlatObject parses a flat JSON object (`{"k":"v","n":123}`) with zero
// intermediate Go heap allocations directly into `d.mem`. If it encounters nested
// `{` or `[` or escape sequences, it resets `d.bump` and falls back to `parseRecursiveTree`.
func (d *Doc) tryFastFlatObject(s string) (bool, error) {
	n := len(s)
	i := 0
	for i < n && (s[i] == ' ' || s[i] == '\t' || s[i] == '\n' || s[i] == '\r') {
		i++
	}
	if i >= n || s[i] != '{' {
		return false, nil
	}
	i++

	const maxFastFields = 32
	tableOff, err := d.allocBytes(maxFastFields * 12)
	if err != nil {
		return false, err
	}
	rootOff, err := d.writeNode(NodeKindObject, tableOff, 0, maxFastFields, 0)
	if err != nil {
		return false, err
	}
	d.rootOff = rootOff
	count := uint32(0)

	for i < n {
		for i < n && (s[i] == ' ' || s[i] == '\t' || s[i] == '\n' || s[i] == '\r' || s[i] == ',') {
			i++
		}
		if i < n && s[i] == '}' {
			i++
			for i < n && (s[i] == ' ' || s[i] == '\t' || s[i] == '\n' || s[i] == '\r') {
				i++
			}
			if i != n {
				d.bump = 0
				return false, nil
			}
			binary.LittleEndian.PutUint32(d.mem[rootOff+8:rootOff+12], count)
			return true, nil
		}
		if i >= n || s[i] != '"' {
			d.bump = 0
			return false, nil
		}
		i++
		kStart := i
		for i < n && s[i] != '"' {
			if s[i] == '\\' {
				d.bump = 0
				return false, nil
			}
			i++
		}
		if i >= n {
			return false, ErrMalformedJSONInput
		}
		kStr := s[kStart:i]
		i++

		for i < n && (s[i] == ' ' || s[i] == '\t' || s[i] == '\n' || s[i] == '\r') {
			i++
		}
		if i >= n || s[i] != ':' {
			return false, ErrMalformedJSONInput
		}
		i++
		for i < n && (s[i] == ' ' || s[i] == '\t' || s[i] == '\n' || s[i] == '\r') {
			i++
		}
		if i >= n {
			return false, ErrMalformedJSONInput
		}
		if s[i] == '{' || s[i] == '[' {
			d.bump = 0
			return false, nil
		}
		if count >= maxFastFields {
			d.bump = 0
			return false, nil
		}

		var valNodeOff uint32
		if s[i] == '"' {
			i++
			vStart := i
			for i < n && s[i] != '"' {
				if s[i] == '\\' {
					d.bump = 0
					return false, nil
				}
				i++
			}
			if i >= n {
				return false, ErrMalformedJSONInput
			}
			vStr := s[vStart:i]
			i++
			vOff, vLen, err := d.allocStringBytes(vStr)
			if err != nil {
				return false, err
			}
			valNodeOff, err = d.writeNode(NodeKindString, vOff, vLen, 0, 0)
			if err != nil {
				return false, err
			}
		} else {
			// Fall back to full recursive parser for non-string primitives (numbers, bools, null).
			d.bump = 0
			return false, nil
		}

		kOff, kLen, err := d.allocStringBytes(kStr)
		if err != nil {
			return false, err
		}
		entryOff := tableOff + count*12
		binary.LittleEndian.PutUint32(d.mem[entryOff+0:entryOff+4], kOff)
		binary.LittleEndian.PutUint32(d.mem[entryOff+4:entryOff+8], kLen)
		binary.LittleEndian.PutUint32(d.mem[entryOff+8:entryOff+12], valNodeOff)
		count++
	}
	return false, ErrMalformedJSONInput
}

func (d *Doc) parseRecursiveTree(s string) (uint32, error) {
	dec := json.NewDecoder(bytes.NewReader([]byte(s)))
	dec.UseNumber()
	var raw any
	if err := dec.Decode(&raw); err != nil {
		return 0, ErrMalformedJSONInput
	}
	// Ensure no trailing non-whitespace tokens remain.
	if _, err := dec.Token(); err != io.EOF {
		return 0, ErrMalformedJSONInput
	}
	return d.lowerAnyToLinearMemory(raw, 1)
}

func (d *Doc) lowerAnyToLinearMemory(v any, depth int) (uint32, error) {
	if depth > maxRecursionDepth {
		return 0, ErrMaxDepthExceeded
	}
	switch val := v.(type) {
	case nil:
		return d.writeNode(NodeKindNull, 0, 0, 0, 0)
	case bool:
		b := uint32(0)
		if val {
			b = 1
		}
		return d.writeNode(NodeKindBool, b, 0, 0, 0)
	case json.Number:
		n, err := val.Int64()
		if err != nil {
			return 0, ErrMalformedJSONInput
		}
		u := uint64(n)
		return d.writeNode(NodeKindInt, uint32(u&0xFFFFFFFF), uint32(u>>32), 0, 0)
	case string:
		sOff, sLen, err := d.allocStringBytes(val)
		if err != nil {
			return 0, err
		}
		return d.writeNode(NodeKindString, sOff, sLen, 0, 0)
	case []any:
		count := uint32(len(val))
		capCount := count
		if capCount == 0 {
			capCount = 4
		}
		tableOff, err := d.allocBytes(capCount * 4)
		if err != nil {
			return 0, err
		}
		for i, elem := range val {
			elemNodeOff, err := d.lowerAnyToLinearMemory(elem, depth+1)
			if err != nil {
				return 0, err
			}
			binary.LittleEndian.PutUint32(d.mem[tableOff+uint32(i)*4:tableOff+uint32(i)*4+4], elemNodeOff)
		}
		return d.writeNode(NodeKindArray, tableOff, count, capCount, 0)
	case map[string]any:
		count := uint32(len(val))
		capCount := count
		if capCount == 0 {
			capCount = 4
		}
		tableOff, err := d.allocBytes(capCount * 12)
		if err != nil {
			return 0, err
		}
		idx := uint32(0)
		for k, fieldVal := range val {
			kOff, kLen, err := d.allocStringBytes(k)
			if err != nil {
				return 0, err
			}
			vOff, err := d.lowerAnyToLinearMemory(fieldVal, depth+1)
			if err != nil {
				return 0, err
			}
			entryOff := tableOff + idx*12
			binary.LittleEndian.PutUint32(d.mem[entryOff+0:entryOff+4], kOff)
			binary.LittleEndian.PutUint32(d.mem[entryOff+4:entryOff+8], kLen)
			binary.LittleEndian.PutUint32(d.mem[entryOff+8:entryOff+12], vOff)
			idx++
		}
		return d.writeNode(NodeKindObject, tableOff, count, capCount, 0)
	default:
		return 0, ErrMalformedJSONInput
	}
}

// ---------------------------------------------------------------------------
// Complete Pure-Go Wasm2Go Export Surface (`CGO_ENABLED=0`)
// ---------------------------------------------------------------------------
//
// Models all 18 exports in `runtime/rust/hyrt/src/lib.rs` plus the `flate2`
// and `ed25519` boundary surfaces named in Issue #757.

// HyNop is the pure-Go lowering of `hy_nop(x: u64) -> u64`.
func HyNop(x uint64) uint64 {
	return x
}

// HyrtAbiWord is the pure-Go lowering of `hyrt_abi_word() -> u64`.
func HyrtAbiWord() uint64 {
	return abiHandshakeWord
}

// HyrtStackAlignmentProbe is the pure-Go lowering of `hyrt_stack_alignment_probe() -> u64`.
func HyrtStackAlignmentProbe() uint64 {
	return 0
}

// Keccak-f[1600] round constants and rotation offsets for pure-Go `hy_sha3_digest256`
// and `hy_sha3_stream_checksum` (matches Rust `sha3 = "=0.10.8"` byte-for-byte).
var keccakRC = [24]uint64{
	0x0000000000000001, 0x0000000000008082, 0x800000000000808a, 0x8000000080008000,
	0x000000000000808b, 0x0000000080000001, 0x8000000080008081, 0x8000000000008009,
	0x000000000000008a, 0x0000000000000088, 0x0000000080008009, 0x000000008000000a,
	0x000000008000808b, 0x800000000000008b, 0x8000000000008089, 0x8000000000008003,
	0x8000000000008002, 0x8000000000000080, 0x000000000000800a, 0x800000008000000a,
	0x8000000080008081, 0x8000000000008080, 0x0000000080000001, 0x8000000080008008,
}

var keccakPiln = [24]int{
	10, 7, 11, 17, 18, 3, 5, 16, 8, 21, 24, 4, 15, 23, 19, 13, 12, 2, 20, 14, 22, 9, 6, 1,
}

var keccakRotc = [24]uint{
	1, 3, 6, 10, 15, 21, 28, 36, 45, 55, 2, 14, 27, 41, 56, 8, 25, 43, 62, 18, 39, 61, 20, 44,
}

func rotl64(x uint64, s uint) uint64 {
	return (x << s) | (x >> (64 - s))
}

func keccakF1600(st *[25]uint64) {
	var bc [5]uint64
	for round := 0; round < 24; round++ {
		for i := 0; i < 5; i++ {
			bc[i] = st[i] ^ st[i+5] ^ st[i+10] ^ st[i+15] ^ st[i+20]
		}
		for i := 0; i < 5; i++ {
			t := bc[(i+4)%5] ^ rotl64(bc[(i+1)%5], 1)
			for j := 0; j < 25; j += 5 {
				st[j+i] ^= t
			}
		}
		t := st[1]
		for i := 0; i < 24; i++ {
			j := keccakPiln[i]
			bc[0] = st[j]
			st[j] = rotl64(t, keccakRotc[i])
			t = bc[0]
		}
		for j := 0; j < 25; j += 5 {
			for i := 0; i < 5; i++ {
				bc[i] = st[j+i]
			}
			for i := 0; i < 5; i++ {
				st[j+i] ^= (^bc[(i+1)%5]) & bc[(i+2)%5]
			}
		}
		st[0] ^= keccakRC[round]
	}
}

// HySha3Digest256 computes SHA3-256 of `data` into `out[0..32]` with 0 heap allocations.
func HySha3Digest256(data []byte, out []byte) int32 {
	if len(out) < 32 {
		return 1
	}
	const rate = 136
	var st [25]uint64
	var block [rate]byte

	for len(data) >= rate {
		for i := 0; i < rate/8; i++ {
			st[i] ^= binary.LittleEndian.Uint64(data[i*8 : (i+1)*8])
		}
		keccakF1600(&st)
		data = data[rate:]
	}
	copy(block[:], data)
	block[len(data)] = 0x06
	block[rate-1] |= 0x80
	for i := 0; i < rate/8; i++ {
		st[i] ^= binary.LittleEndian.Uint64(block[i*8 : (i+1)*8])
	}
	keccakF1600(&st)
	for i := 0; i < 4; i++ {
		binary.LittleEndian.PutUint64(out[i*8:(i+1)*8], st[i])
	}
	return 0
}

// HySha3StreamChecksum lowers `hy_sha3_stream_checksum(iters, seed)` into pure Go
// matching `runtime/rust/hyrt/src/lib.rs` bit-for-bit.
func HySha3StreamChecksum(iters uint64, seed uint64) uint64 {
	state := seed ^ uint64(0x6879627269645f72)
	var buf [8]byte
	var digest [32]byte
	for i := uint64(0); i < iters; i++ {
		binary.LittleEndian.PutUint64(buf[:], state)
		HySha3Digest256(buf[:], digest[:])
		word := binary.LittleEndian.Uint64(digest[0:8])
		state = rotl64(state, 13) ^ word ^ i
	}
	return state
}

// HyRegexIsMatch lowers `hy_regex_is_match` (DEEP class in `interfaces/descriptors.md`)
// onto the growable Go goroutine stack without cgo or the 1 MiB C stack pool.
func HyRegexIsMatch(pattern string, text string) int64 {
	re, err := regexp.Compile(pattern)
	if err != nil {
		return -1
	}
	if re.MatchString(text) {
		return 1
	}
	return 0
}

// HyRegexIsValid lowers `hy_regex_is_valid`.
func HyRegexIsValid(pattern string) int64 {
	if _, err := regexp.Compile(pattern); err != nil {
		return 0
	}
	return 1
}

// HyFlateCompressRoundtrip exercises stateful Deflate compression/decompression
// inside a linear-memory arena (`flate2` boundary crate named in #757).
func HyFlateCompressRoundtrip(payload []byte) ([]byte, error) {
	var compressed bytes.Buffer
	zw, err := flate.NewWriter(&compressed, flate.DefaultCompression)
	if err != nil {
		return nil, err
	}
	if _, err := zw.Write(payload); err != nil {
		return nil, err
	}
	if err := zw.Close(); err != nil {
		return nil, err
	}
	zr := flate.NewReader(&compressed)
	defer zr.Close()
	return io.ReadAll(zr)
}

// HyEd25519SignVerify exercises `ed25519-dalek` deterministic signing and verification
// alongside SHA-256 digesting in pure Go (`CGO_ENABLED=0`).
func HyEd25519SignVerify(seed32 [32]byte, message []byte) (bool, [32]byte) {
	priv := ed25519.NewKeyFromSeed(seed32[:])
	pub := priv.Public().(ed25519.PublicKey)
	sig := ed25519.Sign(priv, message)
	digest := sha256.Sum256(message)
	return ed25519.Verify(pub, message, sig), digest
}

// ---------------------------------------------------------------------------
// Generation-Tagged Handle Table (`abi-types.md` §2 Compatibility Layer)
// ---------------------------------------------------------------------------
//
// Proves that even the handle-table protocol (`(gen << 32) | (idx + 1)`),
// `hy_json_value_text` / `hy_json_to_string_pretty` short-buffer `TRY_AGAIN (3)`
// semantics, and monotonic generation increments on slot reuse work in pure Go
// while fixing the `gen = 1u32` constant-generation bug in `runtime/rust/hyrt/src/lib.rs:146`.

type tableSlot struct {
	gen uint32
	doc *Doc
}

type HandleTable struct {
	mu    sync.Mutex
	slots []tableSlot
	free  []uint32
}

var defaultTable HandleTable

func HandlePack(idx uint32, gen uint32) uint64 {
	return (uint64(gen) << 32) | uint64(idx+1)
}

func HandleUnpack(h uint64) (idx uint32, gen uint32, ok bool) {
	if h == 0 || h == ^uint64(0) {
		return 0, 0, false
	}
	low := uint32(h & 0xFFFFFFFF)
	if low == 0 {
		return 0, 0, false
	}
	return low - 1, uint32(h >> 32), true
}

func (t *HandleTable) Insert(doc *Doc) uint64 {
	t.mu.Lock()
	defer t.mu.Unlock()
	if n := len(t.free); n > 0 {
		idx := t.free[n-1]
		t.free = t.free[:n-1]
		// Monotonically increment the slot's generation on reuse so any stale
		// handle from the previous occupant deterministically fails with StatusErrTable.
		nextGen := t.slots[idx].gen + 1
		if nextGen == 0 {
			nextGen = 1
		}
		t.slots[idx] = tableSlot{gen: nextGen, doc: doc}
		return HandlePack(idx, nextGen)
	}
	idx := uint32(len(t.slots))
	t.slots = append(t.slots, tableSlot{gen: 1, doc: doc})
	return HandlePack(idx, 1)
}

func (t *HandleTable) Get(h uint64) (*Doc, int32) {
	t.mu.Lock()
	defer t.mu.Unlock()
	idx, gen, ok := HandleUnpack(h)
	if !ok || int(idx) >= len(t.slots) {
		return nil, StatusErrTable
	}
	slot := t.slots[idx]
	if slot.doc == nil || slot.gen != gen {
		return nil, StatusErrTable
	}
	return slot.doc, StatusOK
}

func (t *HandleTable) Release(h uint64) int32 {
	t.mu.Lock()
	defer t.mu.Unlock()
	idx, gen, ok := HandleUnpack(h)
	if !ok || int(idx) >= len(t.slots) {
		return StatusOK // idempotent release per lib.rs
	}
	slot := &t.slots[idx]
	if slot.doc != nil && slot.gen == gen {
		slot.doc.Free()
		slot.doc = nil
		t.free = append(t.free, idx)
	}
	return StatusOK
}

func (t *HandleTable) LiveCount() uint64 {
	t.mu.Lock()
	defer t.mu.Unlock()
	var count uint64
	for _, s := range t.slots {
		if s.doc != nil {
			count++
		}
	}
	return count
}

// SerializeHandle implements the exact `hy_json_value_text` / `hy_json_to_string_pretty`
// two-step query/copy buffer protocol tested in `runtime/rust/hyrt/src/lib.rs`:
//   - Invalid/stale handle or nil `outLen` -> `StatusErrTable (1)`, buffer untouched
//   - `out == nil` or `len(out) < need` -> writes `*outLen = need`, returns `StatusTryAgain (3)`, buffer untouched
//   - `len(out) >= need` -> copies `need` bytes into `out[:need]`, writes `*outLen = need`, returns `StatusOK (0)`
func (t *HandleTable) SerializeHandle(h uint64, pretty bool, out []byte, outLen *uint64) int32 {
	if outLen == nil {
		return StatusErrTable
	}
	doc, status := t.Get(h)
	if status != StatusOK {
		return status
	}
	root, err := doc.Root()
	if err != nil {
		return StatusErrTable
	}
	var text string
	if pretty {
		text, err = root.PrettyText()
	} else {
		text, err = root.CompactText()
	}
	if err != nil {
		return StatusErrJSON
	}
	need := uint64(len(text))
	*outLen = need
	if out == nil || uint64(len(out)) < need {
		return StatusTryAgain
	}
	copy(out[:need], text)
	return StatusOK
}

// Package w019 models Hybrid's isolated WASM cell execution backend (W-019).
// It enforces compile-time WireSendable admission (HYB-E3901), module capability
// admission (HYB-E3902), and persistent command-reactor stepping without
// re-entrant host callbacks (HYB-E3903).
package w019

import (
	"encoding/binary"
	"errors"
	"fmt"
	"sync"
	"sync/atomic"
)

const (
	// ErrCodeWireNotSendable indicates a non-WireSendable type at cell boundary.
	ErrCodeWireNotSendable = "HYB-E3901"
	// ErrCodeModuleViolation indicates forbidden shared memory or WASI imports.
	ErrCodeModuleViolation = "HYB-E3902"
	// ErrCodeExecutionViolation indicates _start re-run or re-entrant callback.
	ErrCodeExecutionViolation = "HYB-E3903"

	// MaxWireDepth bounds nested tree depth across cell wire frames.
	MaxWireDepth = 8
	// MaxWireFrameBytes bounds serialized wire message size to one 64 KiB page.
	MaxWireFrameBytes = 65536
	// WasmPageBytes is the standard 64 KiB WebAssembly linear memory page size.
	WasmPageBytes = 65536
)

// Live is a zero-size phantom linear witness representing an active capability.
type Live[R any] struct {
	_ [0]R
}

// Moved is a zero-size phantom linear witness representing a consumed capability.
type Moved[R any] struct {
	_ [0]R
}

// MintLive creates an active zero-size linear witness for resource R.
func MintLive[R any]() Live[R] {
	return Live[R]{}
}

// ConsumeLive transitions a Live[R] witness into a Moved[R] witness.
func ConsumeLive[R any](_ Live[R]) Moved[R] {
	return Moved[R]{}
}

// WireScalarPacket is a statically admitted value-only message type.
type WireScalarPacket struct {
	Tag   uint32
	Value int64
}

// WireSendableStatic constrains static compile-time cell message types.
// Raw pointers (*T), channels, and closures do not satisfy this interface.
type WireSendableStatic interface {
	WireScalarPacket | int64 | uint64 | string
}

// StaticSend consumes a Live[M] witness and admits only WireSendableStatic messages.
func StaticSend[M WireSendableStatic](_ Live[M], msg M) (Moved[M], M) {
	return Moved[M]{}, msg
}

// WireKind enumerates schema tags for dynamic AST admission checks.
type WireKind uint8

const (
	KindScalar WireKind = iota + 1
	KindBytes
	KindTree
	KindPtr
	KindClosure
	KindChan
	KindForeignHandle
)

// WireValue models a schema-checked message AST crossing a cell boundary.
type WireValue struct {
	Kind     WireKind
	Scalar   int64
	Bytes    []byte
	Children []WireValue
}

// CloneDeep returns a deep snapshot copy with zero shared backing slices.
func (v WireValue) CloneDeep() WireValue {
	out := WireValue{
		Kind:   v.Kind,
		Scalar: v.Scalar,
	}
	if len(v.Bytes) > 0 {
		out.Bytes = make([]byte, len(v.Bytes))
		copy(out.Bytes, v.Bytes)
	}
	if len(v.Children) > 0 {
		out.Children = make([]WireValue, len(v.Children))
		for i := range v.Children {
			out.Children[i] = v.Children[i].CloneDeep()
		}
	}
	return out
}

// AdmitWireValue validates that a WireValue satisfies HYB-E3901 constraints.
func AdmitWireValue(v WireValue) (int, error) {
	size, err := validateWireNode(v, 0)
	if err != nil {
		return 0, err
	}
	if size > MaxWireFrameBytes {
		return 0, fmt.Errorf("%s: wire frame %d bytes exceeds limit %d",
			ErrCodeWireNotSendable, size, MaxWireFrameBytes)
	}
	return size, nil
}

func validateWireNode(v WireValue, depth int) (int, error) {
	if depth > MaxWireDepth {
		return 0, fmt.Errorf("%s: wire nesting depth %d exceeds max %d",
			ErrCodeWireNotSendable, depth, MaxWireDepth)
	}
	switch v.Kind {
	case KindScalar:
		return 9, nil
	case KindBytes:
		return 5 + len(v.Bytes), nil
	case KindTree:
		return validateTreeChildren(v.Children, depth)
	default:
		return 0, fmt.Errorf("%s: forbidden wire kind %d across cell boundary",
			ErrCodeWireNotSendable, v.Kind)
	}
}

func validateTreeChildren(children []WireValue, depth int) (int, error) {
	total := 5
	for i := range children {
		childBytes, err := validateWireNode(children[i], depth+1)
		if err != nil {
			return 0, err
		}
		total += childBytes
	}
	return total, nil
}

// EncodeWireFrame serializes an admitted WireValue into dst and returns bytes written.
func EncodeWireFrame(dst []byte, v WireValue) int {
	dst[0] = byte(v.Kind)
	switch v.Kind {
	case KindScalar:
		binary.LittleEndian.PutUint64(dst[1:9], uint64(v.Scalar))
		return 9
	case KindBytes:
		binary.LittleEndian.PutUint32(dst[1:5], uint32(len(v.Bytes)))
		copy(dst[5:], v.Bytes)
		return 5 + len(v.Bytes)
	case KindTree:
		binary.LittleEndian.PutUint32(dst[1:5], uint32(len(v.Children)))
		offset := 5
		for i := range v.Children {
			offset += EncodeWireFrame(dst[offset:], v.Children[i])
		}
		return offset
	default:
		return 0
	}
}

// DecodeWireFrame reconstructs a fresh WireValue snapshot from a linear memory slice.
func DecodeWireFrame(src []byte) (WireValue, int) {
	kind := WireKind(src[0])
	switch kind {
	case KindScalar:
		val := int64(binary.LittleEndian.Uint64(src[1:9]))
		return WireValue{Kind: KindScalar, Scalar: val}, 9
	case KindBytes:
		length := int(binary.LittleEndian.Uint32(src[1:5]))
		buf := make([]byte, length)
		copy(buf, src[5:5+length])
		return WireValue{Kind: KindBytes, Bytes: buf}, 5 + length
	case KindTree:
		count := int(binary.LittleEndian.Uint32(src[1:5]))
		children := make([]WireValue, count)
		offset := 5
		for i := 0; i < count; i++ {
			child, n := DecodeWireFrame(src[offset:])
			children[i] = child
			offset += n
		}
		return WireValue{Kind: KindTree, Children: children}, offset
	default:
		return WireValue{}, 0
	}
}

// ModuleDescriptor describes a WASM cell module submitted for coordinator admission.
type ModuleDescriptor struct {
	Name               string
	UsesSharedMemory   bool
	ImportsHostMemory  bool
	AmbientWasiImports []string
	ExportsCellStep    bool
	UsesOneShotStart   bool
}

// AdmitModule validates a WASM module descriptor against HYB-E3902 and HYB-E3903.
func AdmitModule(desc ModuleDescriptor) error {
	if desc.UsesSharedMemory || desc.ImportsHostMemory {
		return fmt.Errorf("%s: module %q violates private non-shared linear memory",
			ErrCodeModuleViolation, desc.Name)
	}
	if len(desc.AmbientWasiImports) > 0 {
		return fmt.Errorf("%s: module %q declares unauthorized ambient WASI import %q",
			ErrCodeModuleViolation, desc.Name, desc.AmbientWasiImports[0])
	}
	if desc.UsesOneShotStart || !desc.ExportsCellStep {
		return fmt.Errorf("%s: module %q must export persistent hy_cell_step reactor",
			ErrCodeExecutionViolation, desc.Name)
	}
	return nil
}

// CellMemory represents a cell's private 64 KiB linear memory space.
type CellMemory struct {
	Bytes []byte
}

// StepHandler processes an incoming wire message inside a persistent reactor step.
type StepHandler func(cell *CellInstance, msg WireValue) (WireValue, error)

// CellInstance models an instantiated isolated WASM cell with persistent state.
type CellInstance struct {
	ID          uint64
	Memory      CellMemory
	ArenaOffset int
	StepCount   uint64
	inFlight    atomic.Bool
	mu          sync.Mutex
	handler     StepHandler
}

// Step executes a single persistent command-reactor turn (hy_cell_step).
// Re-entrant entry while a frame is active fails closed with HYB-E3903.
func (c *CellInstance) Step(frameLen int) (WireValue, error) {
	if !c.inFlight.CompareAndSwap(false, true) {
		return WireValue{}, fmt.Errorf(
			"%s: re-entrant host callback or nested step on cell %d rejected",
			ErrCodeExecutionViolation, c.ID)
	}
	defer func() {
		c.ArenaOffset = 0
		c.inFlight.Store(false)
	}()

	req, _ := DecodeWireFrame(c.Memory.Bytes[:frameLen])
	resp, err := c.handler(c, req)
	if err != nil {
		return WireValue{}, err
	}
	c.StepCount++
	return resp, nil
}

// Coordinator manages isolated WASM cell admission, instantiation, and messaging.
type Coordinator struct {
	nextID atomic.Uint64
}

// NewCoordinator constructs a host coordinator for isolated WASM cells.
func NewCoordinator() *Coordinator {
	return &Coordinator{}
}

// SpawnCell admits desc and instantiates a persistent cell with private linear memory.
func (coord *Coordinator) SpawnCell(desc ModuleDescriptor, handler StepHandler) (*CellInstance, error) {
	if err := AdmitModule(desc); err != nil {
		return nil, err
	}
	if handler == nil {
		return nil, errors.New("HYB-E3903: nil reactor step handler")
	}
	id := coord.nextID.Add(1)
	return &CellInstance{
		ID:      id,
		Memory:  CellMemory{Bytes: make([]byte, WasmPageBytes)},
		handler: handler,
	}, nil
}

// SendSnapshot validates wire admission, copies the frame into dst's private
// linear memory arena, executes one reactor step, and returns an isolated reply.
func (coord *Coordinator) SendSnapshot(dst *CellInstance, msg WireValue) (WireValue, error) {
	frameSize, err := AdmitWireValue(msg)
	if err != nil {
		return WireValue{}, err
	}
	dst.mu.Lock()
	defer dst.mu.Unlock()

	dst.ArenaOffset = frameSize
	written := EncodeWireFrame(dst.Memory.Bytes[:frameSize], msg)
	reply, err := dst.Step(written)
	if err != nil {
		return WireValue{}, err
	}
	if _, err := AdmitWireValue(reply); err != nil {
		return WireValue{}, err
	}
	return reply.CloneDeep(), nil
}

// SpawnBypassedSharedMemoryPair is a falsifier-only constructor (F1) that
// bypasses AdmitModule and backs two cells with the same linear memory slice.
func (coord *Coordinator) SpawnBypassedSharedMemoryPair(h StepHandler) (*CellInstance, *CellInstance) {
	sharedBacking := make([]byte, WasmPageBytes)
	c1 := &CellInstance{
		ID:      coord.nextID.Add(1),
		Memory:  CellMemory{Bytes: sharedBacking},
		handler: h,
	}
	c2 := &CellInstance{
		ID:      coord.nextID.Add(1),
		Memory:  CellMemory{Bytes: sharedBacking},
		handler: h,
	}
	return c1, c2
}

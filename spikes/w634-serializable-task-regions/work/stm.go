// Package stm implements the bounded M1/M2 prototype for Wildcard #634:
// "Delete sendability analysis: task regions as serializable transactions".
//
// Emitted Go code inside an opted-in `tx_region` routes mutable composite accesses
// (maps, slices, structs, and cyclic object graphs) through typed, versioned
// transactional cells (`TxCell[T]`, `TxMap[K,V]`, `TxSlice[T]`) backed by an
// optimistic TL2/MVCC per-task read/write set.
package stm

import (
	"context"
	"errors"
	"sync"
	"sync/atomic"
)

// ErrTxConflict indicates an optimistic concurrency conflict requiring a clean retry.
var ErrTxConflict = errors.New("HYB-TX-CONFLICT: read/write set version mismatch")

// ErrTxCancelled indicates the enclosing task context cancelled before commit.
var ErrTxCancelled = errors.New("HYB-TX-CANCELLED: transactional task cancelled")

// PureBodyToken is a unexported-field capability token synthesized only by the
// Hybrid compiler after verifying HYB-E3601..HYB-E3603 (no unmediated escapes,
// no I/O or foreign callbacks, no linear Rust handle drops).
type PureBodyToken struct {
	_ [0]func()
}

// SynthesizePureToken is emitted by `hyb` only for statically verified pure task bodies.
func SynthesizePureToken() PureBodyToken {
	return PureBodyToken{}
}

type cellEntry interface {
	version() uint64
	commitWrite(raw any, newVer uint64)
}

// TxCell represents a typed transactional memory cell with stable identity.
type TxCell[T any] struct {
	id      uint64
	mu      sync.RWMutex
	val     T
	ver     atomic.Uint64
	cloner  func(T) T
}

// NewTxCell allocates a versioned cell inside the transactional region heap.
func NewTxCell[T any](r *Region, initial T, cloner func(T) T) *TxCell[T] {
	id := r.nextID.Add(1)
	if cloner == nil {
		cloner = func(v T) T { return v }
	}
	c := &TxCell[T]{
		id:     id,
		val:    cloner(initial),
		cloner: cloner,
	}
	c.ver.Store(r.clock.Load())
	r.regMu.Lock()
	r.cells[id] = c
	r.regMu.Unlock()
	return c
}

func (c *TxCell[T]) version() uint64 {
	return c.ver.Load()
}

func (c *TxCell[T]) commitWrite(raw any, newVer uint64) {
	v := raw.(T)
	c.val = c.cloner(v)
	c.ver.Store(newVer)
}

// Region coordinates transactional tasks, global commit clock, and serializable validation.
type Region struct {
	clock    atomic.Uint64
	nextID   atomic.Uint64
	commitMu sync.Mutex
	regMu    sync.RWMutex
	cells    map[uint64]cellEntry
	Commits  atomic.Uint64
	Aborts   atomic.Uint64
}

// NewRegion initializes a software-transactional shared-heap membrane.
func NewRegion() *Region {
	return &Region{
		cells: make(map[uint64]cellEntry),
	}
}

// Tx holds the per-task snapshot epoch, read-set versions, and buffered write-set.
type Tx struct {
	region    *Region
	readEpoch uint64
	readSet   map[uint64]uint64
	writeSet  map[uint64]any
	aborted   bool
}

func (r *Region) begin() *Tx {
	return &Tx{
		region:    r,
		readEpoch: r.clock.Load(),
		readSet:   make(map[uint64]uint64, 8),
		writeSet:  make(map[uint64]any, 4),
	}
}

// Load mediates a typed read through the transaction's private write-set and snapshot read-set.
func Load[T any](tx *Tx, c *TxCell[T]) (T, error) {
	var zero T
	if tx.aborted {
		return zero, ErrTxConflict
	}
	if buffered, ok := tx.writeSet[c.id]; ok {
		return c.cloner(buffered.(T)), nil
	}
	c.mu.RLock()
	v := c.cloner(c.val)
	ver := c.ver.Load()
	c.mu.RUnlock()

	if prevVer, seen := tx.readSet[c.id]; seen {
		if prevVer != ver {
			tx.aborted = true
			return zero, ErrTxConflict
		}
	} else {
		if ver > tx.readEpoch {
			tx.aborted = true
			return zero, ErrTxConflict
		}
		tx.readSet[c.id] = ver
	}
	return v, nil
}

// Store buffers a typed write in the task's private write-set until commit.
func Store[T any](tx *Tx, c *TxCell[T], val T) error {
	if tx.aborted {
		return ErrTxConflict
	}
	tx.writeSet[c.id] = c.cloner(val)
	return nil
}

func (r *Region) tryCommit(tx *Tx) bool {
	if tx.aborted {
		r.Aborts.Add(1)
		return false
	}
	r.commitMu.Lock()
	defer r.commitMu.Unlock()

	for id, obsVer := range tx.readSet {
		r.regMu.RLock()
		entry := r.cells[id]
		r.regMu.RUnlock()
		if entry.version() != obsVer {
			tx.aborted = true
			r.Aborts.Add(1)
			return false
		}
	}
	if len(tx.writeSet) > 0 {
		newVer := r.clock.Add(1)
		for id, rawVal := range tx.writeSet {
			r.regMu.RLock()
			entry := r.cells[id]
			r.regMu.RUnlock()
			entry.commitWrite(rawVal, newVer)
		}
	}
	r.Commits.Add(1)
	return true
}

// RunTask executes a statically verified pure task body inside a retryable transaction.
func (r *Region) RunTask(ctx context.Context, _ PureBodyToken, body func(tx *Tx) error) error {
	for {
		select {
		case <-ctx.Done():
			r.Aborts.Add(1)
			return ErrTxCancelled
		default:
		}
		tx := r.begin()
		err := body(tx)
		if errors.Is(err, ErrTxConflict) {
			r.Aborts.Add(1)
			continue
		}
		if err != nil {
			r.Aborts.Add(1)
			return err
		}
		if r.tryCommit(tx) {
			return nil
		}
	}
}

// TxMap mediates map accesses via per-key versioned cells.
type TxMap[K comparable, V any] struct {
	region *Region
	mu     sync.Mutex
	slots  map[K]*TxCell[V]
	zero   V
	cloner func(V) V
}

// NewTxMap creates a transactional map with per-key versioned cells.
func NewTxMap[K comparable, V any](r *Region, initial map[K]V, cloner func(V) V) *TxMap[K, V] {
	m := &TxMap[K, V]{
		region: r,
		slots:  make(map[K]*TxCell[V]),
		cloner: cloner,
	}
	for k, v := range initial {
		m.slots[k] = NewTxCell(r, v, cloner)
	}
	return m
}

func (m *TxMap[K, V]) slot(key K) *TxCell[V] {
	m.mu.Lock()
	defer m.mu.Unlock()
	s, ok := m.slots[key]
	if !ok {
		s = NewTxCell(m.region, m.zero, m.cloner)
		m.slots[key] = s
	}
	return s
}

// Get loads a key's value into the transaction's read-set.
func (m *TxMap[K, V]) Get(tx *Tx, key K) (V, error) {
	return Load(tx, m.slot(key))
}

// Put buffers a key's updated value in the transaction's write-set.
func (m *TxMap[K, V]) Put(tx *Tx, key K, val V) error {
	return Store(tx, m.slot(key), val)
}

// TxSlice mediates slice elements across overlapping sub-slice aliases.
type TxSlice[T any] struct {
	cells  []*TxCell[T]
	offset int
	length int
}

// NewTxSlice allocates a slice of versioned transactional cells.
func NewTxSlice[T any](r *Region, items []T, cloner func(T) T) *TxSlice[T] {
	cells := make([]*TxCell[T], len(items))
	for i, v := range items {
		cells[i] = NewTxCell(r, v, cloner)
	}
	return &TxSlice[T]{cells: cells, offset: 0, length: len(items)}
}

// SubSlice creates an aliased view sharing the underlying transactional cells.
func (s *TxSlice[T]) SubSlice(start, end int) *TxSlice[T] {
	return &TxSlice[T]{
		cells:  s.cells,
		offset: s.offset + start,
		length: end - start,
	}
}

// Get reads element idx into the transaction's read-set.
func (s *TxSlice[T]) Get(tx *Tx, idx int) (T, error) {
	return Load(tx, s.cells[s.offset+idx])
}

// Set writes element idx into the transaction's write-set.
func (s *TxSlice[T]) Set(tx *Tx, idx int, val T) error {
	return Store(tx, s.cells[s.offset+idx], val)
}

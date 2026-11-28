// Package roi measures the three rollback strategies on a 1 MiB state:
// copy-before-run (snapshot), an explicit undo log, and inverse execution.
//
// The transform is one ARX sweep over the whole buffer. Its inverse is written
// by hand here (the benchmark measures cost, not synthesis).
package roi

import (
	"bytes"
	"encoding/binary"
	"math/bits"
	"testing"
	"unsafe"
)

const (
	// numWords * 8 bytes = exactly 1 MiB of state.
	numWords = 131072
	key      = 0x0123456789abcdef
	stride   = 0x9e3779b97f4a7c15
)

func fresh() []uint64 {
	b := make([]uint64, numWords)
	for i := range b {
		b[i] = uint64(i)*stride + 0xdeadbeef
	}
	return b
}

// forward is the speculative transform: rotl17 -> xor key -> add stride*i -> rotl41.
func forward(b []uint64) {
	for i, w := range b {
		w = bits.RotateLeft64(w, 17)
		w ^= key
		w += uint64(i) * stride
		w = bits.RotateLeft64(w, 41)
		b[i] = w
	}
}

// inverse is the mechanically-derived inverse of forward, applied to the buffer.
func inverse(b []uint64) {
	for i, w := range b {
		w = bits.RotateLeft64(w, -41)
		w -= uint64(i) * stride
		w ^= key
		w = bits.RotateLeft64(w, -17)
		b[i] = w
	}
}

// undo is one log record: 4 bytes index + 8 bytes old value, padded to 16.
type undo struct {
	idx uint32
	old uint64
}

func forwardLogged(b []uint64, log []undo) {
	for i, w := range b {
		log[i] = undo{idx: uint32(i), old: w}
		w = bits.RotateLeft64(w, 17)
		w ^= key
		w += uint64(i) * stride
		w = bits.RotateLeft64(w, 41)
		b[i] = w
	}
}

func replay(b []uint64, log []undo) {
	for i := len(log) - 1; i >= 0; i-- {
		b[log[i].idx] = log[i].old
	}
}

func TestUndoRecordSize(t *testing.T) {
	if got := unsafe.Sizeof(undo{}); got != 16 {
		t.Fatalf("undo record is %d bytes, the ROI table assumes 16", got)
	}
}

func asBytes(b []uint64) []byte {
	out := make([]byte, len(b)*8)
	for i, v := range b {
		binary.LittleEndian.PutUint64(out[i*8:], v)
	}
	return out
}

// TestSchemesRestoreExactly is the semantic check behind the benchmark: all three
// strategies must return the buffer to the original byte sequence.
func TestSchemesRestoreExactly(t *testing.T) {
	if len(fresh()) != numWords {
		t.Fatal("buffer size mismatch")
	}
	orig := asBytes(fresh())

	// snapshot
	buf := fresh()
	snap := make([]uint64, numWords)
	copy(snap, buf)
	forward(buf)
	copy(buf, snap)
	if !bytes.Equal(asBytes(buf), orig) {
		t.Fatal("snapshot rollback did not restore the buffer")
	}

	// undo log
	buf = fresh()
	log := make([]undo, numWords)
	forwardLogged(buf, log)
	replay(buf, log)
	if !bytes.Equal(asBytes(buf), orig) {
		t.Fatal("undo-log rollback did not restore the buffer")
	}

	// inverse execution
	buf = fresh()
	forward(buf)
	if bytes.Equal(asBytes(buf), orig) {
		t.Fatal("forward transform was a no-op; the benchmark would be meaningless")
	}
	inverse(buf)
	if !bytes.Equal(asBytes(buf), orig) {
		t.Fatal("inverse execution did not restore the buffer")
	}
}

func benchBytes(b *testing.B, buf []uint64) {
	b.SetBytes(int64(len(buf) * 8))
}

func BenchmarkForwardOnly(b *testing.B) {
	buf := fresh()
	benchBytes(b, buf)
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		forward(buf)
	}
}

func BenchmarkInverseOnly(b *testing.B) {
	buf := fresh()
	benchBytes(b, buf)
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		inverse(buf)
	}
}

// BenchmarkSnapshotRollback: copy 1 MiB, transform, abort, copy back.
func BenchmarkSnapshotRollback(b *testing.B) {
	buf := fresh()
	benchBytes(b, buf)
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		snap := make([]uint64, numWords)
		copy(snap, buf)
		forward(buf)
		copy(buf, snap)
	}
}

// BenchmarkUndoLogRollback: transform while logging (idx, old), abort, replay backwards.
func BenchmarkUndoLogRollback(b *testing.B) {
	buf := fresh()
	benchBytes(b, buf)
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		log := make([]undo, numWords)
		forwardLogged(buf, log)
		replay(buf, log)
	}
}

// BenchmarkReversibleRollback: transform, abort, run the derived inverse.
func BenchmarkReversibleRollback(b *testing.B) {
	buf := fresh()
	benchBytes(b, buf)
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		forward(buf)
		inverse(buf)
	}
}

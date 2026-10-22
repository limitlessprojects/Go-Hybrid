package hybridge

import (
	"bytes"
	"testing"
)

func TestOwnHandleAllocAndReset(t *testing.T) {
	h := NewOwnHandle(1024)
	if cap(h.Memory()) < 1024 {
		t.Fatalf("expected capacity >= 1024, got %d", cap(h.Memory()))
	}

	ptr1, slice1, err := h.Alloc(64)
	if err != nil {
		t.Fatalf("unexpected alloc error: %v", err)
	}
	if ptr1 != ReservedNullHeader {
		t.Fatalf("expected ptr1 = %d, got %d", ReservedNullHeader, ptr1)
	}
	if len(slice1) != 64 {
		t.Fatalf("expected slice1 len = 64, got %d", len(slice1))
	}

	copy(slice1, []byte("hello world"))

	sub, err := h.SubSlice(ptr1, 11)
	if err != nil {
		t.Fatalf("unexpected subslice error: %v", err)
	}
	if !bytes.Equal(sub, []byte("hello world")) {
		t.Fatalf("expected 'hello world', got %s", string(sub))
	}

	// O(1) Reset
	h.Reset()
	if h.off != ReservedNullHeader {
		t.Fatalf("expected offset = %d after Reset, got %d", ReservedNullHeader, h.off)
	}

	ptr2, slice2, err := h.Alloc(32)
	if err != nil {
		t.Fatalf("unexpected alloc error: %v", err)
	}
	if ptr2 != ReservedNullHeader {
		t.Fatalf("expected ptr2 = %d after Reset, got %d", ReservedNullHeader, ptr2)
	}
	if len(slice2) != 32 {
		t.Fatalf("expected slice2 len = 32, got %d", len(slice2))
	}
}

func TestOwnHandleAutoGrow(t *testing.T) {
	h := NewOwnHandle(16)
	_, _, err := h.Alloc(128)
	if err != nil {
		t.Fatalf("unexpected grow alloc error: %v", err)
	}
	if len(h.Memory()) < 128 {
		t.Fatalf("expected memory len >= 128, got %d", len(h.Memory()))
	}
}

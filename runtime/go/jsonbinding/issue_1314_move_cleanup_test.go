package jsonbinding

import (
	"runtime"
	"testing"
)

func drainCleanups() {
	for i := 0; i < 6; i++ {
		runtime.GC()
		runtime.Gosched()
	}
}

// TestMovedHandleCleanupDoesNotFreeRecycledSlot verifies Issue #1314 Hazard 1:
// Moving a child handle via Set stops its registered runtime.Cleanup so a
// subsequent GC cycle cannot fire FnRelease on the moved handle's old slot ID
// after that slot has been reused by a newly allocated Value.
func TestMovedHandleCleanupDoesNotFreeRecycledSlot(t *testing.T) {
	drainCleanups()
	parent := NewObject()

	moveChild := func() {
		child, err := Parse([]byte(`{"n":42}`))
		if err != nil {
			t.Fatalf("Parse child: %v", err)
		}
		if err := parent.Set("child", child); err != nil {
			t.Fatalf("parent.Set(child): %v", err)
		}
	}
	moveChild()

	reused, err := Parse([]byte(`{"live":123}`))
	if err != nil {
		t.Fatalf("Parse reused: %v", err)
	}

	for i := 0; i < 6; i++ {
		runtime.GC()
		runtime.Gosched()
	}

	got, err := reused.Int("live")
	if err != nil || got != 123 {
		t.Fatalf("reused.Int(\"live\") after GC = (%d, %v), want (123, nil)", got, err)
	}
}

// TestScalarBoxingSetMoveAndFailureCleanup verifies that scalar boxing through
// Set stops cleanup when moved into an Object, and deterministically releases
// the temporary handle when Set fails on a non-object receiver (e.g. Array).
func TestScalarBoxingSetMoveAndFailureCleanup(t *testing.T) {
	drainCleanups()
	doc := NewObject()
	for i := 0; i < 32; i++ {
		if err := doc.Set("counter", int64(i)); err != nil {
			t.Fatalf("doc.Set(counter, %d): %v", i, err)
		}
	}
	survivor, err := Parse([]byte(`{"ok":999}`))
	if err != nil {
		t.Fatalf("Parse survivor: %v", err)
	}
	arr := NewArray()
	if err := arr.Set("bad_on_array", int64(77)); err == nil {
		t.Fatal("expected Set on array receiver to return ErrJSON")
	}
	for i := 0; i < 6; i++ {
		runtime.GC()
		runtime.Gosched()
	}
	if got, err := survivor.Int("ok"); err != nil || got != 999 {
		t.Fatalf("survivor.Int(\"ok\") = (%d, %v), want (999, nil)", got, err)
	}
}

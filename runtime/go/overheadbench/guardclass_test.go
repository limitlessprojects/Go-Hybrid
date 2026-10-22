package overheadbench

import (
	"testing"
)

func TestPureGoExecution(t *testing.T) {
	if got := Nop(42); got != 42 {
		t.Fatalf("Nop(42) = %d, want 42", got)
	}
}

package sha3binding

// Row 4 stack-budget regression — MEASURED, not argued (see
// jsonbinding/stack_budget_test.go for the method). descriptors.md argues
// the sha3 fns are "constant stack, independent of input length"; the
// painted-stack high-water mark is the check on that argument, at input
// lengths on both sides of the 136-byte SHA3-256 rate (the absorb loop only
// runs for full blocks).

import (
	"testing"
	"unsafe"

	"hybrid.program/hybridge"
)

const (
	smallLimit   = uintptr(hybridge.SmallGuard) // Cross*S
	defaultLimit = uintptr(4096)                // Cross* (hybridge.fastStackNeed)
)

func TestDigest256StackUnderItsGuard(t *testing.T) {
	var worst uintptr
	var worstLen int
	out := make([]byte, 32)
	for _, n := range []int{0, 1, 135, 136, 137, 271, 272, 273, 4096, 1 << 20} {
		data := make([]byte, n+1)
		_, hw := hybridge.StackHighWaterForTest(FnDigest256,
			uintptr(unsafe.Pointer(&data[0])), uintptr(n), uintptr(unsafe.Pointer(&out[0])), 0)
		if hw > worst {
			worst, worstLen = hw, n
		}
	}
	// Digest256 is routed through: Cross3 (DEFAULT, 4096 B). It measures 1048 B
	// from one full 136-byte block up, so it must NOT sit in the 1024 B SMALL class.
	if worst > defaultLimit {
		t.Fatalf("FnDigest256: measured stack high-water %d B (input %d B) exceeds its %d B guard class",
			worst, worstLen, defaultLimit)
	}
	if worst <= smallLimit {
		t.Logf("FnDigest256 now fits the SMALL guard (%d B); the ledger may move it back", worst)
	}
}

func TestStreamChecksumStackUnderItsGuard(t *testing.T) {
	var worst uintptr
	for _, iters := range []uintptr{0, 1, 1000} {
		_, hw := hybridge.StackHighWaterForTest(FnStreamChecksum, iters, 7, 0, 0)
		if hw > worst {
			worst = hw
		}
	}
	// StreamChecksum is routed through: Cross2S (SMALL).
	if worst > smallLimit {
		t.Fatalf("FnStreamChecksum: measured stack high-water %d B exceeds the %d B SMALL guard", worst, smallLimit)
	}
}

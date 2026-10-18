// Package noheap implements the bounded M1/M2 prototype for Wildcard #636:
// "Certified Heap-Freedom & Zero-GC Escape Proofs (#[no_heap]): Stack-Certified Lowering from Tier-0 Loan Graphs".
package noheap

import (
	"fmt"
	"strconv"
	"strings"
)

// MaxImplicitStackVarBytes mirrors Go cmd/compile's 64 KiB stack allocation ceiling.
const MaxImplicitStackVarBytes = 64 * 1024

// NoHeapToken is a zero-size capability witness synthesized only when a function
// passes HYB-E3701..HYB-E3703 static non-escape and frame-budget admission.
type NoHeapToken struct {
	_ [0]func()
}

// SynthesizeNoHeapToken is emitted by `hyb` for verified #[no_heap] scopes.
func SynthesizeNoHeapToken() NoHeapToken {
	return NoHeapToken{}
}

// NaiveEmitSensorWindow represents standard uncertified Go emission where a runtime-sized
// slice allocation and closure/interface dispatch escape to the Go GC heap.
func NaiveEmitSensorWindow(n int, seed int64) int64 {
	buf := make([]int64, n) // escapes to heap when n is not a compile-time constant
	var sum any = int64(0)  // interface boxing escapes to heap
	for i := 0; i < n; i++ {
		buf[i] = seed + int64(i*3)
		sum = sum.(int64) + buf[i]
	}
	return sum.(int64)
}

// StackCertifiedSensorWindow represents `hy-emit`'s #[no_heap] lowering for a bounded
// lexical loan: a fixed-capacity `[64]int64` stack array sliced as `stackBuf[:n:64]`
// with monomorphized value arithmetic. Zero heap allocations (`0 B/op, 0 allocs/op`).
func StackCertifiedSensorWindow(_ NoHeapToken, n int, seed int64) int64 {
	if n < 0 || n > 64 {
		n = 64
	}
	var stackBuf [64]int64
	window := stackBuf[:n:64]
	var sum int64
	for i := 0; i < len(window); i++ {
		window[i] = seed + int64(i*3)
		sum += window[i]
	}
	return sum
}

// VerifyGcflagsEscapeOutput parses `go build -gcflags="-m=2"` stderr and enforces HYB-E3703.
func VerifyGcflagsEscapeOutput(stderr string, startLine, endLine int) []string {
	var violations []string
	for _, raw := range strings.Split(stderr, "\n") {
		line := strings.TrimSpace(raw)
		if !strings.Contains(line, "escapes to heap") && !strings.Contains(line, "moved to heap:") {
			continue
		}
		parts := strings.Split(line, ":")
		if len(parts) < 2 {
			continue
		}
		lineNo, err := strconv.Atoi(parts[1])
		if err != nil {
			continue
		}
		if lineNo >= startLine && lineNo <= endLine {
			violations = append(violations, fmt.Sprintf("HYB-E3703: line %d: %s", lineNo, line))
		}
	}
	return violations
}

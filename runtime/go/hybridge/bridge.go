// Package hybridge — the Hybrid runtime's boundary core.
package hybridge

import (
	"sync/atomic"
)

var DeepPathCount atomic.Uint64
var DeepCrossCount atomic.Uint64

func SPHeadroomForTest() uintptr {
	return 8192
}

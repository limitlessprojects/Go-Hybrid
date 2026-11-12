#include "textflag.h"

// ABI0 arguments/results are stack slots, even with //go:noescape.
TEXT ·identity(SB), NOSPLIT, $0-16
	MOVD value+0(FP), R0
	MOVD R0, ret+8(FP)
	RET

// Each function declares 96 local bytes, below the proposal's 128-byte
// per-function threshold. All three local frames are live at the leaf.
TEXT ·frameRoot(SB), NOSPLIT, $96-0
	CALL ·frameMiddle(SB)
	RET

TEXT ·frameMiddle(SB), NOSPLIT, $96-0
	CALL ·frameLeaf(SB)
	RET

TEXT ·frameLeaf(SB), NOSPLIT, $96-0
	RET

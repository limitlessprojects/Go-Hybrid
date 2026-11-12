#include "textflag.h"

// Deliberately invalid for an ordinary external Go package: probe A.
TEXT ·identity<ABIInternal>(SB), NOSPLIT, $0-0
	RET

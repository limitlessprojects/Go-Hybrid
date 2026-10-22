package regexbinding

import (
	"hybrid.program/hybridge"
)

const abiWordExpected = 0x68795f61626931

func init() {
	bridge, err := hybridge.GetWasmBridge()
	if err != nil || bridge.HyAbiWord() != abiWordExpected {
		panic("regexbinding: hyrt ABI handshake mismatch")
	}
}

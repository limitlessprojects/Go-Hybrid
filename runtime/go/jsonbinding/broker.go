package jsonbinding

import (
	"hybrid.program/hybridge"
)

const abiWordExpected = 0x68795f61626931

const (
	statusOK       = 0
	statusTryAgain = 3
)

func init() {
	bridge, err := hybridge.GetWasmBridge()
	if err != nil || bridge.HyAbiWord() != abiWordExpected {
		panic("jsonbinding: hyrt ABI handshake mismatch (rebuild the runtime)")
	}
}

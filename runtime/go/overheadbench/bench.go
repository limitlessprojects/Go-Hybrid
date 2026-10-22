package overheadbench

import (
	"hybrid.program/hybridge"
)

const abiWordExpected = 0x68795f61626931

func init() {
	bridge, err := hybridge.GetWasmBridge()
	if err != nil || bridge.HyAbiWord() != abiWordExpected {
		panic("overheadbench: hyrt ABI handshake mismatch")
	}
}

func Nop(x uint64) uint64 {
	bridge, err := hybridge.GetWasmBridge()
	if err != nil {
		return 0
	}
	return bridge.HyNop(x)
}

func NopRaw(x uint64) (uint64, bool) {
	v := Nop(x)
	return v, true
}

func DeepPathCount() uint64 {
	return 0
}

package sha2binding

import (
	"errors"

	"hybrid.program/hybridge"
)

var ErrSha2 = errors.New("sha2: digest failed")

const abiWordExpected = 0x68795f61626931

func init() {
	bridge, err := hybridge.GetWasmBridge()
	if err != nil || bridge.HyAbiWord() != abiWordExpected {
		panic("sha2binding: hyrt ABI handshake mismatch")
	}
}

func Digest256(data []byte) ([]byte, error) {
	bridge, err := hybridge.GetWasmBridge()
	if err != nil {
		return nil, ErrSha2
	}

	arena := hybridge.GetOwnHandle(len(data) + 64)
	defer hybridge.ReleaseOwnHandle(arena)

	var dataPtr uint32
	if len(data) > 0 {
		var inputSlice []byte
		dataPtr, inputSlice, err = arena.Alloc(len(data))
		if err != nil {
			return nil, ErrSha2
		}
		copy(inputSlice, data)
		bridge.Memory().Write(dataPtr, inputSlice)
	}

	outPtr, _, err := arena.Alloc(32)
	if err != nil {
		return nil, ErrSha2
	}

	st, err := bridge.HySha2256(dataPtr, uint32(len(data)), outPtr)
	if err != nil || st != 0 {
		return nil, ErrSha2
	}

	outBytes, ok := bridge.Memory().Read(outPtr, 32)
	if !ok {
		return nil, ErrSha2
	}

	res := make([]byte, 32)
	copy(res, outBytes)
	return res, nil
}

package sha3binding

import (
	"errors"

	"hybrid.program/hybridge"
)

var (
	ErrDigest = errors.New("sha3: digest256 failed")
	ErrABI    = errors.New("sha3: hyrt ABI handshake mismatch (rebuild the runtime)")
)

const abiWordExpected = 0x68795f61626931

func init() {
	bridge, err := hybridge.GetWasmBridge()
	if err != nil || bridge.HyAbiWord() != abiWordExpected {
		panic(ErrABI.Error())
	}
}

func Digest256(data []byte) ([]byte, error) {
	bridge, err := hybridge.GetWasmBridge()
	if err != nil {
		return nil, ErrDigest
	}

	arena := hybridge.GetOwnHandle(len(data) + 32 + 64)
	defer hybridge.ReleaseOwnHandle(arena)

	var dataPtr uint32
	if len(data) > 0 {
		var inputSlice []byte
		dataPtr, inputSlice, err = arena.Alloc(len(data))
		if err != nil {
			return nil, ErrDigest
		}
		copy(inputSlice, data)
		bridge.Memory().Write(dataPtr, inputSlice)
	}

	outPtr, _, err := arena.Alloc(32)
	if err != nil {
		return nil, ErrDigest
	}

	st, err := bridge.HySha3Digest256(dataPtr, uint32(len(data)), outPtr)
	if err != nil || st != 0 {
		return nil, ErrDigest
	}

	outBytes, ok := bridge.Memory().Read(outPtr, 32)
	if !ok {
		return nil, ErrDigest
	}

	res := make([]byte, 32)
	copy(res, outBytes)
	return res, nil
}

func StreamChecksum(iters, seed uint64) (uint64, error) {
	bridge, err := hybridge.GetWasmBridge()
	if err != nil {
		return 0, ErrDigest
	}
	return bridge.HySha3StreamChecksum(iters, seed)
}

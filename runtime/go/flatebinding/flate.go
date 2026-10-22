package flatebinding

import (
	"encoding/binary"
	"errors"

	"hybrid.program/hybridge"
)

var (
	ErrFlate = errors.New("flate: compression or decompression failed")
)

const abiWordExpected = 0x68795f61626931

func init() {
	bridge, err := hybridge.GetWasmBridge()
	if err != nil || bridge.HyAbiWord() != abiWordExpected {
		panic("flatebinding: hyrt ABI handshake mismatch")
	}
}

func Compress(data []byte) ([]byte, error) {
	bridge, err := hybridge.GetWasmBridge()
	if err != nil {
		return nil, ErrFlate
	}

	capVal := len(data) + 128
	arena := hybridge.GetOwnHandle(len(data) + capVal + 64)
	defer hybridge.ReleaseOwnHandle(arena)

	var dataPtr uint32
	if len(data) > 0 {
		var inputSlice []byte
		dataPtr, inputSlice, err = arena.Alloc(len(data))
		if err != nil {
			return nil, ErrFlate
		}
		copy(inputSlice, data)
		bridge.Memory().Write(dataPtr, inputSlice)
	}

	outPtr, _, err := arena.Alloc(capVal)
	if err != nil {
		return nil, ErrFlate
	}

	outLenPtr, _, err := arena.Alloc(8)
	if err != nil {
		return nil, ErrFlate
	}

	st, err := bridge.HyFlateCompress(dataPtr, uint32(len(data)), outPtr, uint32(capVal), outLenPtr)
	if st == 3 {
		lenBytes, ok := bridge.Memory().Read(outLenPtr, 8)
		if !ok {
			return nil, ErrFlate
		}
		need := int(binary.LittleEndian.Uint64(lenBytes))
		capVal = need

		bigArena := hybridge.GetOwnHandle(len(data) + need + 64)
		defer hybridge.ReleaseOwnHandle(bigArena)

		if len(data) > 0 {
			dataPtr, _, _ = bigArena.Alloc(len(data))
			bridge.Memory().Write(dataPtr, data)
		}
		outPtr, _, _ = bigArena.Alloc(need)
		outLenPtr, _, _ = bigArena.Alloc(8)

		st, err = bridge.HyFlateCompress(dataPtr, uint32(len(data)), outPtr, uint32(need), outLenPtr)
	}

	if err != nil || st != 0 {
		return nil, ErrFlate
	}

	lenBytes, ok := bridge.Memory().Read(outLenPtr, 8)
	if !ok {
		return nil, ErrFlate
	}
	outLen := binary.LittleEndian.Uint64(lenBytes)

	readBytes, ok := bridge.Memory().Read(outPtr, uint32(outLen))
	if !ok {
		return nil, ErrFlate
	}

	res := make([]byte, outLen)
	copy(res, readBytes)
	return res, nil
}

func Decompress(data []byte) ([]byte, error) {
	bridge, err := hybridge.GetWasmBridge()
	if err != nil {
		return nil, ErrFlate
	}

	capVal := len(data) * 4
	if capVal < 256 {
		capVal = 256
	}
	arena := hybridge.GetOwnHandle(len(data) + capVal + 64)
	defer hybridge.ReleaseOwnHandle(arena)

	var dataPtr uint32
	if len(data) > 0 {
		var inputSlice []byte
		dataPtr, inputSlice, err = arena.Alloc(len(data))
		if err != nil {
			return nil, ErrFlate
		}
		copy(inputSlice, data)
		bridge.Memory().Write(dataPtr, inputSlice)
	}

	outPtr, _, err := arena.Alloc(capVal)
	if err != nil {
		return nil, ErrFlate
	}

	outLenPtr, _, err := arena.Alloc(8)
	if err != nil {
		return nil, ErrFlate
	}

	st, err := bridge.HyFlateDecompress(dataPtr, uint32(len(data)), outPtr, uint32(capVal), outLenPtr)
	if st == 3 {
		lenBytes, ok := bridge.Memory().Read(outLenPtr, 8)
		if !ok {
			return nil, ErrFlate
		}
		need := int(binary.LittleEndian.Uint64(lenBytes))
		capVal = need

		bigArena := hybridge.GetOwnHandle(len(data) + need + 64)
		defer hybridge.ReleaseOwnHandle(bigArena)

		if len(data) > 0 {
			dataPtr, _, _ = bigArena.Alloc(len(data))
			bridge.Memory().Write(dataPtr, data)
		}
		outPtr, _, _ = bigArena.Alloc(need)
		outLenPtr, _, _ = bigArena.Alloc(8)

		st, err = bridge.HyFlateDecompress(dataPtr, uint32(len(data)), outPtr, uint32(need), outLenPtr)
	}

	if err != nil || st != 0 {
		return nil, ErrFlate
	}

	lenBytes, ok := bridge.Memory().Read(outLenPtr, 8)
	if !ok {
		return nil, ErrFlate
	}
	outLen := binary.LittleEndian.Uint64(lenBytes)

	readBytes, ok := bridge.Memory().Read(outPtr, uint32(outLen))
	if !ok {
		return nil, ErrFlate
	}

	res := make([]byte, outLen)
	copy(res, readBytes)
	return res, nil
}

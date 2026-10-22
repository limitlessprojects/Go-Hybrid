package ed25519binding

import (
	"errors"

	"hybrid.program/hybridge"
)

var ErrEd25519 = errors.New("ed25519: sign operation failed")

const abiWordExpected = 0x68795f61626931

func init() {
	bridge, err := hybridge.GetWasmBridge()
	if err != nil || bridge.HyAbiWord() != abiWordExpected {
		panic("ed25519binding: hyrt ABI handshake mismatch")
	}
}

func Sign(secretKey [32]byte, msg []byte) ([64]byte, error) {
	var sig [64]byte
	bridge, err := hybridge.GetWasmBridge()
	if err != nil {
		return sig, ErrEd25519
	}

	arena := hybridge.GetOwnHandle(32 + len(msg) + 64 + 64)
	defer hybridge.ReleaseOwnHandle(arena)

	secretPtr, secretSlice, err := arena.Alloc(32)
	if err != nil {
		return sig, ErrEd25519
	}
	copy(secretSlice, secretKey[:])
	bridge.Memory().Write(secretPtr, secretSlice)

	var msgPtr uint32
	if len(msg) > 0 {
		var msgSlice []byte
		msgPtr, msgSlice, err = arena.Alloc(len(msg))
		if err != nil {
			return sig, ErrEd25519
		}
		copy(msgSlice, msg)
		bridge.Memory().Write(msgPtr, msgSlice)
	}

	outSigPtr, _, err := arena.Alloc(64)
	if err != nil {
		return sig, ErrEd25519
	}

	st, err := bridge.HyEd25519Sign(secretPtr, msgPtr, uint32(len(msg)), outSigPtr)
	if err != nil || st != 0 {
		return sig, ErrEd25519
	}

	sigBytes, ok := bridge.Memory().Read(outSigPtr, 64)
	if !ok {
		return sig, ErrEd25519
	}

	copy(sig[:], sigBytes)
	return sig, nil
}

func Verify(pubKey [32]byte, msg []byte, sig [64]byte) bool {
	bridge, err := hybridge.GetWasmBridge()
	if err != nil {
		return false
	}

	arena := hybridge.GetOwnHandle(32 + len(msg) + 64 + 64)
	defer hybridge.ReleaseOwnHandle(arena)

	pkPtr, pkSlice, err := arena.Alloc(32)
	if err != nil {
		return false
	}
	copy(pkSlice, pubKey[:])
	bridge.Memory().Write(pkPtr, pkSlice)

	var msgPtr uint32
	if len(msg) > 0 {
		var msgSlice []byte
		msgPtr, msgSlice, err = arena.Alloc(len(msg))
		if err != nil {
			return false
		}
		copy(msgSlice, msg)
		bridge.Memory().Write(msgPtr, msgSlice)
	}

	sigPtr, sigSlice, err := arena.Alloc(64)
	if err != nil {
		return false
	}
	copy(sigSlice, sig[:])
	bridge.Memory().Write(sigPtr, sigSlice)

	st, err := bridge.HyEd25519Verify(pkPtr, msgPtr, uint32(len(msg)), sigPtr)
	if err != nil {
		return false
	}
	return st == 1
}

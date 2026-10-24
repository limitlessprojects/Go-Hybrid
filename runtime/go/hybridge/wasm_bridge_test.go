package hybridge

import (
	"encoding/hex"
	"testing"
)

func TestWasmBridgeInitAndSha3(t *testing.T) {
	bridge, err := GetWasmBridge()
	if err != nil {
		t.Fatalf("failed to initialize WasmBridge: %v", err)
	}

	word := bridge.HyAbiWord()
	if word != 0x68795f61626931 {
		t.Fatalf("unexpected ABI word: 0x%x", word)
	}

	nopRes := bridge.HyNop(12345)
	if nopRes != 12345 {
		t.Fatalf("expected 12345, got %d", nopRes)
	}

	input := []byte("abc")
	inputPtr, err := bridge.Alloc(uint32(len(input)))
	if err != nil {
		t.Fatalf("alloc input failed: %v", err)
	}
	defer bridge.Free(inputPtr, uint32(len(input)))

	bridge.Memory().Write(inputPtr, input)

	outPtr, err := bridge.Alloc(32)
	if err != nil {
		t.Fatalf("alloc out failed: %v", err)
	}
	defer bridge.Free(outPtr, 32)

	st, err := bridge.HySha3Digest256(inputPtr, uint32(len(input)), outPtr)
	if err != nil || st != 0 {
		t.Fatalf("HySha3Digest256 failed: status=%d err=%v", st, err)
	}

	gotBytes, ok := bridge.Memory().Read(outPtr, 32)
	if !ok {
		t.Fatal("failed to read SHA3 output from Wasm memory")
	}

	wantHex := "3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532"
	if h := hex.EncodeToString(gotBytes); h != wantHex {
		t.Fatalf("SHA3 digest = %s, want %s", h, wantHex)
	}
}

func TestWasmBridgeSha2(t *testing.T) {
	bridge, err := GetWasmBridge()
	if err != nil {
		t.Fatalf("failed to initialize WasmBridge: %v", err)
	}

	input := []byte("abc")
	inputPtr, err := bridge.Alloc(uint32(len(input)))
	if err != nil {
		t.Fatalf("alloc input failed: %v", err)
	}
	defer bridge.Free(inputPtr, uint32(len(input)))

	bridge.Memory().Write(inputPtr, input)

	outPtr, err := bridge.Alloc(32)
	if err != nil {
		t.Fatalf("alloc out failed: %v", err)
	}
	defer bridge.Free(outPtr, 32)

	st, err := bridge.HySha2256(inputPtr, uint32(len(input)), outPtr)
	if err != nil || st != 0 {
		t.Fatalf("HySha2256 failed: status=%d err=%v", st, err)
	}

	gotBytes, ok := bridge.Memory().Read(outPtr, 32)
	if !ok {
		t.Fatal("failed to read SHA2 output from Wasm memory")
	}

	wantHex := "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
	if h := hex.EncodeToString(gotBytes); h != wantHex {
		t.Fatalf("SHA2 digest = %s, want %s", h, wantHex)
	}
}

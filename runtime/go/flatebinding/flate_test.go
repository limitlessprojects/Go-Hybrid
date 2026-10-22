package flatebinding

import (
	"bytes"
	"testing"
)

func TestFlateRoundTrip(t *testing.T) {
	original := []byte("The quick brown fox jumps over the lazy dog. Hybrid pure Go Wasm bridge flate2 compression test!")

	compressed, err := Compress(original)
	if err != nil {
		t.Fatalf("Compress failed: %v", err)
	}
	if len(compressed) == 0 {
		t.Fatal("compressed bytes empty")
	}

	decompressed, err := Decompress(compressed)
	if err != nil {
		t.Fatalf("Decompress failed: %v", err)
	}

	if !bytes.Equal(original, decompressed) {
		t.Fatalf("decompressed mismatch!\nwant: %s\ngot:  %s", string(original), string(decompressed))
	}
}

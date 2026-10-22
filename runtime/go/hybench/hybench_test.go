package hybench

import (
	"crypto/rand"
	"testing"

	"hybrid.program/flatebinding"
	"hybrid.program/hybridge"
	"hybrid.program/sha3binding"
)

// Gate Criterion 1: Small-call overhead <= 15 ns
func BenchmarkSmallCallNop(b *testing.B) {
	bridge, err := hybridge.GetWasmBridge()
	if err != nil {
		b.Fatalf("failed to initialize Wasm bridge: %v", err)
	}

	b.ResetTimer()
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		_ = bridge.HyNop(uint64(i))
	}
}

// Gate Criterion 2: Streaming Payload Throughput (SHA3-256 / Checksum)
func BenchmarkSha3Digest256Small(b *testing.B) {
	data := make([]byte, 64)
	rand.Read(data)

	b.ResetTimer()
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		_, _ = sha3binding.Digest256(data)
	}
}

func BenchmarkSha3StreamChecksumLarge(b *testing.B) {
	b.ResetTimer()
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		_, _ = sha3binding.StreamChecksum(1000, 42)
	}
}

func BenchmarkFlateCompressLargePayload(b *testing.B) {
	payload := make([]byte, 1024*1024) // 1 MiB payload
	rand.Read(payload)

	b.SetBytes(int64(len(payload)))
	b.ResetTimer()
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		_, _ = flatebinding.Compress(payload)
	}
}

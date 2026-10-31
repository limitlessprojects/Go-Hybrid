package noheap

import "testing"

func TestP1_ZeroAllocStackCertifiedLowering(t *testing.T) {
	tok := SynthesizeNoHeapToken()
	allocs := testing.AllocsPerRun(1000, func() {
		res := StackCertifiedSensorWindow(tok, 64, 42)
		if res == 0 {
			t.Fatal("unexpected zero sum")
		}
	})
	if allocs != 0 {
		t.Fatalf("HYB-E3703: expected 0 allocs/op for StackCertifiedSensorWindow, got %v", allocs)
	}
}

func TestP2_EscapeOracleRejectsHeapSpill(t *testing.T) {
	mockOutput := "./noheap.go:26:13: make([]int64, n) escapes to heap\n./noheap.go:40:6: &stackBuf does not escape\n"
	vNaive := VerifyGcflagsEscapeOutput(mockOutput, 24, 34)
	if len(vNaive) != 1 {
		t.Fatalf("expected 1 HYB-E3703 violation on NaiveEmitSensorWindow, got %v", vNaive)
	}
	vCert := VerifyGcflagsEscapeOutput(mockOutput, 36, 52)
	if len(vCert) != 0 {
		t.Fatalf("expected 0 violations on StackCertifiedSensorWindow, got %v", vCert)
	}
}

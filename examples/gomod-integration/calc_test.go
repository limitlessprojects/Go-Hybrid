package main

import "testing"

// The generated Go is covered by ORDINARY Go tests (row 13's constraint):
// no separate test runner, no separate toolchain culture.
func TestSumConcurrent(t *testing.T) {
	got := SumConcurrent([]int{2, 3, 5, 7, 11, 13}, 4)
	if got != 41 {
		t.Fatalf("SumConcurrent = %d, want 41", got)
	}
	if got := SumConcurrent(nil, 4); got != 0 {
		t.Fatalf("SumConcurrent(nil) = %d, want 0", got)
	}
	if got := SumConcurrent([]int{5}, 0); got != 0 {
		t.Fatalf("SumConcurrent(shards=0) = %d, want 0", got)
	}
}

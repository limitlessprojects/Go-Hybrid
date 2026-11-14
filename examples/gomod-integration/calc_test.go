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

func TestSumConcurrentTable(t *testing.T) {
	tests := []struct {
		name   string
		xs     []int
		shards int
		want   int
	}{
		{"empty_slice", []int{}, 4, 0},
		{"nil_slice", nil, 4, 0},
		{"zero_shards", []int{1, 2, 3}, 0, 0},
		{"negative_shards", []int{1, 2, 3}, -1, 0},
		{"single_element_single_shard", []int{42}, 1, 42},
		{"single_element_multiple_shards", []int{5}, 4, 5},
		{"prime_numbers_4_shards", []int{2, 3, 5, 7, 11, 13}, 4, 41},
		{"negative_numbers", []int{-5, 10, -20, 30}, 2, 15},
		{"large_shard_count_exceeding_len", []int{1, 2, 3}, 10, 6},
		{"many_elements", []int{1, 2, 3, 4, 5, 6, 7, 8, 9, 10}, 3, 55},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			got := SumConcurrent(tc.xs, tc.shards)
			if got != tc.want {
				t.Fatalf("SumConcurrent(%v, %d) = %d, want %d", tc.xs, tc.shards, got, tc.want)
			}
		})
	}
}

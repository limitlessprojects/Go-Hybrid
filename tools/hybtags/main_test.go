package main

import (
	"reflect"
	"sort"
	"testing"
)

func TestParseTags(t *testing.T) {
	tests := []struct {
		in   string
		want []string
	}{
		{"", nil},
		{"   ", nil},
		{"fast", []string{"fast"}},
		{"fast,small", []string{"fast", "small"}},
		{" fast , small , debug ", []string{"fast", "small", "debug"}},
		{"fast,fast,small", []string{"fast", "small"}},
		{"fast,,small", []string{"fast", "small"}},
	}
	for _, tt := range tests {
		got := parseTags(tt.in)
		if !reflect.DeepEqual(got, tt.want) {
			t.Errorf("parseTags(%q) = %v, want %v", tt.in, got, tt.want)
		}
	}
}

func TestEnumerateCombinations(t *testing.T) {
	tags := []string{"fast", "small"}
	combos := enumerateCombinations(tags)

	if len(combos) != 4 {
		t.Fatalf("expected 4 combinations, got %d", len(combos))
	}

	// all-off first
	if len(combos[0].Tags) != 0 {
		t.Errorf("first combination should have no tags, got %v", combos[0].Tags)
	}

	// all-on last
	if len(combos[3].Tags) != 2 || combos[3].Tags[0] != "fast" || combos[3].Tags[1] != "small" {
		t.Errorf("last combination should have both tags, got %v", combos[3].Tags)
	}

	// every combination has correct membership
	for _, c := range combos {
		for _, tag := range tags {
			if c.Active[tag] {
				found := false
				for _, t := range c.Tags {
					if t == tag {
						found = true
						break
					}
				}
				if !found {
					t.Errorf("combination %v marks %q active but it is not in Tags", c.Tags, tag)
				}
			}
		}
	}
}

func TestGroupByFiles(t *testing.T) {
	sel := []fileSelection{
		{combo: Combination{Tags: []string{"fast"}, Active: map[string]bool{"fast": true}}, files: []string{"a.go", "b.go"}},
		{combo: Combination{Tags: []string{"small"}, Active: map[string]bool{"small": true}}, files: []string{"a.go", "b.go"}},
		{combo: Combination{Tags: []string{"debug"}, Active: map[string]bool{"debug": true}}, files: []string{"a.go", "c.go"}},
		{combo: Combination{Tags: []string{}, Active: map[string]bool{}}, files: []string{"a.go"}},
	}

	groups := groupByFiles(sel)

	if len(groups) != 3 {
		t.Fatalf("expected 3 groups, got %d", len(groups))
	}

	// first two selections share a group (identical files)
	var multiGroup *group
	for _, g := range groups {
		if len(g.combinations) == 2 {
			multiGroup = g
		}
	}
	if multiGroup == nil {
		t.Fatal("expected a group with 2 combinations")
	}
	if len(multiGroup.files) != 2 {
		t.Errorf("expected 2 files in shared group, got %d", len(multiGroup.files))
	}
}

func TestFilesKey(t *testing.T) {
	k1 := filesKey([]string{"a.go", "b.go"})
	k2 := filesKey([]string{"b.go", "a.go"})
	if k1 != k2 {
		t.Error("filesKey should be order-independent")
	}
	k3 := filesKey([]string{"a.go", "c.go"})
	if k1 == k3 {
		t.Error("different file sets should produce different keys")
	}
}

// sortedStrings returns a sorted copy for comparison in tests.
func sortedStrings(s []string) []string {
	out := append([]string(nil), s...)
	sort.Strings(out)
	return out
}

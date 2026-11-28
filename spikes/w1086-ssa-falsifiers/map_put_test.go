package ssafalsifiers

import "testing"

func mapPutInPlace(m map[string]int, key string, value int) map[string]int {
	m[key] = value
	return m
}

func mapPutCopy(m map[string]int, key string, value int) map[string]int {
	out := make(map[string]int, len(m)+1)
	for k, v := range m {
		out[k] = v
	}
	out[key] = value
	return out
}

func TestInPlaceUpdateChangesOldSSAVersion(t *testing.T) {
	m0 := map[string]int{"k": 1}
	m1 := mapPutInPlace(m0, "k", 2)
	if m0["k"] != 2 || m1["k"] != 2 {
		t.Fatal("expected the in-place counterexample to mutate both versions")
	}
	t.Log("F3: distinct SSA names m0/m1 share storage; old version observes 2")
}

func TestCopyUpdatePreservesOldSSAVersion(t *testing.T) {
	m0 := map[string]int{"k": 1}
	m1 := mapPutCopy(m0, "k", 2)
	if m0["k"] != 1 || m1["k"] != 2 {
		t.Fatal("copy control failed to preserve old version")
	}
	t.Log("control: copy helper preserves old version at 1")
}

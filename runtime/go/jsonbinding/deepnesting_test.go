package jsonbinding

import (
	"errors"
	"strings"
	"testing"
)

const maxDepth = 127

func deepArray(n int) []byte {
	return []byte(strings.Repeat("[", n) + strings.Repeat("]", n))
}

func TestSerdeDepthCeiling(t *testing.T) {
	if _, err := Parse(deepArray(maxDepth)); err != nil {
		t.Fatalf("depth %d must parse: %v", maxDepth, err)
	}
	if _, err := Parse(deepArray(maxDepth + 1)); err == nil {
		t.Fatalf("depth %d must FAIL", maxDepth+1)
	}
}

func TestDeepNestingParsesViaPureGo(t *testing.T) {
	v, err := Parse(deepArray(maxDepth))
	if err != nil {
		t.Fatalf("Parse(%d-deep) failed: %v", maxDepth, err)
	}

	s, err := ToStringPretty(v)
	if err != nil {
		t.Fatalf("ToStringPretty(%d-deep) failed: %v", maxDepth, err)
	}
	if len(s) < 2*maxDepth {
		t.Fatalf("round-trip length = %d, want >= %d", len(s), 2*maxDepth)
	}
	v.Release()
}

func TestDeepNestingOnConstrainedStack(t *testing.T) {
	done := make(chan error, 1)
	go func() {
		var pad [1024]byte
		var grow func(depth int) byte
		grow = func(depth int) byte {
			if depth == 0 {
				return pad[0]
			}
			pad[depth%1024]++
			return grow(depth-1) + pad[(depth+1)%1024]
		}
		_ = grow(30)

		v, err := Parse(deepArray(maxDepth))
		if err != nil {
			done <- errors.New("step parse: " + err.Error())
			return
		}
		if _, err := ToStringPretty(v); err != nil {
			done <- errors.New("step pretty: " + err.Error())
			return
		}
		arr, err := v.Array("nope")
		if err == nil || arr != nil {
			done <- errors.New("absent key must error")
			return
		}
		v.Release()
		done <- nil
	}()
	if err := <-done; err != nil {
		t.Fatalf("deep-nesting work on a constrained stack failed: %v", err)
	}
}

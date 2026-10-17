package regexbinding

import (
	"fmt"
	"strings"
	"sync"
	"testing"

	"hybrid.program/hybridge"
)

// The Batch-H lesson, pinned: regex patterns cross as RAW strings — `\s`
// is not a hy escape. These tests cross into the real hyrt regex wrapper
// (pinned regex =1.11.1).

func TestIsMatchRawClasses(t *testing.T) {
	cases := []struct {
		pattern, text string
		want          bool
	}{
		{`\s+`, "   ", true},
		{`\s+`, "x", false},
		{`^\d{3}-\d{4}$`, "555-1234", true},
		{`^\d{3}-\d{4}$`, "5551234", false},
		{`^$`, "", true},
		{"b", "cat", false},
	}
	for _, c := range cases {
		got, err := IsMatch(c.pattern, c.text)
		if err != nil {
			t.Fatalf("IsMatch(%q, %q): %v", c.pattern, c.text, err)
		}
		if got != c.want {
			t.Fatalf("IsMatch(%q, %q) = %v, want %v", c.pattern, c.text, got, c.want)
		}
	}
}

func TestIsValid(t *testing.T) {
	if !IsValid(`a+\s?`) {
		t.Fatal("IsValid(a+\\s?) = false, want true")
	}
	if IsValid("(") {
		t.Fatal("IsValid(() = true, want false (unclosed group)")
	}
	if IsValid(`[a-`) {
		t.Fatal("IsValid([a-) = true, want false (bad class range)")
	}
}

func TestInvalidPatternErrors(t *testing.T) {
	// hyrt's is_match returns status -1 on a regex error; the binding
	// surfaces it as an error, never a silent false.
	if _, err := IsMatch("(", "x"); err == nil {
		t.Fatal("IsMatch with an invalid pattern must error")
	}
}

func TestDeepClassAndConstrainedStack(t *testing.T) {
	// Both IsMatch and IsValid compile the pattern via regex::Regex::new on
	// every call (regex_automata::meta::strategy::new has a 7,648 B single
	// frame and up to 22,976 B compile chain, plus recursive Ast/ClassSet drop
	// on nested character classes). Verify (1) both routes increment
	// DeepCrossCount and (2) concurrent goroutines calling IsMatch/IsValid at
	// SPHeadroom in (4150, 5200) — above the 4096 B DEFAULT guard where Cross4/
	// Cross2 used to take the fast path and overflow into sibling goroutine
	// stacks — succeed cleanly on the 1 MiB pooled deep stack.
	pat := strings.Repeat("[", 200) + "abc|def|ghi" + strings.Repeat("]", 200)

	before := hybridge.DeepCrossCount.Load()
	if !IsValid(pat) {
		t.Fatal("IsValid(nested class) = false, want true")
	}
	afterValid := hybridge.DeepCrossCount.Load()
	if afterValid == before {
		t.Fatal("IsValid did not take the ALWAYS-deep route (CrossDeep2)")
	}
	ok, err := IsMatch(pat, "a")
	if err != nil || !ok {
		t.Fatalf("IsMatch(nested class) = %v, %v", ok, err)
	}
	if hybridge.DeepCrossCount.Load() == afterValid {
		t.Fatal("IsMatch did not take the ALWAYS-deep route (CrossDeep4)")
	}

	const workers = 16
	var wg sync.WaitGroup
	errCh := make(chan error, workers)
	for w := 0; w < workers; w++ {
		wg.Add(1)
		go func(id int) {
			defer wg.Done()
			var grow func(int) [256]byte
			grow = func(n int) [256]byte {
				var b [256]byte
				b[0] = byte(n + id)
				if n > 0 {
					inner := grow(n - 1)
					b[1] = inner[0]
				}
				return b
			}
			_ = grow(28)

			var descend func(int) byte
			descend = func(depth int) byte {
				var frame [256]byte
				for i := range frame {
					frame[i] = byte(id + depth + i)
				}
				h := hybridge.SPHeadroomForTest()
				if h > 4150 && h < 5200 {
					for iter := 0; iter < 5; iter++ {
						if !IsValid(pat) {
							errCh <- fmt.Errorf("worker %d: IsValid=false", id)
							return 0
						}
						matched, err := IsMatch(pat, "a")
						if err != nil || !matched {
							errCh <- fmt.Errorf("worker %d: IsMatch=%v err=%v", id, matched, err)
							return 0
						}
					}
					return frame[0]
				}
				if h >= 5200 && depth < 100 {
					res := descend(depth + 1)
					for i := range frame {
						if frame[i] != byte(id+depth+i) {
							errCh <- fmt.Errorf("stack corruption at depth %d", depth)
							return 0
						}
					}
					return res
				}
				return frame[0]
			}
			_ = descend(0)
		}(w)
	}
	wg.Wait()
	close(errCh)
	for err := range errCh {
		t.Fatal(err)
	}
}

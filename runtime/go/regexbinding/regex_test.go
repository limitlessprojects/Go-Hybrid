package regexbinding

import (
	"fmt"
	"strings"
	"sync"
	"testing"
)

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
	if _, err := IsMatch("(", "x"); err == nil {
		t.Fatal("IsMatch with an invalid pattern must error")
	}
}

func TestDeepClassAndConstrainedStack(t *testing.T) {
	pat := strings.Repeat("[", 20) + "abc|def|ghi" + strings.Repeat("]", 20)

	if !IsValid(pat) {
		t.Fatal("IsValid(nested class) = false, want true")
	}
	ok, err := IsMatch(pat, "a")
	if err != nil || !ok {
		t.Fatalf("IsMatch(nested class) = %v, %v", ok, err)
	}

	const workers = 8
	var wg sync.WaitGroup
	errCh := make(chan error, workers)
	for w := 0; w < workers; w++ {
		wg.Add(1)
		go func(id int) {
			defer wg.Done()
			var pad [1024]byte
			for i := range pad {
				pad[i] = byte(id + i)
			}
			if !IsValid(pat) {
				errCh <- fmt.Errorf("worker %d: IsValid=false", id)
				return
			}
			matched, err := IsMatch(pat, "a")
			if err != nil || !matched {
				errCh <- fmt.Errorf("worker %d: IsMatch=%v err=%v", id, matched, err)
				return
			}
		}(w)
	}
	wg.Wait()
	close(errCh)
	for err := range errCh {
		t.Fatal(err)
	}
}

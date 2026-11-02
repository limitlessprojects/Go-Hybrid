package regexbinding_test

import (
	"errors"
	"testing"

	"hybrid.program/hybridge"
	"hybrid.program/regexbinding"
)

func TestInvalidUTF8RejectedBeforeDeepCrossing(t *testing.T) {
	bad := string([]byte{0xff, 0xfe})

	before := hybridge.DeepCrossCount.Load()
	if regexbinding.IsValid(bad) {
		t.Fatal("IsValid(invalid UTF-8) = true, want false")
	}
	if _, err := regexbinding.IsMatch(bad, "ok"); !errors.Is(err, regexbinding.ErrRegex) {
		t.Fatalf("IsMatch(invalid UTF-8 pattern) = %v, want ErrRegex", err)
	}
	if _, err := regexbinding.IsMatch(`^\w+$`, bad); !errors.Is(err, regexbinding.ErrRegex) {
		t.Fatalf("IsMatch(invalid UTF-8 text) = %v, want ErrRegex", err)
	}
	if after := hybridge.DeepCrossCount.Load(); after != before {
		t.Fatalf("DeepCrossCount advanced %d -> %d on invalid UTF-8 inputs", before, after)
	}
}

func TestEmptyPatternAndEmptyTextEdgeCases(t *testing.T) {
	if !regexbinding.IsValid("") {
		t.Fatal("IsValid(\"\") = false, want true")
	}
	ok, err := regexbinding.IsMatch("", "")
	if err != nil || !ok {
		t.Fatalf("IsMatch(\"\", \"\") = %v, %v (want true, nil)", ok, err)
	}
	ok, err = regexbinding.IsMatch("", "nonempty")
	if err != nil || !ok {
		t.Fatalf("IsMatch(\"\", \"nonempty\") = %v, %v (want true, nil)", ok, err)
	}
}

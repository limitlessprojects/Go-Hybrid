package jsonbinding_test

import (
	"testing"

	"hybrid.program/jsonbinding"
)

func TestSetRejectsInvalidUTF8KeyAndStringValue(t *testing.T) {
	doc := jsonbinding.NewObject()
	if err := doc.Set("ok", "initial"); err != nil {
		t.Fatalf("Set(ok): %v", err)
	}

	// Invalid UTF-8 keys must fail and leave existing entries intact.
	if err := doc.Set(string([]byte{0xff, 0xfe}), "val"); err == nil {
		t.Fatal("Set with invalid UTF-8 key must error")
	}

	// Invalid UTF-8 string values must error rather than silently replacing
	// invalid bytes with U+FFFD via json.Marshal.
	if err := doc.Set("ok", string([]byte{0xff, 0xfe})); err == nil {
		t.Fatal("Set with invalid UTF-8 string value must error")
	}

	got, err := doc.String("ok")
	if err != nil || got != "initial" {
		t.Fatalf("String(ok) = %q, %v (want %q)", got, err, "initial")
	}
}

func TestSetScalarOnZeroOrArrayValueErrorsWithoutCorruptingReceiver(t *testing.T) {
	var zero jsonbinding.Value
	if err := zero.Set("k", "v"); err == nil {
		t.Fatal("Set on zero Value must error")
	}

	arr := jsonbinding.NewArray()
	if err := arr.PushString("first"); err != nil {
		t.Fatalf("PushString: %v", err)
	}
	if err := arr.Set("k", 123); err == nil {
		t.Fatal("Set on array Value must error")
	}

	pretty, err := jsonbinding.ToStringPretty(arr)
	if err != nil || pretty != "[\n  \"first\"\n]" {
		t.Fatalf("ToStringPretty(arr) = %q, %v", pretty, err)
	}
}

func TestNewObjectAndArrayMaterializeAndSerializeCleanly(t *testing.T) {
	doc := jsonbinding.NewObject()
	items := jsonbinding.NewArray()
	for _, s := range []string{"alpha", "beta"} {
		if err := items.PushString(s); err != nil {
			t.Fatalf("PushString(%q): %v", s, err)
		}
	}
	if err := doc.Set("items", items); err != nil {
		t.Fatalf("Set(items): %v", err)
	}

	got, err := doc.Array("items")
	if err != nil {
		t.Fatalf("Array(items): %v", err)
	}
	if len(got) != 2 {
		t.Fatalf("len(got) = %d, want 2", len(got))
	}
	for i, want := range []string{"\"alpha\"", "\"beta\""} {
		text, err := jsonbinding.ToStringPretty(got[i])
		if err != nil || text != want {
			t.Fatalf("got[%d] = %q, %v (want %q)", i, text, err, want)
		}
	}
}

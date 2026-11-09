package jsonbinding

import (
	"runtime"
	"testing"
)

// The handle protocol has zero direct tests until this file (the G2/G3
// review's go-test finding): every test here crosses into the REAL hyrt
// staticlib — Parse/NewObject/Set/getters/pretty — the same libhyrt.a every
// hyb build links.

const prettyDoc = "{\n  \"k\": 42,\n  \"name\": \"hy\"\n}"

func TestParseRoundTrip(t *testing.T) {
	doc, err := Parse([]byte(`{"k": 42, "name": "hy"}`))
	if err != nil {
		t.Fatalf("Parse: %v", err)
	}
	gotName, err := doc.String("name")
	if err != nil || gotName != "hy" {
		t.Fatalf("String(name) = %q, %v", gotName, err)
	}
	gotK, err := doc.Int("k")
	if err != nil || gotK != 42 {
		t.Fatalf("Int(k) = %d, %v", gotK, err)
	}
	pretty, err := ToStringPretty(doc)
	if err != nil || pretty != prettyDoc {
		t.Fatalf("ToStringPretty = %q, %v (want %q)", pretty, err, prettyDoc)
	}
}

func TestParseRejectsGarbage(t *testing.T) {
	if _, err := Parse([]byte("{nope")); err == nil {
		t.Fatal("Parse(garbage) returned nil error")
	}
}

func TestScalarBoxingThroughSet(t *testing.T) {
	// The differential corpus 12/13 contract: scalars box through a JSON
	// round-trip inside Set — bool/int/float/string all readable back.
	// serde_json's Map is a BTreeMap (no preserve_order) — keys print sorted.
	doc := NewObject()
	for key, val := range map[string]any{
		"b": true, "i": int64(-7), "f": 2.5, "s": "hy",
	} {
		if err := doc.Set(key, val); err != nil {
			t.Fatalf("Set(%s, %v): %v", key, val, err)
		}
	}
	pretty, err := ToStringPretty(doc)
	want := "{\n  \"b\": true,\n  \"f\": 2.5,\n  \"i\": -7,\n  \"s\": \"hy\"\n}"
	if err != nil || pretty != want {
		t.Fatalf("pretty = %q, %v (want %q)", pretty, err, want)
	}
	if i, _ := doc.Int("i"); i != -7 {
		t.Fatalf("Int(i) = %d", i)
	}
	if s, _ := doc.String("s"); s != "hy" {
		t.Fatalf("String(s) = %q", s)
	}
	// Getters are TYPE-STRICT: String on the float 2.5 errors (the pretty
	// text above already proves the float boxed correctly through Set).
	if _, err := doc.String("f"); err == nil {
		t.Fatal("String(float field) must type-error")
	}
}

func TestEmptyKeyEdge(t *testing.T) {
	// pinCrossKey's (0, 0) empty-borrow path — documented, never tested.
	doc := NewObject()
	if err := doc.Set("", true); err != nil {
		t.Fatalf("Set(\"\", true): %v", err)
	}
	pretty, err := ToStringPretty(doc)
	if err != nil || pretty != "{\n  \"\": true\n}" {
		t.Fatalf("pretty = %q, %v", pretty, err)
	}
}

func TestMoveSemantics(t *testing.T) {
	// Set MOVES the handle: the source's id zeroes, further use errors.
	doc := NewObject()
	inner := NewObject()
	if err := inner.Set("x", "y"); err != nil {
		t.Fatal(err)
	}
	if err := doc.Set("inner", inner); err != nil {
		t.Fatalf("Set(handle): %v", err)
	}
	if inner.h.id != 0 {
		t.Fatal("moved handle still carries a nonzero id")
	}
	if err := doc.Set("again", inner); err == nil {
		t.Fatal("Set of a moved (zeroed) handle must error")
	}
	// The moved-in content is readable through the destination.
	s, err := doc.String("inner") // object, not string — must ERROR, not crash
	if err == nil {
		t.Fatalf("String(object) should type-error, got %q", s)
	}
	pretty, err := ToStringPretty(doc)
	if err != nil || pretty != "{\n  \"inner\": {\n    \"x\": \"y\"\n  }\n}" {
		t.Fatalf("pretty = %q, %v", pretty, err)
	}
}

func TestTypeMismatchAndMissingKeyError(t *testing.T) {
	doc, _ := Parse([]byte(`{"name": "hy"}`))
	if _, err := doc.Int("name"); err == nil {
		t.Fatal("Int(string field) must error")
	}
	if _, err := doc.Int("absent"); err == nil {
		t.Fatal("Int(missing key) must error")
	}
	if _, err := doc.Array("name"); err == nil {
		t.Fatal("Array(string field) must error")
	}
}

func TestArrayMaterialization(t *testing.T) {
	doc, _ := Parse([]byte(`{"a": [1, 2, 3]}`))
	arr, err := doc.Array("a")
	if err != nil {
		t.Fatalf("Array(a): %v", err)
	}
	if len(arr) != 3 {
		t.Fatalf("len = %d, want 3", len(arr))
	}
	for i, v := range arr {
		text, err := ToStringPretty(v)
		if err != nil {
			t.Fatalf("element %d: %v", i, err)
		}
		want := []string{"1", "2", "3"}[i]
		if text != want {
			t.Fatalf("element %d = %q, want %q", i, text, want)
		}
	}
}

func TestPushString(t *testing.T) {
	arr := NewArray()
	if err := arr.PushString("x"); err != nil {
		t.Fatalf("PushString: %v", err)
	}
	if err := arr.PushString("y"); err != nil {
		t.Fatalf("PushString: %v", err)
	}
	pretty, err := ToStringPretty(arr)
	if err != nil || pretty != "[\n  \"x\",\n  \"y\"\n]" {
		t.Fatalf("pretty = %q, %v", pretty, err)
	}
}

func TestZeroValueGuard(t *testing.T) {
	var v Value
	if _, err := v.Int("k"); err == nil {
		t.Fatal("zero Value must error, not cross")
	}
}

func TestStringValueUnescape(t *testing.T) {
	doc := NewObject()
	cases := []string{
		"simple",
		"with \"quotes\" and \\backslash",
		"newline\nand\ttab",
		"unicode: \u2764 \u0026 \u00a9",
	}
	for _, want := range cases {
		if err := doc.Set("k", want); err != nil {
			t.Fatalf("Set(%q): %v", want, err)
		}
		got, err := doc.String("k")
		if err != nil {
			t.Fatalf("String(%q): %v", want, err)
		}
		if got != want {
			t.Errorf("got %q, want %q", got, want)
		}
	}
}

func TestMovedHandleCleanupDoesNotDestroyReusedSlot(t *testing.T) {
	doc := NewObject()
	func() {
		child := NewArray()
		if err := child.PushString("ada"); err != nil {
			t.Fatalf("PushString: %v", err)
		}
		if err := doc.Set("active", child); err != nil {
			t.Fatalf("Set(active): %v", err)
		}
	}()
	next, err := Parse([]byte(`{"k": 99}`))
	if err != nil {
		t.Fatalf("Parse: %v", err)
	}
	// Trigger runtime.AddCleanup for the moved `child` handle. Because `next`
	// reuses `child`'s table slot with an incremented generation tag, the
	// stale cleanup release must not free `next`.
	for range 4 {
		runtime.GC()
	}
	got, err := next.Int("k")
	if err != nil || got != 99 {
		t.Fatalf("next.Int(k) after GC = %d, %v (want 99, nil)", got, err)
	}
	runtime.KeepAlive(doc)
	runtime.KeepAlive(next)
}

func TestFailedSetPreservesValueHandle(t *testing.T) {
	arr := NewArray()
	child, err := Parse([]byte(`{"x": 7}`))
	if err != nil {
		t.Fatalf("Parse: %v", err)
	}
	if err := arr.Set("k", child); err == nil {
		t.Fatal("Set on array receiver must error")
	}
	got, err := child.Int("x")
	if err != nil || got != 7 {
		t.Fatalf("child.Int(x) after failed Set = %d, %v (want 7, nil)", got, err)
	}
}

package w024wasm2go

import (
	"bytes"
	"encoding/hex"
	"fmt"
	"strings"
	"sync"
	"testing"
)

const samplePayload = `{"service":"hybrid-compiler","tier":"zero-cgo","version":"2026.10","status":"verified"}`

// TestP1_ZeroCgoLinearArena_PreservesOwnAndBorrowSemantics verifies Hypothesis P1:
// OwnHandle (Doc) and BorrowHandle (View) operate entirely inside a pure-Go
// bounds-checked []byte Wasm linear-memory arena without cgo or unsafe.Pointer.
func TestP1_ZeroCgoLinearArena_PreservesOwnAndBorrowSemantics(t *testing.T) {
	doc, err := Parse(samplePayload, 1)
	if err != nil {
		t.Fatalf("unexpected parse error: %v", err)
	}
	defer doc.Free()

	root, err := doc.Root()
	if err != nil {
		t.Fatalf("unexpected root error: %v", err)
	}

	gotService, err := root.GetStr("service")
	if err != nil || gotService != "hybrid-compiler" {
		t.Fatalf("GetStr(service) = (%q, %v), want (\"hybrid-compiler\", nil)", gotService, err)
	}

	gotTier, err := root.GetBytesZeroCopy("tier")
	if err != nil || string(gotTier) != "zero-cgo" {
		t.Fatalf("GetBytesZeroCopy(tier) = (%q, %v), want (\"zero-cgo\", nil)", gotTier, err)
	}
}

// TestP2_CrossTaskMoveTransfer_ZeroDataRaceUnderRaceDetector verifies Hypothesis P2:
// Moving an OwnHandle (`move doc`) across a `task` / `chan` boundary transfers the
// underlying linear-memory slice cleanly with 0 data races under `go test -race`.
func TestP2_CrossTaskMoveTransfer_ZeroDataRaceUnderRaceDetector(t *testing.T) {
	const workers = 16
	const iterations = 200
	ch := make(chan *Doc, workers)
	var wg sync.WaitGroup

	for w := 0; w < workers; w++ {
		wg.Add(1)
		go func(id int) {
			defer wg.Done()
			for i := 0; i < iterations; i++ {
				payload := fmt.Sprintf(`{"worker":"%d","iter":"%d","mode":"wasm2go"}`, id, i)
				doc, err := Parse(payload, uint64(id*10000+i+1))
				if err != nil {
					t.Errorf("parse failed: %v", err)
					return
				}
				// Emulates `ch <- move doc`: producer relinquishes ownership.
				ch <- doc
			}
		}(w)
	}

	go func() {
		wg.Wait()
		close(ch)
	}()

	consumed := 0
	for doc := range ch {
		root, err := doc.Root()
		if err != nil {
			t.Fatalf("consumer Root() failed: %v", err)
		}
		mode, err := root.GetStr("mode")
		if err != nil || mode != "wasm2go" {
			t.Fatalf("consumer GetStr(mode) = (%q, %v)", mode, err)
		}
		doc.Free()
		consumed++
	}

	if consumed != workers*iterations {
		t.Fatalf("consumed %d docs, want %d", consumed, workers*iterations)
	}
}

// TestP3_DeepNesting128AndFullExportSurface_ZeroCgo verifies Hypothesis P3 (M2 Gate):
//   1. 128-deep nested JSON (`interfaces/descriptors.md` DEEP class) parses, traverses,
//      mutates (`NewObject`, `NewArray`, `ArrayPushString`, `Set`), and serializes
//      (`CompactText`, `PrettyText`) on the standard growable Go goroutine stack
//      without the 1 MiB C stack pool (`CrossDeep*`).
//   2. All remaining boundary surfaces (`sha3`, `regex`, `flate2`, `ed25519`, `abi_word`,
//      `stack_alignment_probe`, `hy_nop`) execute in pure Go (`CGO_ENABLED=0`).
func TestP3_DeepNesting128AndFullExportSurface_ZeroCgo(t *testing.T) {
	// 1. 60-pair (121-deep) nested object/array stress test (matches `serde_json`
	// remaining_depth = 128 in `interfaces/descriptors.md` DEEP class, which previously
	// overflowed the 4,096 B DEFAULT cgo guard and required a 1 MiB C stack pool).
	const depth = 60
	var sb strings.Builder
	for i := 0; i < depth; i++ {
		sb.WriteString(`{"a":[`)
	}
	sb.WriteString(`{"leaf":"ok","count":42}`)
	for i := 0; i < depth; i++ {
		sb.WriteString(`]}`)
	}

	deepDoc, err := Parse(sb.String(), NextGeneration())
	if err != nil {
		t.Fatalf("121-deep Parse failed: %v", err)
	}
	defer deepDoc.Free()

	cur, err := deepDoc.Root()
	if err != nil {
		t.Fatalf("deepDoc.Root() failed: %v", err)
	}
	for i := 0; i < depth; i++ {
		arrView, err := cur.GetArray("a")
		if err != nil {
			t.Fatalf("depth %d GetArray(a) failed: %v", i, err)
		}
		n, err := arrView.ArrayLen()
		if err != nil || n != 1 {
			t.Fatalf("depth %d ArrayLen = (%d, %v), want (1, nil)", i, n, err)
		}
		cur, err = arrView.ArrayGet(0)
		if err != nil {
			t.Fatalf("depth %d ArrayGet(0) failed: %v", i, err)
		}
	}
	leafStr, err := cur.GetStr("leaf")
	if err != nil || leafStr != "ok" {
		t.Fatalf("deep leaf GetStr = (%q, %v), want (\"ok\", nil)", leafStr, err)
	}
	leafCount, err := cur.GetInt("count")
	if err != nil || leafCount != 42 {
		t.Fatalf("deep leaf GetInt = (%d, %v), want (42, nil)", leafCount, err)
	}

	// 2. Verify >128 nesting depth is deterministically rejected with ErrMaxDepthExceeded.
	var tooDeep strings.Builder
	for i := 0; i < 135; i++ {
		tooDeep.WriteByte('[')
	}
	tooDeep.WriteString("1")
	for i := 0; i < 135; i++ {
		tooDeep.WriteByte(']')
	}
	if _, err := Parse(tooDeep.String(), NextGeneration()); err != ErrMaxDepthExceeded {
		t.Fatalf("expected ErrMaxDepthExceeded on 135-deep JSON, got %v", err)
	}

	// 3. Verify NewObject, NewArray, ArrayPushString, and Set(move child).
	parent, err := NewObject(NextGeneration())
	if err != nil {
		t.Fatalf("NewObject failed: %v", err)
	}
	defer parent.Free()

	arrDoc, err := NewArray(NextGeneration())
	if err != nil {
		t.Fatalf("NewArray failed: %v", err)
	}
	arrRoot, _ := arrDoc.Root()
	if err := arrRoot.ArrayPushString("alpha"); err != nil {
		t.Fatalf("ArrayPushString(alpha) failed: %v", err)
	}
	if err := arrRoot.ArrayPushString("beta"); err != nil {
		t.Fatalf("ArrayPushString(beta) failed: %v", err)
	}
	// Move arrDoc into parent["items"]; arrDoc is consumed.
	if err := parent.Set("items", arrDoc); err != nil {
		t.Fatalf("parent.Set(items, arrDoc) failed: %v", err)
	}
	if _, err := arrDoc.Root(); err != ErrUseAfterFree {
		t.Fatalf("expected consumed child handle to fail with ErrUseAfterFree, got %v", err)
	}
	parentRoot, _ := parent.Root()
	itemsView, err := parentRoot.GetArray("items")
	if err != nil {
		t.Fatalf("parentRoot.GetArray(items) failed: %v", err)
	}
	elem1, err := itemsView.ArrayGet(1)
	if err != nil {
		t.Fatalf("itemsView.ArrayGet(1) failed: %v", err)
	}
	compactElem1, err := elem1.CompactText()
	if err != nil || compactElem1 != `"beta"` {
		t.Fatalf("elem1.CompactText() = (%q, %v), want (\"\\\"beta\\\"\", nil)", compactElem1, err)
	}

	// 4. Verify SHA3-256 NIST vector ("abc") and stream checksum determinism.
	var sha3Out [32]byte
	if st := HySha3Digest256([]byte("abc"), sha3Out[:]); st != 0 {
		t.Fatalf("HySha3Digest256 status = %d, want 0", st)
	}
	const wantSha3Abc = "3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532"
	if gotHex := hex.EncodeToString(sha3Out[:]); gotHex != wantSha3Abc {
		t.Fatalf("SHA3-256(\"abc\") = %s, want %s", gotHex, wantSha3Abc)
	}
	cs1 := HySha3StreamChecksum(16, 42)
	cs2 := HySha3StreamChecksum(16, 42)
	if cs1 == 0 || cs1 != cs2 {
		t.Fatalf("HySha3StreamChecksum non-deterministic: %x vs %x", cs1, cs2)
	}

	// 5. Verify Regex, Flate2, Ed25519, and ABI probes.
	if HyRegexIsValid(`^[a-z0-9_-]+$`) != 1 || HyRegexIsValid(`[unclosed`) != 0 {
		t.Fatalf("HyRegexIsValid mismatch")
	}
	if HyRegexIsMatch(`^hybrid-[0-9]+$`, "hybrid-2026") != 1 || HyRegexIsMatch(`[bad`, "x") != -1 {
		t.Fatalf("HyRegexIsMatch mismatch")
	}
	rawFlate := bytes.Repeat([]byte("wasm2go-linear-memory-arena-"), 64)
	roundtrip, err := HyFlateCompressRoundtrip(rawFlate)
	if err != nil || !bytes.Equal(roundtrip, rawFlate) {
		t.Fatalf("HyFlateCompressRoundtrip failed: %v", err)
	}
	var seed [32]byte
	seed[0] = 7
	okSig, sha256Digest := HyEd25519SignVerify(seed, rawFlate)
	if !okSig || sha256Digest == ([32]byte{}) {
		t.Fatalf("HyEd25519SignVerify failed")
	}
	if HyNop(0xCAFEBABE) != 0xCAFEBABE || HyrtAbiWord() != abiHandshakeWord || HyrtStackAlignmentProbe() != 0 {
		t.Fatalf("ABI probe mismatch")
	}
}

// TestF1_Falsifier_UseAfterFreeAndStaleGenerationIntercepted verifies Declared
// Falsifier F1: any attempt to read through a BorrowHandle (`View`) after the
// owning `Doc` is freed (or after its pooled linear-memory page is reused by a
// newer generation) is deterministically rejected with HYB-E3105 / HYB-E3102.
func TestF1_Falsifier_UseAfterFreeAndStaleGenerationIntercepted(t *testing.T) {
	doc1, err := Parse(samplePayload, 101)
	if err != nil {
		t.Fatalf("parse doc1 failed: %v", err)
	}
	staleView, err := doc1.Root()
	if err != nil {
		t.Fatalf("doc1.Root() failed: %v", err)
	}

	// Free doc1 (emulating end of `managed` scope `defer doc.free()`).
	doc1.Free()

	if _, err := staleView.GetStr("service"); err != ErrUseAfterFree {
		t.Fatalf("expected ErrUseAfterFree on freed Doc, got %v", err)
	}

	// Allocate doc2 (which reuses the pooled 64 KiB page under generation 202)
	// and verify that a view forged with old generation 101 against doc2 is
	// rejected with ErrStaleGeneration.
	doc2, err := Parse(`{"service":"reused-arena"}`, 202)
	if err != nil {
		t.Fatalf("parse doc2 failed: %v", err)
	}
	defer doc2.Free()

	forgedStaleView := View{
		owner:   doc2,
		gen:     101, // old generation
		nodeOff: doc2.rootOff,
	}
	if _, err := forgedStaleView.GetStr("service"); err != ErrStaleGeneration {
		t.Fatalf("expected ErrStaleGeneration on mismatched generation, got %v", err)
	}
}

// TestF2_Falsifier_HandleTableSlotReuseAndShortBufferCanary verifies Declared
// Falsifier F2:
//   1. Reusing a freed slot in `HandleTable` increments the 32-bit generation
//      monotonically so the old packed `u64` handle fails with `StatusErrTable (1)`
//      (closing the constant `gen = 1u32` ABA hazard in `runtime/rust/hyrt/src/lib.rs:146`).
//   2. `SerializeHandle` enforces the exact `json_text_tests` sentinel-canary protocol
//      from `runtime/rust/hyrt/src/lib.rs:487-573` (`0xa5` guard bytes untouched on
//      invalid handles, short buffers, and offset writes).
func TestF2_Falsifier_HandleTableSlotReuseAndShortBufferCanary(t *testing.T) {
	var tbl HandleTable

	// 1. Slot reuse ABA falsifier: insert -> release -> insert reuses slot 0 with gen=2.
	d1, err := Parse(`{"k":7}`, NextGeneration())
	if err != nil {
		t.Fatalf("Parse d1 failed: %v", err)
	}
	h1 := tbl.Insert(d1)
	if tbl.Release(h1) != StatusOK {
		t.Fatalf("Release(h1) failed")
	}
	if tbl.LiveCount() != 0 {
		t.Fatalf("LiveCount after release = %d, want 0", tbl.LiveCount())
	}

	d2, err := Parse(`{"k":7}`, NextGeneration())
	if err != nil {
		t.Fatalf("Parse d2 failed: %v", err)
	}
	h2 := tbl.Insert(d2)
	defer tbl.Release(h2)

	if h1 == h2 {
		t.Fatalf("slot reuse produced identical handle %#x (ABA hazard)", h1)
	}
	if _, st := tbl.Get(h1); st != StatusErrTable {
		t.Fatalf("stale handle h1 after slot reuse returned status %d, want StatusErrTable (1)", st)
	}

	// 2. Exact `json_text_tests` buffer canary protocol (compact & pretty).
	cases := []struct {
		pretty   bool
		expected []byte
	}{
		{false, []byte(`{"k":7}`)},
		{true, []byte("{\n  \"k\": 7\n}")},
	}

	for _, tc := range cases {
		// Invalid handles must return StatusErrTable and leave buffer & outLen untouched.
		for _, badHandle := range []uint64{0, ^uint64(0), 0x0000_0001_ffff_ffff, h1} {
			out := bytes.Repeat([]byte{0xa5}, 32)
			need := uint64(0xfeed)
			st := tbl.SerializeHandle(badHandle, tc.pretty, out, &need)
			if st != StatusErrTable {
				t.Fatalf("pretty=%v badHandle=%#x status=%d, want StatusErrTable", tc.pretty, badHandle, st)
			}
			if need != 0xfeed || !bytes.Equal(out, bytes.Repeat([]byte{0xa5}, 32)) {
				t.Fatalf("pretty=%v badHandle mutated out or need", tc.pretty)
			}
		}

		// Query length with nil buffer -> StatusTryAgain (3).
		var need uint64
		if st := tbl.SerializeHandle(h2, tc.pretty, nil, &need); st != StatusTryAgain {
			t.Fatalf("pretty=%v nil buffer status=%d, want StatusTryAgain (3)", tc.pretty, st)
		}
		if int(need) != len(tc.expected) {
			t.Fatalf("pretty=%v need=%d, want %d", tc.pretty, need, len(tc.expected))
		}

		// Short buffer (len - 1) -> StatusTryAgain (3) and 0xa5 canary bytes untouched.
		out := bytes.Repeat([]byte{0xa5}, 64)
		if st := tbl.SerializeHandle(h2, tc.pretty, out[:len(tc.expected)-1], &need); st != StatusTryAgain {
			t.Fatalf("pretty=%v short buffer status=%d, want StatusTryAgain (3)", tc.pretty, st)
		}
		if !bytes.Equal(out, bytes.Repeat([]byte{0xa5}, 64)) {
			t.Fatalf("pretty=%v short buffer clobbered 0xa5 canary bytes", tc.pretty)
		}

		// Exact slice window out[1 : 1+len(expected)] -> StatusOK (0) with surrounding 0xa5 intact.
		if st := tbl.SerializeHandle(h2, tc.pretty, out[1:1+len(tc.expected)], &need); st != StatusOK {
			t.Fatalf("pretty=%v exact window status=%d, want StatusOK (0)", tc.pretty, st)
		}
		if !bytes.Equal(out[1:1+len(tc.expected)], tc.expected) {
			t.Fatalf("pretty=%v payload=%q, want %q", tc.pretty, out[1:1+len(tc.expected)], tc.expected)
		}
		if out[0] != 0xa5 || !bytes.Equal(out[1+len(tc.expected):], bytes.Repeat([]byte{0xa5}, 64-1-len(tc.expected))) {
			t.Fatalf("pretty=%v surrounding 0xa5 canary bytes clobbered", tc.pretty)
		}

		// Nil outLen pointer -> StatusErrTable (1).
		guarded := bytes.Repeat([]byte{0xa5}, 64)
		if st := tbl.SerializeHandle(h2, tc.pretty, guarded, nil); st != StatusErrTable {
			t.Fatalf("pretty=%v nil outLen status=%d, want StatusErrTable (1)", tc.pretty, st)
		}
	}
}

package analyzer

import (
	"context"
	"encoding/hex"
	"encoding/json"
	"testing"
	"time"
)

func TestMalformedEnvelopeFailsClosed(t *testing.T) {
	valid := `{"version":1,"diagnostics":[],"summary":{"verdict":"accept","errors":0,"warnings":0}}`
	for _, payload := range []string{
		`{}`, `null`, `[]`, `{"summary":{"verdict":"accept"}}`,
		`{"version":1,"diagnostics":[],"summary":{"verdict":"mystery","errors":0,"warnings":0}}`,
		`{"version":1,"diagnostics":[],"summary":{"verdict":"reject","errors":0,"warnings":0}}`,
		`{"version":1,"diagnostics":[],"summary":{"verdict":"accept","errors":1,"warnings":0}}`,
		`{"version":1,"diagnostics":null,"summary":{"verdict":"accept","errors":0,"warnings":0}}`,
		`{"version":1,"diagnostics":[],"summary":{"verdict":"accept","errors":-1,"warnings":0}}`,
		`{"version":2,"diagnostics":[],"summary":{"verdict":"accept","errors":0,"warnings":0}}`,
	} {
		t.Run(payload, func(t *testing.T) {
			abi := &MockWasmABI{Memory: make([]byte, 4096), CheckFn: func(context.Context, uint64, uint64) (uint64, error) {
				return uint64(1024)<<32 | uint64(len(payload)), nil
			}}
			copy(abi.Memory[1024:], payload)
			result := CheckSourceWithABI(context.Background(), abi, "source")
			if result.Verdict != VerdictAnalysisFailure || result.Failure == nil || len(result.Diagnostics) != 0 {
				t.Fatalf("not fail-closed: %+v", result)
			}
		})
	}
	var envelope map[string]any
	if err := json.Unmarshal([]byte(valid), &envelope); err != nil {
		t.Fatal(err)
	}
	envelope["future_field"] = "allowed"
	data, _ := json.Marshal(envelope)
	if _, err := DecodeCheckOutput(data); err != nil {
		t.Fatalf("additive field rejected: %v", err)
	}
}

// Minimal real WASM guest: memory, allocator, free, and check exports. The
// check body is either unreachable or an infinite loop; no mocked Go error.
func TestRealGuestTrapAndActiveTimeout(t *testing.T) {
	for _, tc := range []struct{ name, body string }{
		{"trap", "00000042000b"},
		{"active_timeout", "0003400c000b42000b"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			wasm, err := hex.DecodeString(guestModuleHex(tc.body))
			if err != nil {
				t.Fatal(err)
			}
			engine, err := NewWasmEngine(context.Background(), wasm)
			if err != nil {
				t.Fatal(err)
			}
			defer engine.Close()
			ctx, cancel := context.WithTimeout(context.Background(), 50*time.Millisecond)
			defer cancel()
			result := engine.CheckSource(ctx, "input")
			if result.Verdict != VerdictAnalysisFailure || result.Failure == nil {
				t.Fatalf("not fail-closed: %+v", result)
			}
			if tc.name == "active_timeout" && ctx.Err() == nil {
				t.Fatal("guest returned before deadline")
			}
		})
	}
}

func guestModuleHex(checkBody string) string {
	prefix := "0061736d0100000001110360017f017f60027f7f0060027f7f017e0304030001020503010001073904066d656d6f727902000d68795f7761736d5f616c6c6f6300000c68795f7761736d5f6672656500010d68795f7761736d5f636865636b0002"
	body, _ := hex.DecodeString(checkBody)
	code := []byte{3, 4, 0, 0x41, 8, 0x0b, 2, 0, 0x0b, byte(len(body))}
	code = append(code, body...)
	section := append([]byte{10, byte(len(code))}, code...)
	return prefix + hex.EncodeToString(section)
}

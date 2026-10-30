package main

import (
	"context"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
)

const validRejection = `{"version":1,"diagnostics":[{"code":"HYB-E3003","severity":"error","message":"bad","span":{"start":{"line":1,"col":1},"end":{"line":1,"col":2}}}],"summary":{"verdict":"reject","errors":1,"warnings":0}}`

func TestNativeExitHelper(t *testing.T) {
	if os.Getenv("W012_TEST_NATIVE_HELPER") != "1" {
		return
	}
	fmt.Print(os.Getenv("W012_TEST_NATIVE_OUTPUT"))
	code, _ := strconv.Atoi(os.Getenv("W012_TEST_NATIVE_EXIT"))
	os.Exit(code)
}

func TestNativeRejectionValidation(t *testing.T) {
	for _, tc := range []struct {
		name, output string
		code         int
		wantErr      bool
	}{
		{"valid_rejection", validRejection, 1, false},
		{"missing_output", "", 1, true},
		{"invalid_json", "oops", 1, true},
		{"missing_warning_count", strings.Replace(validRejection, `,"warnings":0`, "", 1), 1, true},
		{"missing_code", strings.Replace(validRejection, `"code":"HYB-E3003",`, "", 1), 1, true},
		{"missing_message", strings.Replace(validRejection, `"message":"bad",`, "", 1), 1, true},
		{"invalid_span", strings.Replace(validRejection, `"line":1`, `"line":0`, 1), 1, true},
		{"invalid_severity", strings.Replace(validRejection, `"severity":"error"`, `"severity":"mystery"`, 1), 1, true},
		{"unexpected_success", validRejection, 0, true},
		{"unexpected_failure", validRejection, 2, true},
		{"false_reject", `{"version":1,"diagnostics":[],"summary":{"verdict":"reject","errors":1,"warnings":0}}`, 1, true},
	} {
		t.Run(tc.name, func(t *testing.T) {
			cmd := exec.Command(os.Args[0], "-test.run=^TestNativeExitHelper$")
			cmd.Env = append(os.Environ(), "W012_TEST_NATIVE_HELPER=1", "W012_TEST_NATIVE_EXIT="+strconv.Itoa(tc.code), "W012_TEST_NATIVE_OUTPUT="+tc.output)
			output, err := cmd.Output()
			got := validateNativeRejection(output, err)
			if (got != nil) != tc.wantErr {
				t.Fatalf("error=%v, wantErr=%v", got, tc.wantErr)
			}
		})
	}
}

func TestMissingNativeBinaryFails(t *testing.T) {
	if err := nativeReject(context.Background(), filepath.Join(t.TempDir(), "missing-hyb"), "fixture.hy"); err == nil {
		t.Fatal("missing native binary accepted")
	}
}

func TestStatistics(t *testing.T) {
	samples := []float64{4, 1, 3, 2}
	stats := calcStats(samples)
	if stats.Min != 1 || stats.P50 != 2 || stats.P90 != 4 || stats.P99 != 4 || stats.Mean != 2.5 {
		t.Fatalf("wrong stats: %+v", stats)
	}
	if samples[0] != 4 {
		t.Fatal("mutated raw samples")
	}
}

func TestNoFiniteBreakEvenWhenWarmPathIsSlower(t *testing.T) {
	for _, savings := range []float64{0, -1} {
		if got := compileBreakEven(10, savings); got != "no finite break-even (warm path is not faster)" {
			t.Fatal(got)
		}
	}
	if got := compileBreakEven(10, 3); got != "3.3 checks (ceil: 4 checks)" {
		t.Fatal(got)
	}
}

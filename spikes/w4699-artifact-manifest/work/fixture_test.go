package w4699artifactmanifest

import (
	"os"
	"path/filepath"
	"testing"
)

// TestAdmissionFixturesAreLoadBearing reads the real `.hy` fixture files from
// the spike's hy/ directory and runs the static admission pass on them. This
// makes the fixtures part of the verified surface rather than decoration: the
// admitted fixture must pass with zero diagnostics, and the rejected fixture
// must fail closed with the documented codes.
func TestAdmissionFixturesAreLoadBearing(t *testing.T) {
	hyDir := filepath.Join("..", "hy")

	admitted, err := os.ReadFile(filepath.Join(hyDir, "01-artifact-group-admitted.hy"))
	if err != nil {
		t.Fatalf("read admitted fixture: %v", err)
	}
	if diags := CheckWorkerAdmission(string(admitted)); len(diags) != 0 {
		t.Fatalf("admitted fixture rejected: %+v", diags)
	}

	rejected, err := os.ReadFile(filepath.Join(hyDir, "02-artifact-group-opaque-rejected.hy"))
	if err != nil {
		t.Fatalf("read rejected fixture: %v", err)
	}
	diags := CheckWorkerAdmission(string(rejected))
	if len(diags) == 0 {
		t.Fatalf("rejected fixture was admitted; expected HYB-E4701/HYB-E4702")
	}
	// The rejected fixture contains both an ambient read (time.now) and an
	// opaque call (opaque_call); both must be reported.
	var sawAmbient, sawOpaque bool
	for _, d := range diags {
		switch d.Code {
		case "HYB-E4701":
			sawAmbient = true
		case "HYB-E4702":
			sawOpaque = true
		}
	}
	if !sawAmbient {
		t.Fatalf("rejected fixture missing HYB-E4701 (ambient): %+v", diags)
	}
	if !sawOpaque {
		t.Fatalf("rejected fixture missing HYB-E4702 (opaque): %+v", diags)
	}
}
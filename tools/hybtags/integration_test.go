package main

import (
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"testing"
)

// TestHybtagsEndToEnd builds the hybtags binary and runs it against the
// testdata/fixture package with three declared tags. It verifies:
//   - all 2^3 = 8 combinations are enumerated,
//   - {fast} and {fast,debug} select identical files and share one checker run
//     (fast_or_debug.go is already selected by `fast`, so adding `debug`
//     changes nothing),
//   - every group passes with `go build`,
//   - with a failing checker, every group fails and a minimal failing config
//     is reported.
func TestHybtagsEndToEnd(t *testing.T) {
	moduleRoot, err := findModuleRoot()
	if err != nil {
		t.Fatal(err)
	}

	// Build the binary.
	bin := filepath.Join(moduleRoot, "hybtags.testbin")
	cmd := exec.Command("go", "build", "-o", bin, ".")
	cmd.Dir = moduleRoot
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("go build hybtags: %v\n%s", err, out)
	}
	defer os.Remove(bin)

	fixture := filepath.Join(moduleRoot, "testdata", "fixture")

	// --- Run 1: `go build` as checker, all groups must pass. -------------
	out, _, err := runHybtags(bin, fixture, "fast,small,debug", "go build", true)
	if err != nil && len(out) == 0 {
		t.Fatalf("hybtags run (passing checker): %v", err)
	}
	var rep report
	if err := json.Unmarshal(out, &rep); err != nil {
		t.Fatalf("parsing hybtags JSON: %v\n%s", err, out)
	}

	if rep.Combinations != 8 {
		t.Errorf("expected 8 combinations, got %d", rep.Combinations)
	}
	if rep.UniqueInputs >= rep.Combinations {
		t.Errorf("expected grouping: %d unique inputs >= %d combinations", rep.UniqueInputs, rep.Combinations)
	}
	if rep.CheckerInvocations != rep.UniqueInputs {
		t.Errorf("expected one checker run per unique input: %d runs vs %d inputs", rep.CheckerInvocations, rep.UniqueInputs)
	}

	foundShared := false
	for _, g := range rep.Groups {
		if !g.Pass {
			t.Errorf("group %v unexpectedly failed with go build: %s", g.TagCombinations, g.CheckerErr)
		}
		comboSet := map[string]bool{}
		for _, tc := range g.TagCombinations {
			comboSet[tc] = true
		}
		if comboSet["fast"] && comboSet["fast,debug"] {
			foundShared = true
		}
	}
	if !foundShared {
		t.Error("expected {fast} and {fast,debug} to share a group (identical file selection)")
	}

	// --- Run 2: `false` as checker, every group fails. ------------------
	out, _, err = runHybtags(bin, fixture, "fast,small", "false", true)
	if len(out) == 0 {
		t.Fatalf("hybtags run (failing checker) produced no output: %v", err)
	}
	var rep2 report
	if err := json.Unmarshal(out, &rep2); err != nil {
		t.Fatalf("parsing hybtags JSON (failing checker): %v\n%s", err, out)
	}
	for _, g := range rep2.Groups {
		if g.Pass {
			t.Errorf("group %v should fail with `false` checker", g.TagCombinations)
		}
		if len(g.MinimalFailing) == 0 {
			t.Errorf("group %v should report a non-empty minimal failing config", g.TagCombinations)
		}
	}
}

// runHybtags executes the hybtags binary and returns (combinedOutput, nil).
// A non-zero exit is not fatal: the caller decides how to interpret it, and
// the JSON report is still on stdout even when groups fail.
func runHybtags(bin, pkg, tags, checker string, jsonOut bool) ([]byte, error) {
	args := []string{"--pkg", pkg, "--tags", tags, "--checker", checker}
	if jsonOut {
		args = append(args, "--json")
	}
	cmd := exec.Command(bin, args...)
	return cmd.CombinedOutput()
}

// findModuleRoot walks up from the current directory to find go.mod.
func findModuleRoot() (string, error) {
	dir, err := os.Getwd()
	if err != nil {
		return "", err
	}
	for {
		if _, err := os.Stat(filepath.Join(dir, "go.mod")); err == nil {
			return dir, nil
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			return "", os.ErrNotExist
		}
		dir = parent
	}
}

package main

import (
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"testing"
)

func TestRealGoToolchainIntegration(t *testing.T) {
	dir := t.TempDir()
	hook := filepath.Join(dir, "nativehook.exe")
	if output, err := exec.Command("go", "build", "-o", hook, ".").CombinedOutput(); err != nil {
		t.Fatalf("build native hook: %v: %s", err, output)
	}
	sourceDir := filepath.Join(dir, "source with space")
	if err := os.Mkdir(sourceDir, 0700); err != nil {
		t.Fatal(err)
	}
	source, err := os.ReadFile("testdata/pass/main.go")
	if err != nil {
		t.Fatal(err)
	}
	sourceFile := filepath.Join(sourceDir, "main.go")
	if err = os.WriteFile(sourceFile, source, 0600); err != nil {
		t.Fatal(err)
	}
	artifact := filepath.Join(dir, "pass.exe")
	option := "-toolexec=" + strconv.Quote(filepath.ToSlash(hook))
	if output, err := exec.Command("go", "build", option, "-o", artifact, sourceFile).CombinedOutput(); err != nil {
		t.Fatalf("native checked build: %v: %s", err, output)
	}
	if output, err := exec.Command(artifact).CombinedOutput(); err != nil || strings.TrimSpace(string(output)) != "42" {
		t.Fatalf("actual program: %v: %s", err, output)
	}
	output, err := exec.Command("go", "build", option, "-o", filepath.Join(dir, "reject.exe"), "./testdata/reject").CombinedOutput()
	if err == nil || !strings.Contains(string(output), "HYB-NATIVE-E001") {
		t.Fatalf("unproved input not rejected: %v: %s", err, output)
	}
	compiler := filepath.Join(runtime.GOROOT(), "pkg", "tool", runtime.GOOS+"_"+runtime.GOARCH, "compile")
	if runtime.GOOS == "windows" {
		compiler += ".exe"
	}
	version, err := exec.Command(hook, compiler, "-V=full").CombinedOutput()
	if err != nil || !strings.Contains(string(version), "native-scalar-v1-") {
		t.Fatalf("version/cache identity: %v: %s", err, version)
	}
	directErr := exec.Command(compiler, "-unsupported-native-hook-test-option").Run()
	wrappedErr := exec.Command(hook, compiler, "-unsupported-native-hook-test-option").Run()
	direct, ok1 := directErr.(*exec.ExitError)
	wrapped, ok2 := wrappedErr.(*exec.ExitError)
	if !ok1 || !ok2 || direct.ExitCode() != wrapped.ExitCode() {
		t.Fatalf("underlying exit status changed: %v / %v", directErr, wrappedErr)
	}
}

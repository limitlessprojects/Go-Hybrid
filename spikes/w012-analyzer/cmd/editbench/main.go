// editbench measures after-write request latency. It is NOT an LSP or gopls benchmark.
package main

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"math"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"sort"
	"strings"
	"time"

	analyzer "github.com/limitlessprojects/go-hybrid/spikes/w012-analyzer"
)

type statistics struct {
	Min, P50, P90, P95, P99, Mean float64
}

func summarize(values []float64) statistics {
	if len(values) == 0 {
		return statistics{}
	}
	sorted := append([]float64(nil), values...)
	sort.Float64s(sorted)
	percentile := func(p float64) float64 {
		i := int(math.Ceil(p*float64(len(sorted)))) - 1
		if i < 0 {
			i = 0
		}
		if i >= len(sorted) {
			i = len(sorted) - 1
		}
		return sorted[i]
	}
	var sum float64
	for _, value := range sorted {
		sum += value
	}
	return statistics{Min: sorted[0], P50: percentile(.5), P90: percentile(.9), P95: percentile(.95), P99: percentile(.99), Mean: sum / float64(len(sorted))}
}

type sample struct {
	Edit                 int
	File                 string
	First                string
	NativeMS             float64
	InProcessMS          float64
	ExperimentalReusedMS float64
}

type report struct {
	Kind                                        string
	TimestampUTC                                string
	OS, Arch, Go                                string
	CPUs, GOMAXPROCS, Files, Edits, Warmup      int
	FixtureSHA256, WASMSHA256, NativeSHA256     string
	WASMBytes, NativeBytes                      int
	ColdWASMInitMS, ColdReusedInitMS            float64
	Native, InProcess, ReusedGuest              statistics
	NativeOverInProcessP50, NativeOverReusedP50 float64
	Samples                                     []sample
	Limitations                                 []string
}

type settings struct {
	root, wasm, native, fixture, output string
	files, edits, warmup                int
}

func editedSource(template string, iteration int) string {
	// Fixed-width, non-cumulative changes preserve the registered oracle.
	return template + fmt.Sprintf("\n// W012-EDIT-BENCH revision=%06d\n", iteration)
}

func expectedVerdict(template string) (string, error) {
	if strings.Contains(template, "EXPECTED-VERDICT: REJECT") {
		return "reject", nil
	}
	if strings.Contains(template, "EXPECTED-VERDICT: ACCEPT") {
		return "accept", nil
	}
	return "", errors.New("missing preregistered EXPECTED-VERDICT in benchmark fixture")
}

func artifactInfo(path string) (string, int, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return "", 0, err
	}
	hash := sha256.Sum256(data)
	return hex.EncodeToString(hash[:]), len(data), nil
}

func nativeCheck(binary, file string) (*analyzer.CheckOutput, float64, error) {
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	start := time.Now()
	data, runErr := exec.CommandContext(ctx, binary, "check", file, "--json").Output()
	var exitErr *exec.ExitError
	if runErr != nil && (!errors.As(runErr, &exitErr) || exitErr.ExitCode() != 1) {
		return nil, 0, fmt.Errorf("unexpected native process failure: %w", runErr)
	}
	out, err := analyzer.DecodeCheckOutput(data)
	if err != nil {
		return nil, 0, fmt.Errorf("malformed native diagnostics: %w", err)
	}
	if (runErr == nil) != (out.Summary.Verdict == "accept") {
		return nil, 0, fmt.Errorf("native exit code inconsistent with %s", out.Summary.Verdict)
	}
	return out, float64(time.Since(start).Nanoseconds()) / 1e6, nil
}

func wasmCheck(engine *analyzer.Engine, source string) (*analyzer.CheckOutput, float64, error) {
	start := time.Now()
	out, err := engine.Check(source)
	if err != nil {
		return nil, 0, err
	}
	return out, float64(time.Since(start).Nanoseconds()) / 1e6, nil
}

func comparable(native, embedded *analyzer.CheckOutput, expected string) error {
	if native.Summary.Verdict != expected || embedded.Summary.Verdict != expected {
		return fmt.Errorf("oracle %s: native=%s embedded=%s", expected, native.Summary.Verdict, embedded.Summary.Verdict)
	}
	if native.Summary.Errors != embedded.Summary.Errors || native.Summary.Warnings != embedded.Summary.Warnings {
		return fmt.Errorf("diagnostic count mismatch: %+v %+v", native.Summary, embedded.Summary)
	}
	if len(native.Diagnostics) != len(embedded.Diagnostics) {
		return fmt.Errorf("diagnostic length mismatch %d != %d", len(native.Diagnostics), len(embedded.Diagnostics))
	}
	for i, d := range native.Diagnostics {
		w := embedded.Diagnostics[i]
		if d.Code != w.Code || d.Severity != w.Severity || d.Tier0Rule != w.Tier0Rule || d.Span != w.Span {
			return fmt.Errorf("diagnostic %d differs: native=%+v wasm=%+v", i, d, w)
		}
	}
	return nil
}

func execute(cfg settings) (*report, error) {
	if cfg.files <= 0 || cfg.edits <= 0 || cfg.warmup < 0 {
		return nil, errors.New("invalid files, edits or warmup count")
	}
	templateBytes, err := os.ReadFile(cfg.fixture)
	if err != nil {
		return nil, fmt.Errorf("fixture: %w", err)
	}
	template := string(templateBytes)
	expected, err := expectedVerdict(template)
	if err != nil {
		return nil, err
	}
	fixtureHash, _, err := artifactInfo(cfg.fixture)
	if err != nil {
		return nil, err
	}
	wasmHash, wasmSize, err := artifactInfo(cfg.wasm)
	if err != nil {
		return nil, err
	}
	nativeHash, nativeSize, err := artifactInfo(cfg.native)
	if err != nil {
		return nil, err
	}
	workspace, err := os.MkdirTemp("", "w012-editbench-")
	if err != nil {
		return nil, err
	}
	defer os.RemoveAll(workspace)
	if err := os.WriteFile(filepath.Join(workspace, "go.mod"), []byte("module example.com/w012-editbench\n\ngo 1.26.9\n"), 0600); err != nil {
		return nil, err
	}
	if err := os.WriteFile(filepath.Join(workspace, "anchor.go"), []byte("package editbench\n"), 0600); err != nil {
		return nil, err
	}
	paths := make([]string, cfg.files)
	for i := range paths {
		paths[i] = filepath.Join(workspace, fmt.Sprintf("file-%03d.hy", i))
		if err := os.WriteFile(paths[i], []byte(editedSource(template, 0)), 0600); err != nil {
			return nil, err
		}
	}
	coldStart := time.Now()
	engine, err := analyzer.GetEngine(cfg.wasm)
	if err != nil {
		return nil, fmt.Errorf("cold WASM init: %w", err)
	}
	coldMS := float64(time.Since(coldStart).Nanoseconds()) / 1e6
	reused, coldReusedMS, err := newExperimentalGuest(cfg.wasm)
	if err != nil {
		return nil, fmt.Errorf("initialize experimental guest: %w", err)
	}
	defer reused.Close()
	// Mixed-verdict controls expose simple cross-request state residue.
	checkOpposite := func() error {
		other := "tests/checker-corpus/09-a3-map-slot-valid.hy"
		if expected == "accept" {
			other = "tests/checker-corpus/08-a3-map-slot-invalidated.hy"
		}
		file := filepath.Join(cfg.root, other)
		data, err := os.ReadFile(file)
		if err != nil {
			return err
		}
		want, err := expectedVerdict(string(data))
		if err != nil {
			return err
		}
		n, _, err := nativeCheck(cfg.native, file)
		if err != nil {
			return err
		}
		w, _, err := wasmCheck(engine, string(data))
		if err != nil {
			return err
		}
		e, _, err := reused.Check(string(data))
		if err != nil {
			return err
		}
		if err := comparable(n, w, want); err != nil {
			return err
		}
		return comparable(n, e, want)
	}
	if err := checkOpposite(); err != nil {
		return nil, fmt.Errorf("before-run cross-verdict control: %w", err)
	}
	for i := 0; i < cfg.warmup; i++ {
		src := editedSource(template, -i-1)
		if err := os.WriteFile(paths[0], []byte(src), 0600); err != nil {
			return nil, err
		}
		n, _, err := nativeCheck(cfg.native, paths[0])
		if err != nil {
			return nil, err
		}
		w, _, err := wasmCheck(engine, src)
		if err != nil {
			return nil, err
		}
		e, _, err := reused.Check(src)
		if err != nil {
			return nil, err
		}
		if err := comparable(n, w, expected); err != nil {
			return nil, fmt.Errorf("warmup %d: %w", i, err)
		}
		if err := comparable(n, e, expected); err != nil {
			return nil, fmt.Errorf("reused warmup %d: %w", i, err)
		}
	}
	samples := make([]sample, 0, cfg.edits)
	nativeTimes := make([]float64, 0, cfg.edits)
	wasmTimes := make([]float64, 0, cfg.edits)
	reusedTimes := make([]float64, 0, cfg.edits)
	for i := 0; i < cfg.edits; i++ {
		index := i % cfg.files
		src := editedSource(template, i+1)
		if err := os.WriteFile(paths[index], []byte(src), 0600); err != nil {
			return nil, err
		}
		var n, w, e *analyzer.CheckOutput
		var nt, wt, et float64
		order := [][3]int{{0, 1, 2}, {1, 2, 0}, {2, 0, 1}}[i%3]
		first := []string{"native", "embedded", "experimental-reused"}[order[0]]
		for _, mode := range order {
			switch mode {
			case 0:
				n, nt, err = nativeCheck(cfg.native, paths[index])
			case 1:
				w, wt, err = wasmCheck(engine, src)
			case 2:
				e, et, err = reused.Check(src)
			}
			if err != nil {
				return nil, fmt.Errorf("edit %d request path %d: %w", i+1, mode, err)
			}
		}
		if err := comparable(n, w, expected); err != nil {
			return nil, fmt.Errorf("edit %d public path: %w", i+1, err)
		}
		if err := comparable(n, e, expected); err != nil {
			return nil, fmt.Errorf("edit %d reused guest: %w", i+1, err)
		}
		samples = append(samples, sample{Edit: i + 1, File: filepath.Base(paths[index]), First: first, NativeMS: nt, InProcessMS: wt, ExperimentalReusedMS: et})
		nativeTimes = append(nativeTimes, nt)
		wasmTimes = append(wasmTimes, wt)
		reusedTimes = append(reusedTimes, et)
	}
	if err := checkOpposite(); err != nil {
		return nil, fmt.Errorf("after-run cross-verdict control: %w", err)
	}
	r := &report{
		Kind:         "single-file-after-write request proxy, NOT gopls/LSP",
		TimestampUTC: time.Now().UTC().Format(time.RFC3339),
		OS:           runtime.GOOS, Arch: runtime.GOARCH, Go: runtime.Version(), CPUs: runtime.NumCPU(), GOMAXPROCS: runtime.GOMAXPROCS(0),
		Files: cfg.files, Edits: cfg.edits, Warmup: cfg.warmup,
		FixtureSHA256: fixtureHash, WASMSHA256: wasmHash, NativeSHA256: nativeHash, WASMBytes: wasmSize, NativeBytes: nativeSize,
		ColdWASMInitMS: coldMS, ColdReusedInitMS: coldReusedMS, Native: summarize(nativeTimes), InProcess: summarize(wasmTimes), ReusedGuest: summarize(reusedTimes), Samples: samples,
		Limitations: []string{
			"Not stock gopls, not unsaved buffers, not editor keystroke-to-diagnostic latency.",
			"Each request checks the edited file only, not all 25 workspace files.",
			"Durations begin after the write; native includes subprocess, file read and envelope decode; embedded receives in-memory source bytes.",
			"Cold WASM initialization is separate from warm Engine.Check timings.",
			"The experimental reused guest is strictly sequential and not a production instance pool; cancellation, concurrency and lifetime safety remain unproven.",
			"One fixture and one CI machine are insufficient for population-level performance or memory claims.",
		},
	}
	if r.InProcess.P50 > 0 {
		r.NativeOverInProcessP50 = r.Native.P50 / r.InProcess.P50
	}
	if r.ReusedGuest.P50 > 0 {
		r.NativeOverReusedP50 = r.Native.P50 / r.ReusedGuest.P50
	}
	if cfg.output != "" {
		b, err := json.MarshalIndent(r, "", "  ")
		if err != nil {
			return nil, err
		}
		if err := os.WriteFile(cfg.output, append(b, '\n'), 0600); err != nil {
			return nil, err
		}
	}
	return r, nil
}

func main() {
	root := flag.String("root", ".", "repository root")
	wasm := flag.String("wasm", os.Getenv("HYBRID_WASM_PATH"), "WASM artifact")
	native := flag.String("native", os.Getenv("HYBRID_NATIVE_PATH"), "native CLI")
	fixture := flag.String("fixture", "", "checker corpus .hy source")
	files := flag.Int("files", 25, "workspace file count")
	edits := flag.Int("edits", 100, "sequential edits")
	warmup := flag.Int("warmup", 10, "untimed checks per path")
	output := flag.String("output", "", "raw JSON output")
	flag.Parse()
	if *wasm == "" {
		*wasm = filepath.Join(*root, "compiler/target/wasm32-unknown-unknown/release/hy_wasm.wasm")
	}
	if *native == "" {
		*native = filepath.Join(*root, "compiler/target/release/hyb")
	}
	if *fixture == "" {
		*fixture = filepath.Join(*root, "tests/checker-corpus/08-a3-map-slot-invalidated.hy")
	}
	for _, p := range []*string{wasm, native, fixture} {
		path, err := filepath.Abs(*p)
		if err != nil {
			fmt.Fprintln(os.Stderr, err)
			os.Exit(2)
		}
		*p = path
	}
	r, err := execute(settings{root: *root, wasm: *wasm, native: *native, fixture: *fixture, output: *output, files: *files, edits: *edits, warmup: *warmup})
	if err != nil {
		fmt.Fprintln(os.Stderr, "W-012 edit benchmark FAILED:", err)
		os.Exit(1)
	}
	fmt.Printf("W-012 after-write request proxy (NOT gopls): %d edits, %d files\n", r.Edits, r.Files)
	fmt.Printf("WASM cold initialization %.3f ms; bytes WASM=%d native=%d\n", r.ColdWASMInitMS, r.WASMBytes, r.NativeBytes)
	fmt.Printf("Native subprocess+decode: p50 %.3f ms p95 %.3f ms p99 %.3f ms\n", r.Native.P50, r.Native.P95, r.Native.P99)
	fmt.Printf("Real public Engine.Check: p50 %.3f ms p95 %.3f ms p99 %.3f ms\n", r.InProcess.P50, r.InProcess.P95, r.InProcess.P99)
	fmt.Printf("Native/embedded p50 ratio %.3fx (above 1: embedded faster)\n", r.NativeOverInProcessP50)
	fmt.Printf("EXPERIMENTAL sequential reused guest: p50 %.3f ms p95 %.3f ms p99 %.3f ms; cold init %.3f ms\n", r.ReusedGuest.P50, r.ReusedGuest.P95, r.ReusedGuest.P99, r.ColdReusedInitMS)
	fmt.Printf("Native/experimental reused p50 %.3fx (research only)\n", r.NativeOverReusedP50)
	fmt.Println("No stock gopls, whole-package latency, or production memory footprint claim.")
}

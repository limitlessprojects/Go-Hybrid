package main

import (
	"context"
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
	"time"

	analyzer "github.com/limitlessprojects/go-hybrid/spikes/w012-analyzer"
	"github.com/tetratelabs/wazero"
)

type LatencyStats struct {
	Min  float64
	P50  float64
	P90  float64
	P99  float64
	Mean float64
}

func calcStats(samples []float64) LatencyStats {
	if len(samples) == 0 {
		return LatencyStats{}
	}
	sorted := make([]float64, len(samples))
	copy(sorted, samples)
	sort.Float64s(sorted)

	var sum float64
	for _, v := range sorted {
		sum += v
	}

	p := func(percent float64) float64 {
		idx := int(math.Ceil(percent*float64(len(sorted)))) - 1
		if idx < 0 {
			idx = 0
		}
		if idx >= len(sorted) {
			idx = len(sorted) - 1
		}
		return sorted[idx]
	}

	return LatencyStats{
		Min:  sorted[0],
		P50:  p(0.50),
		P90:  p(0.90),
		P99:  p(0.99),
		Mean: sum / float64(len(sorted)),
	}
}

// Paths default to the repository containing the current working directory.
// Environment overrides match the parity and analyzer test harnesses.
func repoRoot() (string, error) {
	dir, err := os.Getwd()
	if err != nil {
		return "", err
	}
	for {
		if _, err := os.Stat(filepath.Join(dir, "compiler", "Cargo.toml")); err == nil {
			return dir, nil
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			return "", errors.New("repository root not found; run inside the checkout or set -root")
		}
		dir = parent
	}
}

// Exit 1 is expected only when the CLI also returns a valid rejection envelope.
// Missing binaries, signals, unexpected exit codes, and malformed output abort
// the benchmark instead of being counted as artificially fast checks.
func nativeReject(ctx context.Context, binary, fixture string) error {
	output, err := exec.CommandContext(ctx, binary, "check", fixture, "--json").Output()
	return validateNativeRejection(output, err)
}

func validateNativeRejection(output []byte, runErr error) error {
	var exitErr *exec.ExitError
	if !errors.As(runErr, &exitErr) || exitErr.ExitCode() != 1 {
		return fmt.Errorf("native CLI must reject with exit 1; got %v", runErr)
	}
	out, err := analyzer.DecodeCheckOutput(output)
	if err != nil {
		return fmt.Errorf("native CLI returned an invalid diagnostic envelope: %w", err)
	}
	if out.Summary.Verdict != "reject" {
		return fmt.Errorf("native CLI exit 1 disagrees with verdict %q", out.Summary.Verdict)
	}
	return nil
}

// A slower warm path has no finite amortization point; do not print a negative
// or infinite check count. This is an estimate for compilation alone.
func compileBreakEven(overhead, savings float64) string {
	if savings <= 0 {
		return "no finite break-even (warm path is not faster)"
	}
	n := overhead / savings
	return fmt.Sprintf("%.1f checks (ceil: %.0f checks)", n, math.Ceil(n))
}

func main() {
	ctx := context.Background()
	defaultRoot, _ := repoRoot()
	root := flag.String("root", defaultRoot, "repository root")
	wasmFlag := flag.String("wasm", os.Getenv("HYBRID_WASM_PATH"), "WASM artifact (or HYBRID_WASM_PATH)")
	nativeFlag := flag.String("native", os.Getenv("HYBRID_NATIVE_PATH"), "native CLI (or HYBRID_NATIVE_PATH)")
	fixtureFlag := flag.String("fixture", "", "reject fixture")
	iterationsFlag := flag.Int("iterations", 100, "samples per case")
	outputFlag := flag.String("output", "", "write raw samples and environment metadata as JSON")
	flag.Parse()
	if *root == "" {
		panic("repository root not found; use -root")
	}
	if *iterationsFlag < 1 {
		panic("iterations must be positive")
	}
	wasmPath := *wasmFlag
	if wasmPath == "" {
		wasmPath = filepath.Join(*root, "compiler/target/wasm32-unknown-unknown/release/hy_wasm.wasm")
	}
	hybPath := *nativeFlag
	if hybPath == "" {
		hybPath = filepath.Join(*root, "compiler/target/release/hyb")
	}
	testHyPath := *fixtureFlag
	if testHyPath == "" {
		testHyPath = filepath.Join(*root, "tests/checker-corpus/08-a3-map-slot-invalidated.hy")
	}
	wasmBytes, err := os.ReadFile(wasmPath)
	if err != nil {
		panic(err)
	}
	testHyBytes, err := os.ReadFile(testHyPath)
	if err != nil {
		panic(err)
	}
	testHyStr := string(testHyBytes)
	iterations := *iterationsFlag
	fmt.Printf("Go=%s OS=%s arch=%s GOMAXPROCS=%d\nWASM=%s\nNative=%s\nFixture=%s\n", runtime.Version(), runtime.GOOS, runtime.GOARCH, runtime.GOMAXPROCS(0), wasmPath, hybPath, testHyPath)

	fmt.Println("================================================================================")
	fmt.Printf("W-012 HARDENING BENCHMARK: Wazero vs Native Subprocess (%d iterations/case)\n", iterations)
	fmt.Println("================================================================================")

	// 0. Native Subprocess Benchmark
	var subprocSamples []float64
	for i := 0; i < iterations; i++ {
		t0 := time.Now()
		if err := nativeReject(ctx, hybPath, testHyPath); err != nil {
			panic(err)
		}
		subprocSamples = append(subprocSamples, float64(time.Since(t0).Microseconds())/1000.0)
	}
	subprocStats := calcStats(subprocSamples)
	fmt.Printf("[Baseline] Native CLI Subprocess:   min=%.3fms, p50=%.3fms, p90=%.3fms, p99=%.3fms, mean=%.3fms\n",
		subprocStats.Min, subprocStats.P50, subprocStats.P90, subprocStats.P99, subprocStats.Mean)

	// CASE A: Same process / fresh runtime / no compilation cache (not process startup)
	var caseACompile, caseAInstantiate, caseACheck, caseATotal []float64
	for i := 0; i < iterations; i++ {
		t0 := time.Now()
		r := wazero.NewRuntimeWithConfig(ctx, wazero.NewRuntimeConfig().WithCloseOnContextDone(true))

		tCompileStart := time.Now()
		compiled, err := r.CompileModule(ctx, wasmBytes)
		if err != nil {
			panic(err)
		}
		tCompile := float64(time.Since(tCompileStart).Microseconds()) / 1000.0

		tInstStart := time.Now()
		mod, err := r.InstantiateModule(ctx, compiled, wazero.NewModuleConfig())
		if err != nil {
			panic(err)
		}
		tInst := float64(time.Since(tInstStart).Microseconds()) / 1000.0

		abi, err := analyzer.NewWazeroInstanceABI(mod)
		if err != nil {
			panic(err)
		}

		tCheckStart := time.Now()
		res := analyzer.CheckSourceWithABI(ctx, abi, testHyStr)
		if res.Verdict != analyzer.VerdictReject {
			panic("expected reject")
		}
		tCheck := float64(time.Since(tCheckStart).Microseconds()) / 1000.0

		_ = mod.Close(ctx)
		_ = r.Close(ctx)

		tTotal := float64(time.Since(t0).Microseconds()) / 1000.0

		caseACompile = append(caseACompile, tCompile)
		caseAInstantiate = append(caseAInstantiate, tInst)
		caseACheck = append(caseACheck, tCheck)
		caseATotal = append(caseATotal, tTotal)
	}
	sACompile := calcStats(caseACompile)
	sAInst := calcStats(caseAInstantiate)
	sACheck := calcStats(caseACheck)
	sATotal := calcStats(caseATotal)

	fmt.Printf("[CASE A] Fresh Runtime (No Cache):  total_mean=%.3fms (compile=%.3fms, inst=%.3fms, check=%.3fms)\n",
		sATotal.Mean, sACompile.Mean, sAInst.Mean, sACheck.Mean)
	fmt.Printf("         Compile Stats: min=%.3fms, p50=%.3fms, p90=%.3fms, p99=%.3fms, mean=%.3fms\n",
		sACompile.Min, sACompile.P50, sACompile.P90, sACompile.P99, sACompile.Mean)

	// CASE B: Same process / same wazero.Runtime / compiled module reused
	var caseBInst, caseBCheck, caseBTotal []float64
	rB := wazero.NewRuntimeWithConfig(ctx, wazero.NewRuntimeConfig().WithCloseOnContextDone(true))
	compiledB, err := rB.CompileModule(ctx, wasmBytes)
	if err != nil {
		panic(err)
	}

	for i := 0; i < iterations; i++ {
		t0 := time.Now()
		mod, err := rB.InstantiateModule(ctx, compiledB, wazero.NewModuleConfig())
		if err != nil {
			panic(err)
		}
		tInst := float64(time.Since(t0).Microseconds()) / 1000.0

		abi, err := analyzer.NewWazeroInstanceABI(mod)
		if err != nil {
			panic(err)
		}

		tCheckStart := time.Now()
		res := analyzer.CheckSourceWithABI(ctx, abi, testHyStr)
		if res.Verdict != analyzer.VerdictReject {
			panic("expected reject")
		}
		tCheck := float64(time.Since(tCheckStart).Microseconds()) / 1000.0

		_ = mod.Close(ctx)
		tTotal := float64(time.Since(t0).Microseconds()) / 1000.0

		caseBInst = append(caseBInst, tInst)
		caseBCheck = append(caseBCheck, tCheck)
		caseBTotal = append(caseBTotal, tTotal)
	}
	_ = rB.Close(ctx)
	sBInst := calcStats(caseBInst)
	sBCheck := calcStats(caseBCheck)
	sBTotal := calcStats(caseBTotal)

	fmt.Printf("[CASE B] Reused CompiledModule:     total_mean=%.3fms (inst=%.3fms, check=%.3fms)\n",
		sBTotal.Mean, sBInst.Mean, sBCheck.Mean)
	fmt.Printf("         Total Stats:   min=%.3fms, p50=%.3fms, p90=%.3fms, p99=%.3fms, mean=%.3fms\n",
		sBTotal.Min, sBTotal.P50, sBTotal.P90, sBTotal.P99, sBTotal.Mean)

	// CASE C: New runtime / shared in-memory compilation cache
	memCache := wazero.NewCompilationCache()
	// Warm cache once
	rWarm := wazero.NewRuntimeWithConfig(ctx, wazero.NewRuntimeConfig().WithCloseOnContextDone(true).WithCompilationCache(memCache))
	cWarm, err := rWarm.CompileModule(ctx, wasmBytes)
	if err != nil {
		panic(err)
	}
	_ = cWarm.Close(ctx)
	_ = rWarm.Close(ctx)

	var caseCCompile, caseCInst, caseCCheck, caseCTotal []float64
	for i := 0; i < iterations; i++ {
		t0 := time.Now()
		rC := wazero.NewRuntimeWithConfig(ctx, wazero.NewRuntimeConfig().WithCloseOnContextDone(true).WithCompilationCache(memCache))

		tCompStart := time.Now()
		compiledC, err := rC.CompileModule(ctx, wasmBytes)
		if err != nil {
			panic(err)
		}
		tComp := float64(time.Since(tCompStart).Microseconds()) / 1000.0

		tInstStart := time.Now()
		mod, err := rC.InstantiateModule(ctx, compiledC, wazero.NewModuleConfig())
		if err != nil {
			panic(err)
		}
		tInst := float64(time.Since(tInstStart).Microseconds()) / 1000.0

		abi, err := analyzer.NewWazeroInstanceABI(mod)
		if err != nil {
			panic(err)
		}

		tCheckStart := time.Now()
		res := analyzer.CheckSourceWithABI(ctx, abi, testHyStr)
		if res.Verdict != analyzer.VerdictReject {
			panic("expected reject")
		}
		tCheck := float64(time.Since(tCheckStart).Microseconds()) / 1000.0

		_ = mod.Close(ctx)
		_ = rC.Close(ctx)
		tTotal := float64(time.Since(t0).Microseconds()) / 1000.0

		caseCCompile = append(caseCCompile, tComp)
		caseCInst = append(caseCInst, tInst)
		caseCCheck = append(caseCCheck, tCheck)
		caseCTotal = append(caseCTotal, tTotal)
	}
	_ = memCache.Close(ctx)
	sCComp := calcStats(caseCCompile)
	sCInst := calcStats(caseCInst)
	sCCheck := calcStats(caseCCheck)
	sCTotal := calcStats(caseCTotal)

	fmt.Printf("[CASE C] New Runtime (In-Memory Cache): total_mean=%.3fms (compile=%.3fms, inst=%.3fms, check=%.3fms)\n",
		sCTotal.Mean, sCComp.Mean, sCInst.Mean, sCCheck.Mean)
	fmt.Printf("         Compile Stats: min=%.3fms, p50=%.3fms, p90=%.3fms, p99=%.3fms, mean=%.3fms\n",
		sCComp.Min, sCComp.P50, sCComp.P90, sCComp.P99, sCComp.Mean)

	// CASE D: Filesystem compilation cache
	var caseDCompile, caseDInst, caseDCheck, caseDTotal []float64
	cacheDir, err := os.MkdirTemp("", "w012-wazero-cache-*")
	if err != nil {
		panic(err)
	}
	defer os.RemoveAll(cacheDir)

	fsCache, err := wazero.NewCompilationCacheWithDir(cacheDir)
	if err != nil {
		fmt.Printf("[CASE D] Filesystem cache NOT TESTED: %v\n", err)
	} else {
		// Warm fs cache once
		rWarmFS := wazero.NewRuntimeWithConfig(ctx, wazero.NewRuntimeConfig().WithCloseOnContextDone(true).WithCompilationCache(fsCache))
		cWarmFS, err := rWarmFS.CompileModule(ctx, wasmBytes)
		if err != nil {
			panic(err)
		}
		_ = cWarmFS.Close(ctx)
		_ = rWarmFS.Close(ctx)
		_ = fsCache.Close(ctx)

		for i := 0; i < iterations; i++ {
			t0 := time.Now()
			// Open a new cache handle in the same process; this is not process startup.
			fsHandle, err := wazero.NewCompilationCacheWithDir(cacheDir)
			if err != nil {
				panic(err)
			}

			rD := wazero.NewRuntimeWithConfig(ctx, wazero.NewRuntimeConfig().WithCloseOnContextDone(true).WithCompilationCache(fsHandle))

			tCompStart := time.Now()
			compiledD, err := rD.CompileModule(ctx, wasmBytes)
			if err != nil {
				panic(err)
			}
			tComp := float64(time.Since(tCompStart).Microseconds()) / 1000.0

			tInstStart := time.Now()
			mod, err := rD.InstantiateModule(ctx, compiledD, wazero.NewModuleConfig())
			if err != nil {
				panic(err)
			}
			tInst := float64(time.Since(tInstStart).Microseconds()) / 1000.0

			abi, err := analyzer.NewWazeroInstanceABI(mod)
			if err != nil {
				panic(err)
			}

			tCheckStart := time.Now()
			res := analyzer.CheckSourceWithABI(ctx, abi, testHyStr)
			if res.Verdict != analyzer.VerdictReject {
				panic("expected reject")
			}
			tCheck := float64(time.Since(tCheckStart).Microseconds()) / 1000.0

			_ = mod.Close(ctx)
			_ = rD.Close(ctx)
			_ = fsHandle.Close(ctx)
			tTotal := float64(time.Since(t0).Microseconds()) / 1000.0

			caseDCompile = append(caseDCompile, tComp)
			caseDInst = append(caseDInst, tInst)
			caseDCheck = append(caseDCheck, tCheck)
			caseDTotal = append(caseDTotal, tTotal)
		}
		sDComp := calcStats(caseDCompile)
		sDInst := calcStats(caseDInst)
		sDCheck := calcStats(caseDCheck)
		sDTotal := calcStats(caseDTotal)

		fmt.Printf("[CASE D] New Runtime (Filesystem Cache): total_mean=%.3fms (compile=%.3fms, inst=%.3fms, check=%.3fms)\n",
			sDTotal.Mean, sDComp.Mean, sDInst.Mean, sDCheck.Mean)
		fmt.Printf("         Compile Stats: min=%.3fms, p50=%.3fms, p90=%.3fms, p99=%.3fms, mean=%.3fms\n",
			sDComp.Min, sDComp.P50, sDComp.P90, sDComp.P99, sDComp.Mean)
	}

	// CASE E: Already-compiled and instantiated module / repeated checks (warm check latency)
	rE := wazero.NewRuntimeWithConfig(ctx, wazero.NewRuntimeConfig().WithCloseOnContextDone(true))
	compiledE, err := rE.CompileModule(ctx, wasmBytes)
	if err != nil {
		panic(err)
	}
	modE, err := rE.InstantiateModule(ctx, compiledE, wazero.NewModuleConfig())
	if err != nil {
		panic(err)
	}
	abiE, err := analyzer.NewWazeroInstanceABI(modE)
	if err != nil {
		panic(err)
	}

	var caseEWarm []float64
	for i := 0; i < iterations; i++ {
		t0 := time.Now()
		res := analyzer.CheckSourceWithABI(ctx, abiE, testHyStr)
		if res.Verdict != analyzer.VerdictReject {
			panic("expected reject")
		}
		caseEWarm = append(caseEWarm, float64(time.Since(t0).Microseconds())/1000.0)
	}
	_ = modE.Close(ctx)
	_ = rE.Close(ctx)
	sEWarm := calcStats(caseEWarm)
	fmt.Printf("[CASE E] Validated Reused Instance: min=%.3fms, p50=%.3fms, p90=%.3fms, p99=%.3fms, mean=%.3fms\n", sEWarm.Min, sEWarm.P50, sEWarm.P90, sEWarm.P99, sEWarm.Mean)

	// CASE F: Actual public Engine.Check used by the analysis.Analyzer.
	// Compile once outside the timed loop; unlike case E, each call creates and
	// closes a fresh instance through the production wrapper.
	engine, err := analyzer.GetEngine(wasmPath)
	if err != nil {
		panic(err)
	}
	var caseFEngine []float64
	for i := 0; i < iterations; i++ {
		t0 := time.Now()
		out, err := engine.Check(testHyStr)
		if err != nil {
			panic(err)
		}
		if out.Summary.Verdict != "reject" {
			panic("expected reject")
		}
		caseFEngine = append(caseFEngine, float64(time.Since(t0).Microseconds())/1000.0)
	}
	sFEngine := calcStats(caseFEngine)
	fmt.Printf("[CASE F] Public Engine.Check: min=%.3fms, p50=%.3fms, p90=%.3fms, p99=%.3fms, mean=%.3fms\n", sFEngine.Min, sFEngine.P50, sFEngine.P90, sFEngine.P99, sFEngine.Mean)
	fmt.Printf("         Native/Public Engine.Check p50 ratio: %.2fx (below 1 means native is faster)\n", subprocStats.P50/sFEngine.P50)

	// Break-Even Calculations:
	// Primary: p50-based comparison (robust to scheduler outliers)
	// Secondary: mean-based comparison (outlier-sensitive)

	savingsP50 := subprocStats.P50 - sEWarm.P50

	savingsMean := subprocStats.Mean - sEWarm.Mean

	speedupP50 := subprocStats.P50 / sEWarm.P50
	speedupMean := subprocStats.Mean / sEWarm.Mean

	fmt.Println("================================================================================")
	fmt.Printf("EXPERIMENTAL REUSED-GUEST COMPILE-ONLY BREAK-EVEN (%d ITERATIONS; excludes instantiation/startup; NOT public Engine.Check):\n", iterations)
	fmt.Printf(" [Primary: p50-based Metrics]\n")
	fmt.Printf("  • Native CLI Subprocess p50:                 %.3f ms\n", subprocStats.P50)
	fmt.Printf("  • Validated Reused-Instance WASM p50:             %.3f ms\n", sEWarm.P50)
	fmt.Printf("  • Native / Experimental Reused-Guest Ratio (p50): %.1fx\n", speedupP50)
	fmt.Printf("  • Latency Advantage per Check (delta_T p50): %.3f ms\n", savingsP50)
	fmt.Printf("  • Cold Compile Overhead (T_compile_cold p50):%.3f ms\n", sACompile.P50)
	fmt.Printf("  -> Break-Even (Cold / No Cache, p50):        %s\n", compileBreakEven(sACompile.P50, savingsP50))

	if len(caseDCompile) > 0 {
		sDComp := calcStats(caseDCompile)
		fmt.Printf("  • FS Cached Compile (T_compile_fs p50):      %.3f ms\n", sDComp.P50)
		fmt.Printf("  -> Break-Even (Filesystem Cache, p50):       %s\n", compileBreakEven(sDComp.P50, savingsP50))
	}

	fmt.Println("\n [Secondary: Outlier-Sensitive Mean Metrics]")
	fmt.Printf("  • Native CLI Subprocess Mean:                %.3f ms\n", subprocStats.Mean)
	fmt.Printf("  • Validated Reused-Instance WASM Mean:            %.3f ms\n", sEWarm.Mean)
	fmt.Printf("  • Native / Experimental Reused-Guest Mean Ratio:             %.1fx\n", speedupMean)
	fmt.Printf("  -> Break-Even (Cold / No Cache, Mean):       %s\n", compileBreakEven(sACompile.Mean, savingsMean))
	if len(caseDCompile) > 0 {
		sDComp := calcStats(caseDCompile)
		fmt.Printf("  -> Break-Even (Filesystem Cache, Mean):      %s\n", compileBreakEven(sDComp.Mean, savingsMean))
	}
	if *outputFlag != "" {
		report := map[string]any{
			"go": runtime.Version(), "os": runtime.GOOS, "arch": runtime.GOARCH, "gomaxprocs": runtime.GOMAXPROCS(0),
			"wasm_path": wasmPath, "native_path": hybPath, "fixture": testHyPath, "iterations": iterations, "unit": "milliseconds",
			"native":                        subprocSamples,
			"fresh_runtime":                 map[string]any{"compile": caseACompile, "instantiate": caseAInstantiate, "check": caseACheck, "total": caseATotal},
			"reused_compiled_module":        map[string]any{"instantiate": caseBInst, "check": caseBCheck, "total": caseBTotal},
			"memory_cache":                  map[string]any{"compile": caseCCompile, "instantiate": caseCInst, "check": caseCCheck, "total": caseCTotal},
			"filesystem_cache_same_process": map[string]any{"compile": caseDCompile, "instantiate": caseDInst, "check": caseDCheck, "total": caseDTotal},
			"public_engine_check":           caseFEngine,
			"reused_instance":               caseEWarm,
		}
		data, err := json.MarshalIndent(report, "", "  ")
		if err != nil {
			panic(err)
		}
		if err := os.WriteFile(*outputFlag, append(data, '\n'), 0600); err != nil {
			panic(err)
		}
	}

	fmt.Println("================================================================================")
}

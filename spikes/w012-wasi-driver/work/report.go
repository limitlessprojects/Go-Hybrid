package main

import (
	"bufio"
	"fmt"
	"os"
	"os/exec"
	"runtime"
	"strings"
)

// printReport renders the report as the markdown tables RESULTS.md carries,
// so a reader can diff the committed numbers against a fresh local run.
func printReport(r Report) {
	w := bufio.NewWriter(os.Stdout)
	defer w.Flush()

	if r.Files > 0 && len(r.Parity) > 0 {
		fmt.Fprintf(w, "commit %s\n", r.Commit)
		fmt.Fprintf(w, "parity: corpus %d | native matches %d | in-process matches %d | engines agree %d\n",
			r.Files, r.ParityNativeOK, r.ParityInProcOK, r.ParityAgree)
		for _, row := range r.Parity {
			if row.NativeOK && row.InProcOK && row.EnginesSame {
				continue
			}
			fmt.Fprintf(w, "  MISMATCH %-52s expected %-14s native %-32s in-process %s\n",
				row.File, row.Expected, row.Native, row.InProcess)
		}
	}
	if len(r.Bench) > 0 {
		fmt.Fprintf(w, "wasm %d bytes | cold module compile %.2f ms | workspace %d files\n",
			r.WasmBytes, r.ColdCompileMS, r.BenchFiles)
		fmt.Fprintf(w, "\n%-20s %5s %8s %8s %8s %8s %8s %8s %6s %5s\n",
			"engine", "n", "p50", "p90", "p99", "min", "max", "mean", "drift", "errs")
		for _, b := range r.Bench {
			fmt.Fprintf(w, "%-20s %5d %8.3f %8.3f %8.3f %8.3f %8.3f %8.3f %6d %5d\n",
				b.Engine, b.All.N, b.All.P50, b.All.P90, b.All.P99,
				b.All.Min, b.All.Max, b.All.Mean, b.Drift, b.Errs)
		}
		fmt.Fprintln(w, "\ninvocation floor, no check (ms):")
		for _, b := range r.Bench {
			fmt.Fprintf(w, "  %-20s n=%2d p50=%.3f p90=%.3f min=%.3f max=%.3f\n",
				b.Engine, b.Floor.N, b.Floor.P50, b.Floor.P90, b.Floor.Min, b.Floor.Max)
		}
		fmt.Fprintln(w, "\nper repetition (ms):")
		for _, b := range r.Bench {
			for i, s := range b.Reps {
				fmt.Fprintf(w, "  %-20s rep%d  n=%3d p50=%.3f p90=%.3f p99=%.3f min=%.3f max=%.3f\n",
					b.Engine, i+1, s.N, s.P50, s.P90, s.P99, s.Min, s.Max)
			}
		}
		if len(r.Bench) == 2 {
			native, inproc := r.Bench[0].All, r.Bench[1].All
			if inproc.P50 > 0 {
				fmt.Fprintf(w, "\nratio native/in-process at p50: %.2fx\n", native.P50/inproc.P50)
			}
		}
	}
	fmt.Fprintf(w, "machine: %s\n", r.Machine)
}

// gitCommit names the exact tree the binaries were built from; a spike that
// cannot say which commit it measured is not reproducible.
func gitCommit() string {
	out, err := exec.Command("git", "rev-parse", "--short", "HEAD").Output()
	if err != nil {
		return "unknown"
	}
	return strings.TrimSpace(string(out))
}

func machine() string {
	return fmt.Sprintf("go %s on %s/%s, GOMAXPROCS=%d, %d cpu",
		runtime.Version(), runtime.GOOS, runtime.GOARCH,
		runtime.GOMAXPROCS(0), runtime.NumCPU())
}

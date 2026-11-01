// Command w012wasi measures one question for wildcard W-012 (issue #268):
// does the stock Hybrid checker, compiled to wasm32-wasip1 and run
// in-process through wazero, beat the native subprocess per check — and does
// the WASI boundary preserve every pre-registered corpus verdict?
//
// The module is the unmodified `hyb` binary: no new crate, no custom ABI and
// no export list to maintain, which is the cheap endcap of the proposal
// (the expensive endcap is a purpose-built engine.Release ABI).
//
// Subcommands:
//
//	w012wasi parity -wasm <hyb.wasm> -hyb <hyb> -corpus <dir>
//	w012wasi bench  -wasm <hyb.wasm> -hyb <hyb> -corpus <dir> [-edits 100] [-files 25] [-reps 3]
//	w012wasi all    ...
package main

import (
	"context"
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"time"
)

type config struct {
	wasm   string
	hyb    string
	corpus string
	work   string
	files  int
	edits  int
	reps   int
	out    string
}

func main() {
	if len(os.Args) < 2 {
		usage()
		os.Exit(2)
	}
	cmd := os.Args[1]
	cfg, err := parseFlags(os.Args[2:])
	if err != nil {
		fmt.Fprintf(os.Stderr, "w012wasi: %v\n", err)
		usage()
		os.Exit(2)
	}
	ctx := context.Background()

	var report Report
	switch cmd {
	case "parity":
		report, err = runParity(ctx, cfg)
	case "bench":
		report, err = runBench(ctx, cfg)
	case "all":
		report, err = runParity(ctx, cfg)
		if err == nil {
			var b Report
			b, err = runBench(ctx, cfg)
			report.Bench = b.Bench
			report.BenchFiles = b.BenchFiles
			report.ColdCompileMS = b.ColdCompileMS
		}
	default:
		usage()
		os.Exit(2)
	}
	if err != nil {
		fmt.Fprintf(os.Stderr, "w012wasi: %v\n", err)
		os.Exit(1)
	}
	report.Commit = gitCommit()
	report.Machine = machine()
	printReport(report)
	if cfg.out != "" {
		if err := writeJSON(cfg.out, report); err != nil {
			fmt.Fprintf(os.Stderr, "w012wasi: %v\n", err)
			os.Exit(1)
		}
	}
}

func usage() {
	fmt.Fprint(os.Stderr, `usage:
  w012wasi parity -wasm <hyb.wasm> -hyb <hyb> -corpus <dir>
  w012wasi bench  -wasm <hyb.wasm> -hyb <hyb> -corpus <dir> [-edits 100] [-files 25] [-reps 3]
  w012wasi all    -wasm <hyb.wasm> -hyb <hyb> -corpus <dir>
`)
}

func parseFlags(args []string) (config, error) {
	var cfg config
	fs := flag.NewFlagSet("w012wasi", flag.ContinueOnError)
	fs.StringVar(&cfg.wasm, "wasm", "", "path to hyb.wasm (wasm32-wasip1, release)")
	fs.StringVar(&cfg.hyb, "hyb", "", "path to the native hyb binary from the same commit")
	fs.StringVar(&cfg.corpus, "corpus", "../../../tests/checker-corpus", "corpus directory")
	fs.StringVar(&cfg.work, "work", "", "scratch directory (default: a temp dir)")
	fs.IntVar(&cfg.files, "files", 25, "files in the benchmark workspace")
	fs.IntVar(&cfg.edits, "edits", 100, "sequential edits measured per repetition")
	fs.IntVar(&cfg.reps, "reps", 3, "independent repetitions per engine")
	fs.StringVar(&cfg.out, "out", "", "write the full report as JSON here")
	if err := fs.Parse(args); err != nil {
		return cfg, err
	}
	if cfg.wasm == "" || cfg.hyb == "" {
		return cfg, fmt.Errorf("-wasm and -hyb are both required")
	}
	if cfg.reps < 1 || cfg.edits < 1 || cfg.files < 1 {
		return cfg, fmt.Errorf("files, edits and reps must be positive")
	}
	return cfg, nil
}

// ParityRow is one corpus program's three-way comparison.
type ParityRow struct {
	File        string `json:"file"`
	Expected    string `json:"expected"`
	Native      string `json:"native"`
	InProcess   string `json:"in_process"`
	NativeOK    bool   `json:"native_matches_corpus"`
	InProcOK    bool   `json:"in_process_matches_corpus"`
	EnginesSame bool   `json:"engines_agree"`
}

type benchRow struct {
	Engine string
	Reps   []Stats
	All    Stats
	Floor  Stats // invocation overhead alone (missing input, no check)
	Drift  int   // checks whose verdict moved away from the corpus expectation
	Errs   int
}

type Report struct {
	Commit         string      `json:"commit"`
	Machine        string      `json:"machine"`
	ColdCompileMS  float64     `json:"cold_compile_ms"`
	WasmBytes      int64       `json:"wasm_bytes"`
	Files          int         `json:"parity_files"`
	BenchFiles     int         `json:"bench_files"`
	Parity         []ParityRow `json:"parity"`
	ParityNativeOK int         `json:"parity_native_ok"`
	ParityInProcOK int         `json:"parity_in_process_ok"`
	ParityAgree    int         `json:"parity_engines_agree"`
	Bench          []benchRow  `json:"bench"`
}

func runParity(ctx context.Context, cfg config) (Report, error) {
	corpus, err := LoadCorpus(cfg.corpus)
	if err != nil {
		return Report{}, err
	}
	native := &Native{Bin: cfg.hyb}
	defer native.Close()
	inproc, cold, err := NewInProcess(ctx, cfg.wasm)
	if err != nil {
		return Report{}, err
	}
	defer inproc.Close()

	st, err := os.Stat(cfg.wasm)
	if err != nil {
		return Report{}, err
	}
	rep := Report{ColdCompileMS: ms(cold), WasmBytes: st.Size(), Files: len(corpus)}
	for _, cf := range corpus {
		nc, err := native.Check(ctx, cf.Path)
		if err != nil {
			return rep, fmt.Errorf("native %s: %w", filepath.Base(cf.Path), err)
		}
		ic, err := inproc.Check(ctx, cf.Path)
		if err != nil {
			return rep, fmt.Errorf("in-process %s: %w", filepath.Base(cf.Path), err)
		}
		row := ParityRow{
			File:      filepath.Base(cf.Path),
			Expected:  expectation(cf.Expected),
			Native:    nc.Canon(),
			InProcess: ic.Canon(),
			NativeOK:  cf.Expected.Matches(nc),
			InProcOK:  cf.Expected.Matches(ic),
		}
		row.EnginesSame = nc.Canon() == ic.Canon()
		if row.NativeOK {
			rep.ParityNativeOK++
		}
		if row.InProcOK {
			rep.ParityInProcOK++
		}
		if row.EnginesSame {
			rep.ParityAgree++
		}
		rep.Parity = append(rep.Parity, row)
	}
	return rep, nil
}

func runBench(ctx context.Context, cfg config) (Report, error) {
	corpus, err := LoadCorpus(cfg.corpus)
	if err != nil {
		return Report{}, err
	}
	inproc, cold, err := NewInProcess(ctx, cfg.wasm)
	if err != nil {
		return Report{}, err
	}
	defer inproc.Close()
	native := &Native{Bin: cfg.hyb}
	defer native.Close()
	st, err := os.Stat(cfg.wasm)
	if err != nil {
		return Report{}, err
	}

	base := cfg.work
	if base == "" {
		base, err = os.MkdirTemp("", "w012-wasi-bench-")
		if err != nil {
			return Report{}, err
		}
		defer os.RemoveAll(base)
	}

	byEngine := map[string]*benchRow{
		native.Name(): {Engine: native.Name()},
		inproc.Name(): {Engine: inproc.Name()},
	}
	var pooled = map[string][]time.Duration{}

	for rep := 0; rep < cfg.reps; rep++ {
		// Alternate engine order each repetition so a machine warming up
		// (page cache, CPU governor) cannot favour one path systematically.
		order := []Engine{native, inproc}
		if rep%2 == 1 {
			order = []Engine{inproc, native}
		}
		for _, eng := range order {
			dir := filepath.Join(base, fmt.Sprintf("rep%d-%s", rep, eng.Name()))
			files, err := Workspace(corpus, dir, cfg.files)
			if err != nil {
				return Report{}, err
			}
			row := byEngine[eng.Name()]
			samples := make([]time.Duration, 0, cfg.edits)
			for i := 0; i < cfg.edits; i++ {
				path := files[i%len(files)]
				if err := Edit(path, i+1); err != nil {
					return Report{}, err
				}
				c, err := eng.Check(ctx, path)
				if err != nil {
					row.Errs++
					continue
				}
				samples = append(samples, c.Wall)
				cf, err := parseCorpusFile(path, mustRead(path))
				if err != nil {
					return Report{}, err
				}
				if !cf.Expected.Matches(c) {
					row.Drift++
				}
			}
			row.Reps = append(row.Reps, Summarize(samples))
			pooled[eng.Name()] = append(pooled[eng.Name()], samples...)
		}
	}

	// Boundary floor: the same two paths, pointed at a missing input. The
	// difference between a full check and this floor is the checker itself.
	const probes = 30
	missing := filepath.Join(base, "no-such-input.hy")
	for _, eng := range []Engine{native, inproc} {
		row := byEngine[eng.Name()]
		row.All = Summarize(pooled[eng.Name()])
		samples := make([]time.Duration, 0, probes)
		for i := 0; i < probes; i++ {
			d, err := probe(ctx, eng, missing)
			if err != nil {
				return Report{}, fmt.Errorf("probe %s: %w", eng.Name(), err)
			}
			samples = append(samples, d)
		}
		row.Floor = Summarize(samples)
	}

	var out []benchRow
	for _, name := range []string{native.Name(), inproc.Name()} {
		out = append(out, *byEngine[name])
	}
	return Report{ColdCompileMS: ms(cold), WasmBytes: st.Size(), BenchFiles: cfg.files, Bench: out}, nil
}

func expectation(e ExpectedVerdict) string {
	if e.Managed {
		return "accept+managed"
	}
	return e.Verdict
}

func mustRead(path string) string {
	b, err := os.ReadFile(path)
	if err != nil {
		panic(err)
	}
	return string(b)
}

func writeJSON(path string, rep Report) error {
	b, err := json.MarshalIndent(rep, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(path, append(b, '\n'), 0o644)
}

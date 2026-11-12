package main

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"os/exec"
	"sort"
	"time"

	"github.com/tetratelabs/wazero"
	"github.com/tetratelabs/wazero/imports/wasi_snapshot_preview1"
	"github.com/tetratelabs/wazero/sys"
)

// Diag is one diagnostic reduced to the fields a verdict comparison needs.
type Diag struct {
	Code     string `json:"code"`
	Severity string `json:"severity"`
	Line     int    `json:"line"`
	Col      int    `json:"col"`
}

// Check is one engine run over one file: the checker's answer plus the wall
// time it took to produce it. Wall includes everything the caller pays for
// that path (process spawn + stdio + JSON for native; WASI instantiation +
// module execution + stdout capture for in-process).
type Check struct {
	Wall    time.Duration `json:"wall_ns"`
	Verdict string        `json:"verdict"`
	Managed bool          `json:"managed"`
	Exit    int           `json:"exit"`
	Diags   []Diag        `json:"diags"`
}

// Envelope is the subset of `hyb check --json` this spike reads.
type envelope struct {
	Diagnostics []struct {
		Code     string `json:"code"`
		Severity string `json:"severity"`
		Span     struct {
			Start struct {
				Line int `json:"line"`
				Col  int `json:"col"`
			} `json:"start"`
		} `json:"span"`
	} `json:"diagnostics"`
	Summary struct {
		Verdict string `json:"verdict"`
		Errors  int    `json:"errors"`
	} `json:"summary"`
}

// Engine is one invocation path for the same checker.
type Engine interface {
	Name() string
	Check(ctx context.Context, path string) (Check, error)
	Close()
}

func parseEnvelope(stdout []byte, wall time.Duration, exit int) (Check, error) {
	var env envelope
	dec := json.NewDecoder(bytes.NewReader(stdout))
	if err := dec.Decode(&env); err != nil {
		return Check{}, fmt.Errorf("decode checker JSON: %w", err)
	}
	c := Check{Wall: wall, Verdict: env.Summary.Verdict, Exit: exit}
	for _, d := range env.Diagnostics {
		c.Diags = append(c.Diags, Diag{
			Code:     d.Code,
			Severity: d.Severity,
			Line:     d.Span.Start.Line,
			Col:      d.Span.Start.Col,
		})
		if d.Code == "HYB-E8001" {
			c.Managed = true
		}
	}
	if c.Verdict == "" {
		return Check{}, fmt.Errorf("checker JSON carried no summary.verdict")
	}
	return c, nil
}

// Native runs the checker as a subprocess, exactly as an editor extension or
// `hyb check` shell invocation does today.
type Native struct {
	Bin string
}

func (n *Native) Name() string { return "native-subprocess" }

func (n *Native) Check(ctx context.Context, path string) (Check, error) {
	var stdout, stderr bytes.Buffer
	cmd := exec.CommandContext(ctx, n.Bin, "check", path, "--json")
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr
	start := time.Now()
	err := cmd.Run()
	wall := time.Since(start)
	exit := 0
	if err != nil {
		if ee, ok := err.(*exec.ExitError); ok {
			exit = ee.ExitCode()
		} else {
			return Check{}, fmt.Errorf("run %s: %w", n.Bin, err)
		}
	}
	c, perr := parseEnvelope(stdout.Bytes(), wall, exit)
	if perr != nil {
		// A checker that emits a verdict exits 0 or 1; anything else is an
		// infrastructure failure, and this spike must not paper over it.
		if exit != 0 && exit != 1 {
			return Check{}, fmt.Errorf("exit %d: %w (stderr: %s)", exit, perr, stderr.String())
		}
		return Check{}, perr
	}
	return c, nil
}

func (n *Native) Close() {}

// Probe measures invocation overhead alone: the checker is pointed at a path
// that does not exist, so it does the minimum work it can (parse argv, fail)
// and every measured microsecond is spawn/instantiation, not checking.
func (n *Native) Probe(ctx context.Context, path string) (time.Duration, error) {
	var stdout, stderr bytes.Buffer
	cmd := exec.CommandContext(ctx, n.Bin, "check", path, "--json")
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr
	start := time.Now()
	err := cmd.Run()
	wall := time.Since(start)
	if err != nil {
		if _, ok := err.(*exec.ExitError); !ok {
			return 0, err
		}
	}
	return wall, nil
}

// InProcess runs the same checker compiled to wasm32-wasip1 inside wazero.
// The module is compiled once; every Check instantiates it again because a
// WASI command module's _start runs a single time per instance — that is the
// per-check cost an embedder actually pays without inventing a custom ABI.
type InProcess struct {
	runtime  wazero.Runtime
	compiled wazero.CompiledModule
}

func NewInProcess(ctx context.Context, wasmPath string) (*InProcess, time.Duration, error) {
	wasmBytes, err := os.ReadFile(wasmPath)
	if err != nil {
		return nil, 0, fmt.Errorf("read wasm: %w", err)
	}
	r := wazero.NewRuntime(ctx)
	wasi_snapshot_preview1.MustInstantiate(ctx, r)
	start := time.Now()
	compiled, err := r.CompileModule(ctx, wasmBytes)
	cold := time.Since(start)
	if err != nil {
		r.Close(ctx)
		return nil, 0, fmt.Errorf("compile wasm: %w", err)
	}
	return &InProcess{runtime: r, compiled: compiled}, cold, nil
}

func (w *InProcess) Name() string { return "in-process-wasi" }

// Preopen is the host directory tree the guest may read; "/" keeps the spike
// free of path translation so both engines see identical absolute paths.
func (w *InProcess) Check(ctx context.Context, path string) (Check, error) {
	var stdout, stderr bytes.Buffer
	cfg := wazero.NewModuleConfig().
		WithName(""). // anonymous: one fresh instance per check, never a registry entry
		WithArgs("hyb", "check", path, "--json").
		WithStdout(&stdout).
		WithStderr(&stderr).
		WithFS(os.DirFS("/"))
	start := time.Now()
	mod, err := w.runtime.InstantiateModule(ctx, w.compiled, cfg)
	wall := time.Since(start)
	if mod != nil {
		// Teardown is outside the measured window: the number reported is
		// instantiate+run, the part an editor waits on. Every instance is
		// closed so a 300-check run does not accumulate guests.
		defer mod.Close(ctx)
	}
	exit := 0
	if err != nil {
		ee, ok := err.(*sys.ExitError)
		if !ok {
			return Check{}, fmt.Errorf("instantiate wasm: %w", err)
		}
		exit = int(ee.ExitCode())
	}
	c, perr := parseEnvelope(stdout.Bytes(), wall, exit)
	if perr != nil {
		return Check{}, fmt.Errorf("exit %d: %w (stderr: %s)", exit, perr, stderr.String())
	}
	return c, nil
}

func (w *InProcess) Close() { w.runtime.Close(context.Background()) }

// Probe is the in-process counterpart of Native.Probe: instantiate and run
// against a missing path, measuring the boundary cost without the check.
func (w *InProcess) Probe(ctx context.Context, path string) (time.Duration, error) {
	cfg := wazero.NewModuleConfig().
		WithName("").
		WithArgs("hyb", "check", path, "--json").
		WithStdout(io.Discard).
		WithStderr(io.Discard).
		WithFS(os.DirFS("/"))
	start := time.Now()
	mod, err := w.runtime.InstantiateModule(ctx, w.compiled, cfg)
	wall := time.Since(start)
	if mod != nil {
		defer mod.Close(ctx)
	}
	if err != nil {
		if _, ok := err.(*sys.ExitError); !ok {
			return 0, err
		}
	}
	return wall, nil
}

// Canon renders a check as the comparable identity used by parity: verdict,
// managed flag, and every diagnostic's code with its position.
func (c Check) Canon() string {
	diags := make([]string, 0, len(c.Diags))
	for _, d := range c.Diags {
		diags = append(diags, fmt.Sprintf("%s@%d:%d", d.Code, d.Line, d.Col))
	}
	sort.Strings(diags)
	return fmt.Sprintf("%s managed=%t %v", c.Verdict, c.Managed, diags)
}

// ExpectedVerdict is the corpus-side expectation for one program.
type ExpectedVerdict struct {
	Verdict string // accept | reject
	Managed bool   // ACCEPT-MANAGED
}

// Matches applies the corpus agreement rule (tests/checker-corpus/README.md):
// ACCEPT needs accept + no error, ACCEPT-MANAGED needs accept + a managed
// lint, REJECT needs reject.
func (e ExpectedVerdict) Matches(c Check) bool {
	if c.Verdict != e.Verdict {
		return false
	}
	if e.Managed && !c.Managed {
		return false
	}
	if !e.Managed && c.Managed {
		return false
	}
	return true
}

// probe dispatches to whichever engines expose the overhead probe; both do.
func probe(ctx context.Context, e Engine, path string) (time.Duration, error) {
	switch v := e.(type) {
	case *Native:
		return v.Probe(ctx, path)
	case *InProcess:
		return v.Probe(ctx, path)
	}
	return 0, fmt.Errorf("engine %s has no probe", e.Name())
}

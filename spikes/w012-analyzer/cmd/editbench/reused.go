package main

import (
	"context"
	"errors"
	"os"
	"time"

	analyzer "github.com/limitlessprojects/go-hybrid/spikes/w012-analyzer"
	"github.com/tetratelabs/wazero"
)

// experimentalGuest is deliberately sequential and never used by the public
// Analyzer. It is a timing control, not a proposal to deploy shared mutable
// guest memory without proven state and cancellation isolation.
type experimentalGuest struct {
	ctx     context.Context
	runtime wazero.Runtime
	abi     *analyzer.WazeroInstanceABI
}

func newExperimentalGuest(path string) (*experimentalGuest, float64, error) {
	start := time.Now()
	wasm, err := os.ReadFile(path)
	if err != nil {
		return nil, 0, err
	}
	ctx := context.Background()
	rt := wazero.NewRuntimeWithConfig(ctx, wazero.NewRuntimeConfig().WithCloseOnContextDone(true))
	compiled, err := rt.CompileModule(ctx, wasm)
	if err != nil {
		_ = rt.Close(ctx)
		return nil, 0, err
	}
	mod, err := rt.InstantiateModule(ctx, compiled, wazero.NewModuleConfig().WithName(""))
	if err != nil {
		_ = rt.Close(ctx)
		return nil, 0, err
	}
	abi, err := analyzer.NewWazeroInstanceABI(mod)
	if err != nil {
		_ = rt.Close(ctx)
		return nil, 0, err
	}
	return &experimentalGuest{ctx: ctx, runtime: rt, abi: abi}, float64(time.Since(start).Nanoseconds()) / 1e6, nil
}

func (g *experimentalGuest) Close() error { return g.runtime.Close(g.ctx) }

func (g *experimentalGuest) Check(src string) (*analyzer.CheckOutput, float64, error) {
	ctx, cancel := context.WithTimeout(g.ctx, 30*time.Second)
	defer cancel()
	start := time.Now()
	res := analyzer.CheckSourceWithABI(ctx, g.abi, src)
	if err := res.CheckInvariant(); err != nil {
		return nil, 0, err
	}
	if res.Verdict == analyzer.VerdictAnalysisFailure {
		return nil, 0, res.Failure
	}
	if res.Summary == nil {
		return nil, 0, errors.New("reused WASM guest returned nil summary")
	}
	return &analyzer.CheckOutput{Version: 1, Summary: *res.Summary, Diagnostics: res.Diagnostics}, float64(time.Since(start).Nanoseconds()) / 1e6, nil
}

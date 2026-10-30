package analyzer

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"

	"github.com/tetratelabs/wazero"
	"github.com/tetratelabs/wazero/api"
)

type AnalysisVerdict string

const (
	VerdictAccept          AnalysisVerdict = "Accept"
	VerdictReject          AnalysisVerdict = "Reject"
	VerdictAnalysisFailure AnalysisVerdict = "AnalysisFailure"
)

type AnalysisResult struct {
	Verdict     AnalysisVerdict
	Diagnostics []Diagnostic
	Failure     error
	Summary     *Summary
}

// Invariant: AnalysisFailure MUST NEVER become Accept, nor masquerade as Reject.
func (r *AnalysisResult) CheckInvariant() error {
	switch r.Verdict {
	case VerdictAccept:
		if r.Failure != nil {
			return errors.New("INVARIANT VIOLATION: Failure present in Accept verdict")
		}
		// Accepted source may still carry managed-safety warnings or notes.
		for _, d := range r.Diagnostics {
			if d.Severity != "warning" && d.Severity != "note" {
				return errors.New("INVARIANT VIOLATION: error or invalid diagnostic in Accept verdict")
			}
		}
	case VerdictReject:
		if r.Failure != nil {
			return errors.New("INVARIANT VIOLATION: Failure present in Reject verdict (infrastructure failure masquerading as user rejection)")
		}
	case VerdictAnalysisFailure:
		if r.Failure == nil {
			return errors.New("INVARIANT VIOLATION: AnalysisFailure without concrete error")
		}
	default:
		return fmt.Errorf("INVARIANT VIOLATION: unknown verdict %s", r.Verdict)
	}
	return nil
}

// WasmABI abstracts the low-level WASM memory and function boundary
type WasmABI interface {
	Alloc(ctx context.Context, byteLen uint64) (uint32, error)
	Free(ctx context.Context, ptr, byteLen uint64) error
	Check(ctx context.Context, ptr, byteLen uint64) (uint64, error)
	WriteMemory(offset uint32, b []byte) bool
	ReadMemory(offset, byteCount uint32) ([]byte, bool)
}

// WazeroInstanceABI implements WasmABI backed by a real Wazero api.Module
type WazeroInstanceABI struct {
	mod     api.Module
	allocFn api.Function
	freeFn  api.Function
	checkFn api.Function
}

func NewWazeroInstanceABI(mod api.Module) (*WazeroInstanceABI, error) {
	allocFn := mod.ExportedFunction("hy_wasm_alloc")
	freeFn := mod.ExportedFunction("hy_wasm_free")
	checkFn := mod.ExportedFunction("hy_wasm_check")

	if allocFn == nil || freeFn == nil || checkFn == nil {
		return nil, errors.New("missing required exports (hy_wasm_alloc, hy_wasm_free, hy_wasm_check)")
	}
	if mod.Memory() == nil {
		return nil, errors.New("missing exported memory")
	}

	return &WazeroInstanceABI{
		mod:     mod,
		allocFn: allocFn,
		freeFn:  freeFn,
		checkFn: checkFn,
	}, nil
}

func (w *WazeroInstanceABI) Alloc(ctx context.Context, byteLen uint64) (uint32, error) {
	res, err := w.allocFn.Call(ctx, byteLen)
	if err != nil {
		return 0, err
	}
	if len(res) == 0 {
		return 0, errors.New("alloc returned empty result")
	}
	ptr := uint32(res[0])
	if ptr == 0 && byteLen > 0 {
		return 0, errors.New("alloc returned null pointer")
	}
	return ptr, nil
}

func (w *WazeroInstanceABI) Free(ctx context.Context, ptr, byteLen uint64) error {
	_, err := w.freeFn.Call(ctx, ptr, byteLen)
	return err
}

func (w *WazeroInstanceABI) Check(ctx context.Context, ptr, byteLen uint64) (uint64, error) {
	res, err := w.checkFn.Call(ctx, ptr, byteLen)
	if err != nil {
		return 0, err
	}
	if len(res) == 0 {
		return 0, errors.New("check returned empty result")
	}
	return res[0], nil
}

func (w *WazeroInstanceABI) WriteMemory(offset uint32, b []byte) bool {
	return w.mod.Memory().Write(offset, b)
}

func (w *WazeroInstanceABI) ReadMemory(offset, byteCount uint32) ([]byte, bool) {
	return w.mod.Memory().Read(offset, byteCount)
}

type WasmEngine struct {
	ctx      context.Context
	runtime  wazero.Runtime
	compiled wazero.CompiledModule
}

func NewWasmEngine(ctx context.Context, wasmBytes []byte) (*WasmEngine, error) {
	r := wazero.NewRuntimeWithConfig(ctx, wazero.NewRuntimeConfig().WithCloseOnContextDone(true))
	compiled, err := r.CompileModule(ctx, wasmBytes)
	if err != nil {
		r.Close(ctx)
		return nil, fmt.Errorf("compilation failed: %w", err)
	}
	return &WasmEngine{
		ctx:      ctx,
		runtime:  r,
		compiled: compiled,
	}, nil
}

func (e *WasmEngine) Close() error {
	return e.runtime.Close(e.ctx)
}

func (e *WasmEngine) CheckSource(ctx context.Context, src string) *AnalysisResult {
	if err := ctx.Err(); err != nil {
		return &AnalysisResult{
			Verdict: VerdictAnalysisFailure,
			Failure: fmt.Errorf("infrastructure error: context timeout before instantiation: %w", err),
		}
	}

	mod, err := e.runtime.InstantiateModule(ctx, e.compiled, wazero.NewModuleConfig().WithName(""))
	if err != nil {
		return &AnalysisResult{
			Verdict: VerdictAnalysisFailure,
			Failure: fmt.Errorf("infrastructure error: module instantiation failed: %w", err),
		}
	}
	defer mod.Close(ctx)

	abi, err := NewWazeroInstanceABI(mod)
	if err != nil {
		return &AnalysisResult{
			Verdict: VerdictAnalysisFailure,
			Failure: fmt.Errorf("infrastructure error: ABI initialization failed: %w", err),
		}
	}

	return CheckSourceWithABI(ctx, abi, src)
}

// CheckSourceWithABI runs analysis against any WasmABI implementation
func CheckSourceWithABI(ctx context.Context, abi WasmABI, src string) *AnalysisResult {
	if err := ctx.Err(); err != nil {
		return &AnalysisResult{
			Verdict: VerdictAnalysisFailure,
			Failure: fmt.Errorf("infrastructure error: execution timeout: %w", err),
		}
	}

	srcBytes := []byte(src)
	srcLen := uint64(len(srcBytes))

	srcPtr, err := abi.Alloc(ctx, srcLen)
	if err != nil {
		return &AnalysisResult{
			Verdict: VerdictAnalysisFailure,
			Failure: fmt.Errorf("infrastructure error: guest memory alloc failed: %w", err),
		}
	}
	// This flag means that cleanup is still due, not that a previous free
	// returned success. A trapped free may already have deallocated memory.
	sourceFreePending := true
	defer func() {
		if sourceFreePending {
			_ = abi.Free(ctx, uint64(srcPtr), srcLen)
		}
	}()

	if !abi.WriteMemory(srcPtr, srcBytes) {
		return &AnalysisResult{
			Verdict: VerdictAnalysisFailure,
			Failure: errors.New("infrastructure error: failed writing source to guest linear memory"),
		}
	}

	packed, err := abi.Check(ctx, uint64(srcPtr), srcLen)
	if err != nil {
		return &AnalysisResult{
			Verdict: VerdictAnalysisFailure,
			Failure: fmt.Errorf("infrastructure error: WASM trap/panic during execution: %w", err),
		}
	}

	// Mark the release as attempted BEFORE invoking guest code. On a trap,
	// retrying from the defer could double-free a partially released buffer.
	sourceFreePending = false
	if err := abi.Free(ctx, uint64(srcPtr), srcLen); err != nil {
		return &AnalysisResult{
			Verdict: VerdictAnalysisFailure,
			Failure: fmt.Errorf("infrastructure error: failed freeing guest source buffer: %w", err),
		}
	}

	if err := ctx.Err(); err != nil {
		return &AnalysisResult{
			Verdict: VerdictAnalysisFailure,
			Failure: fmt.Errorf("infrastructure error: context cancelled post-call: %w", err),
		}
	}

	outPtr := uint32(packed >> 32)
	outLen := uint32(packed & 0xFFFFFFFF)
	if outPtr == 0 || outLen == 0 {
		return &AnalysisResult{
			Verdict: VerdictAnalysisFailure,
			Failure: fmt.Errorf("infrastructure error: invalid guest output buffer: ptr=%d, len=%d", outPtr, outLen),
		}
	}

	outBytes, ok := abi.ReadMemory(outPtr, outLen)
	if !ok {
		return &AnalysisResult{
			Verdict: VerdictAnalysisFailure,
			Failure: fmt.Errorf("infrastructure error: out-of-bounds guest memory read: ptr=%d, len=%d", outPtr, outLen),
		}
	}
	// Wazero exposes a view into guest memory. Copy it before returning ownership
	// of the guest buffer so decoding never observes freed linear memory.
	outCopy := append([]byte(nil), outBytes...)
	if err := abi.Free(ctx, uint64(outPtr), uint64(outLen)); err != nil {
		return &AnalysisResult{
			Verdict: VerdictAnalysisFailure,
			Failure: fmt.Errorf("infrastructure error: failed freeing guest output buffer: %w", err),
		}
	}

	out, err := DecodeCheckOutput(outCopy)
	if err != nil {
		return &AnalysisResult{Verdict: VerdictAnalysisFailure, Failure: fmt.Errorf("infrastructure error: malformed diagnostic envelope: %w", err)}
	}
	if out.Summary.Verdict == "accept" {
		return &AnalysisResult{Verdict: VerdictAccept, Diagnostics: out.Diagnostics, Summary: &out.Summary}
	}
	return &AnalysisResult{Verdict: VerdictReject, Diagnostics: out.Diagnostics, Summary: &out.Summary}
}

// DecodeCheckOutput validates semantic structure, not merely JSON syntax.
// Unknown additive fields remain allowed; missing fields and contradictory
// summaries are infrastructure failures rather than user-code verdicts.
func DecodeCheckOutput(data []byte) (*CheckOutput, error) {
	var wire struct {
		Version     *int          `json:"version"`
		Diagnostics *[]Diagnostic `json:"diagnostics"`
		Summary     *struct {
			Verdict  string `json:"verdict"`
			Errors   *int   `json:"errors"`
			Warnings *int   `json:"warnings"`
		} `json:"summary"`
	}
	if err := json.Unmarshal(data, &wire); err != nil {
		return nil, err
	}
	if wire.Version == nil || *wire.Version != 1 || wire.Diagnostics == nil || wire.Summary == nil || wire.Summary.Errors == nil || wire.Summary.Warnings == nil {
		return nil, errors.New("missing required fields or unsupported envelope version")
	}
	var out CheckOutput
	if err := json.Unmarshal(data, &out); err != nil {
		return nil, err
	}
	errorsCount, warningsCount := 0, 0
	for _, d := range out.Diagnostics {
		if d.Code == "" || d.Message == "" || d.Span.Start.Line < 1 || d.Span.Start.Col < 1 || d.Span.End.Line < d.Span.Start.Line || d.Span.End.Col < 1 || (d.Span.End.Line == d.Span.Start.Line && d.Span.End.Col < d.Span.Start.Col) {
			return nil, errors.New("invalid diagnostic code, message, or span")
		}
		switch d.Severity {
		case "error":
			errorsCount++
		case "warning":
			warningsCount++
		case "note":
		default:
			return nil, fmt.Errorf("unknown diagnostic severity %q", d.Severity)
		}
	}
	if out.Summary.Errors != errorsCount || out.Summary.Warnings != warningsCount {
		return nil, errors.New("summary counts disagree with diagnostics")
	}
	switch out.Summary.Verdict {
	case "accept":
		if errorsCount != 0 {
			return nil, errors.New("accept verdict with error diagnostics")
		}
	case "reject":
		if errorsCount == 0 {
			return nil, errors.New("reject verdict without error diagnostics")
		}
	default:
		return nil, fmt.Errorf("unknown verdict %q", out.Summary.Verdict)
	}
	return &out, nil
}

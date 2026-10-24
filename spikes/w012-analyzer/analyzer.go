package analyzer

import (
	"context"
	"fmt"
	"go/token"
	"os"
	"path/filepath"
	"slices"
	"sort"
	"sync"
	"time"
	"unicode/utf8"

	"github.com/tetratelabs/wazero"
	"golang.org/x/tools/go/analysis"
)

type Pos struct {
	Line int `json:"line"`
	Col  int `json:"col"`
}

type Span struct {
	Start Pos `json:"start"`
	End   Pos `json:"end"`
}

type Diagnostic struct {
	Code      string `json:"code"`
	Severity  string `json:"severity"`
	Message   string `json:"message"`
	Span      Span   `json:"span"`
	Tier0Rule string `json:"tier0_rule,omitempty"`
}

type Summary struct {
	File     string `json:"file"`
	Verdict  string `json:"verdict"`
	Errors   int    `json:"errors"`
	Warnings int    `json:"warnings"`
}

type CheckOutput struct {
	Diagnostics []Diagnostic `json:"diagnostics"`
	Summary     Summary      `json:"summary"`
	Version     int          `json:"version"`
}

type Engine struct {
	ctx      context.Context
	runtime  wazero.Runtime
	compiled wazero.CompiledModule
	cache    wazero.CompilationCache // optional, held until Close
}

var (
	defaultWasmPath  = wasmArtifactPath()
	globalEngine     *Engine
	globalEngineOnce sync.Once
	globalEngineErr  error
)

func wasmArtifactPath() string {
	if path := os.Getenv("HYBRID_WASM_PATH"); path != "" {
		return path
	}
	return filepath.Join("..", "..", "compiler", "target", "wasm32-unknown-unknown", "release", "hy_wasm.wasm")
}

// newEngine loads the same checker as GetEngine with an optional filesystem
// compilation cache. A cache is not an analysis-result cache: every request
// still executes the checker and fail-closed diagnostic validation.
func newEngine(wasmPath, cacheDir string) (*Engine, error) {
	wasmBytes, err := os.ReadFile(wasmPath)
	if err != nil {
		return nil, fmt.Errorf("failed to read wasm binary: %w", err)
	}
	ctx := context.Background()
	config := wazero.NewRuntimeConfig().WithCloseOnContextDone(true)
	var cache wazero.CompilationCache
	if cacheDir != "" {
		if err := os.MkdirAll(cacheDir, 0700); err != nil {
			return nil, fmt.Errorf("prepare WASM compilation cache: %w", err)
		}
		cache, err = wazero.NewCompilationCacheWithDir(cacheDir)
		if err != nil {
			return nil, fmt.Errorf("open WASM compilation cache: %w", err)
		}
		config = config.WithCompilationCache(cache)
	}
	r := wazero.NewRuntimeWithConfig(ctx, config)
	compiled, err := r.CompileModule(ctx, wasmBytes)
	if err != nil {
		_ = r.Close(ctx)
		if cache != nil {
			_ = cache.Close(ctx)
		}
		return nil, fmt.Errorf("failed to compile wasm module: %w", err)
	}
	return &Engine{ctx: ctx, runtime: r, compiled: compiled, cache: cache}, nil
}

func (e *Engine) Close() error {
	err := e.runtime.Close(e.ctx)
	if e.cache != nil {
		cacheErr := e.cache.Close(e.ctx)
		if err == nil {
			err = cacheErr
		}
	}
	return err
}

func GetEngine(wasmPath string) (*Engine, error) {
	globalEngineOnce.Do(func() {
		if wasmPath == "" {
			wasmPath = defaultWasmPath
		}
		// Opt-in: unset preserves the original in-memory-only runtime. The
		// cache stores compiled WASM machine code, not checker verdicts.
		globalEngine, globalEngineErr = newEngine(wasmPath, os.Getenv("HYBRID_W012_CACHE_DIR"))
	})
	return globalEngine, globalEngineErr
}

// Check bounds a single analysis even when the go/analysis driver does not
// provide a cancellation context. Infrastructure timeout remains fail-closed.
func (e *Engine) Check(src string) (*CheckOutput, error) {
	ctx, cancel := context.WithTimeout(e.ctx, 30*time.Second)
	defer cancel()
	return e.CheckContext(ctx, src)
}

// CheckContext permits an IDE or other embedding host to cancel one request
// without closing the shared wazero runtime or poisoning later checks.
func (e *Engine) CheckContext(ctx context.Context, src string) (*CheckOutput, error) {
	if ctx == nil {
		return nil, fmt.Errorf("nil analysis context")
	}
	engine := &WasmEngine{ctx: e.ctx, runtime: e.runtime, compiled: e.compiled}
	res := engine.CheckSource(ctx, src)
	if res.Verdict == VerdictAnalysisFailure {
		return nil, res.Failure
	}
	return &CheckOutput{Version: 1, Summary: *res.Summary, Diagnostics: res.Diagnostics}, nil
}

// PassContextLog records empirical pass inspection for research
type PassContextLog struct {
	PkgPath            string
	FilesCount         int
	GoFiles            []string
	OtherFiles         []string
	IgnoredFiles       []string
	HasReadFile        bool
	HasTypesInfo       bool
	FoundHyFiles       []string
	PassReadFileTested bool
	PassReadFileErr    string
	PassReadFileLen    int
}

var (
	lastContextLogMu sync.RWMutex
	lastContextLog   PassContextLog
)

// Clone the slices as well as the struct: copying only its slice headers would
// expose mutable internal arrays to callers even while protecting assignments
// to lastContextLog with a mutex.
func clonePassContextLog(log PassContextLog) PassContextLog {
	log.GoFiles = slices.Clone(log.GoFiles)
	log.OtherFiles = slices.Clone(log.OtherFiles)
	log.IgnoredFiles = slices.Clone(log.IgnoredFiles)
	log.FoundHyFiles = slices.Clone(log.FoundHyFiles)
	return log
}

func setLastContextLog(log PassContextLog) {
	lastContextLogMu.Lock()
	lastContextLog = clonePassContextLog(log)
	lastContextLogMu.Unlock()
}

// LastContextLogSnapshot returns an independent snapshot. Callers may modify
// its slices without racing with later analyzer runs or other snapshots.
func LastContextLogSnapshot() PassContextLog {
	lastContextLogMu.RLock()
	defer lastContextLogMu.RUnlock()
	return clonePassContextLog(lastContextLog)
}

var Analyzer = &analysis.Analyzer{
	Name: "hybrid",
	Doc:  "Spike: inspects Go package and invokes in-process WASM checker on sibling .hy files",
	Run:  run,
}

func run(pass *analysis.Pass) (interface{}, error) {
	// The cache-safe vet wrapper supplies a pre-run digest snapshot. Malformed
	// snapshots fail closed before analyzing any package.
	snapshot, err := hybridSnapshotFromEnv()
	if err != nil {
		return nil, fmt.Errorf("read Hybrid input snapshot: %w", err)
	}
	log := PassContextLog{
		PkgPath:      pass.Pkg.Path(),
		FilesCount:   len(pass.Files),
		OtherFiles:   pass.OtherFiles,
		IgnoredFiles: pass.IgnoredFiles,
		HasReadFile:  pass.ReadFile != nil,
		HasTypesInfo: pass.TypesInfo != nil,
	}

	for _, f := range pass.Files {
		if pass.Fset != nil {
			pos := pass.Fset.File(f.Pos())
			if pos != nil {
				log.GoFiles = append(log.GoFiles, pos.Name())
			}
		}
	}

	// Sibling .hy discovery:
	// A. Check pass.OtherFiles
	// B. Check directory containing Go files
	hyFiles := make(map[string]string) // name -> full path or content

	// Declared files are a trusted input set, not an optional hint: failing to
	// read one must fail this analysis, never silently produce an all-clear.
	for _, of := range pass.OtherFiles {
		if filepath.Ext(of) != ".hy" {
			continue
		}
		log.FoundHyFiles = append(log.FoundHyFiles, "OtherFiles:"+of)
		var data []byte
		var readErr error
		if pass.ReadFile != nil {
			data, readErr = pass.ReadFile(of)
		} else {
			data, readErr = os.ReadFile(of)
		}
		if readErr != nil {
			return nil, fmt.Errorf("read declared .hy file %q: %w", of, readErr)
		}
		name, err := filepath.Abs(of)
		if err != nil {
			return nil, fmt.Errorf("resolve declared .hy path %q: %w", of, err)
		}
		if err := snapshot.VerifyFile(name, data); err != nil {
			return nil, fmt.Errorf("verify declared .hy input: %w", err)
		}
		if previous, exists := hyFiles[name]; exists && previous != string(data) {
			return nil, fmt.Errorf("conflicting content for declared .hy file %q", name)
		}
		hyFiles[name] = string(data)
	}

	// Also inspect disk directory of first Go file to test if sibling .hy can be discovered via dir scan
	if len(pass.Files) > 0 && pass.Fset != nil {
		firstFile := pass.Fset.File(pass.Files[0].Pos())
		if firstFile != nil {
			dir := filepath.Dir(firstFile.Name())
			entries, err := os.ReadDir(dir)
			if err != nil {
				return nil, fmt.Errorf("discover .hy files: %w", err)
			}
			for _, e := range entries {
				if filepath.Ext(e.Name()) == ".hy" {
					fullPath, err := filepath.Abs(filepath.Join(dir, e.Name()))
					if err != nil {
						return nil, fmt.Errorf("resolve sibling .hy path %q: %w", e.Name(), err)
					}
					if _, already := hyFiles[fullPath]; !already {
						log.FoundHyFiles = append(log.FoundHyFiles, "DirScan:"+e.Name())

						// .hy files found only by directory scan are NOT valid inputs
						// to Pass.ReadFile: strict drivers may reject or panic on
						// undeclared paths. For this explicitly on-disk vet adapter,
						// read from disk and verify the pre-run Hybrid snapshot.
						// Only listed OtherFiles are read through Pass.ReadFile.
						data, err := os.ReadFile(fullPath)
						if err != nil {
							return nil, fmt.Errorf("read %s: %w", fullPath, err)
						}
						if err := snapshot.VerifyFile(fullPath, data); err != nil {
							return nil, fmt.Errorf("verify scanned .hy input: %w", err)
						}
						hyFiles[fullPath] = string(data)
					}
				}
			}
		}
	}

	setLastContextLog(log)

	// Go-only packages need neither a WASM artifact nor a live runtime.
	if len(hyFiles) == 0 {
		return nil, nil
	}
	engine, err := GetEngine("")
	if err != nil {
		return nil, fmt.Errorf("failed to init wazero engine: %w", err)
	}

	// Map iteration order is unstable: report diagnostics in canonical filename
	// order so identical source trees yield reproducible go vet output.
	filenames := make([]string, 0, len(hyFiles))
	for filename := range hyFiles {
		filenames = append(filenames, filename)
	}
	sort.Strings(filenames)
	for _, filename := range filenames {
		content := hyFiles[filename]
		out, err := engine.Check(content)
		if err != nil {
			return nil, fmt.Errorf("hybrid check on %s failed: %w", filename, err)
		}

		var fileToken *token.File
		if pass.Fset != nil {
			fileToken = newHybridTokenFile(pass.Fset, filename, content)
		}

		// The structured engine preserves nonfatal warnings. go/analysis has
		// no severity channel: passing warnings to vet would turn an accepted
		// managed program into a failing build. Report only errors here.
		for _, d := range out.Diagnostics {
			if d.Severity != "error" {
				continue
			}
			var exactPos token.Pos = token.NoPos
			if fileToken != nil {
				exactPos, err = hybridTokenPos(fileToken, content, d.Span.Start.Line, d.Span.Start.Col)
				if err != nil {
					return nil, fmt.Errorf("invalid diagnostic position in %s: %w", filename, err)
				}
			}

			pass.Report(analysis.Diagnostic{
				Pos:      exactPos,
				Message:  fmt.Sprintf("[%s] %s", d.Code, d.Message),
				Category: "hybrid",
			})
		}
	}

	return nil, nil
}

// Go's line table omits an empty final line, but the Hybrid lexer can report
// EOF at 1:1 in an empty file or after a trailing newline. Reserve one byte
// only in the token table so these positions retain their actual line and
// column. The checker and hybridTokenPos still use the unmodified content.
func newHybridTokenFile(fset *token.FileSet, filename, content string) *token.File {
	file := fset.AddFile(filename, -1, len(content)+1)
	file.SetLinesForContent(append([]byte(content), 0))
	return file
}

// hybridTokenPos translates Rust lexer Unicode-scalar columns into Go's
// byte-based token.Pos without allowing a diagnostic to escape its own line.
func hybridTokenPos(file *token.File, content string, line, col int) (token.Pos, error) {
	if line < 1 || line > file.LineCount() || col < 1 {
		return token.NoPos, fmt.Errorf("invalid position %d:%d", line, col)
	}
	start := file.Offset(file.LineStart(line))
	end := len(content)
	if line < file.LineCount() {
		end = file.Offset(file.LineStart(line + 1))
	}
	if end > start && content[end-1] == '\n' {
		end--
	}
	if end > start && content[end-1] == '\r' {
		end--
	}
	runeCol := 1
	for offset := start; offset < end; {
		if runeCol == col {
			return file.Pos(offset), nil
		}
		_, size := utf8.DecodeRuneInString(content[offset:end])
		offset += size
		runeCol++
	}
	if runeCol == col {
		return file.Pos(end), nil
	}
	return token.NoPos, fmt.Errorf("column %d beyond line %d (maximum %d)", col, line, runeCol)
}

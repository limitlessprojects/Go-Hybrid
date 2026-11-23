// Package w4699artifactmanifest implements the M1/M2 feasibility spike for
// Wildcard #4699: Deterministic artifact manifests with ordinary Go task
// scheduling.
//
// A task takes a deeply immutable input and returns an immutable result or a
// typed failure, plus an ordered list of child inputs. A child gets its parent's
// path plus its index in that list. The compiler admits workers from a small
// effect-checked subset. A coordinator can run ready tasks in any order, stage
// results by path, and publish one manifest after the group finishes. The
// manifest orders successes and failures by numeric path. An out-of-budget group
// produces no manifest. Artifacts are expressed as bytes; materializing files is
// left at the consumer boundary.
//
// This package is the Go reference model for the proposal. It does NOT implement
// the Hybrid checker, real Go parallelism, or filesystem commits; it provides a
// pure, deterministic coordinator plus a static admission pass that fails closed
// on unmediated effects, and a battery of M1/M2 tests that falsify the
// determinism guarantee if any admitted fixture emits different manifest bytes
// under different legal schedules.
package w4699artifactmanifest

import (
	"crypto/sha256"
	"encoding/binary"
	"errors"
	"fmt"
	"math"
	"sort"
	"strings"
)

// ---------------------------------------------------------------------------
// Typed admission diagnostics (Phase 1)
// ---------------------------------------------------------------------------

var (
	// ErrAdmissionAmbientIO is emitted when a worker body performs an ambient
	// read or write (wall clock, RNG, environment, network, filesystem).
	ErrAdmissionAmbientIO = errors.New("HYB-E4701: ambient read/write inside artifact worker")
	// ErrAdmissionOpaqueCall is emitted when a worker body calls an opaque
	// (non-transitively-checked) function.
	ErrAdmissionOpaqueCall = errors.New("HYB-E4702: opaque call inside artifact worker")
	// ErrAdmissionChannelOp is emitted when a worker body performs an ordinary
	// channel operation.
	ErrAdmissionChannelOp = errors.New("HYB-E4703: ordinary channel operation inside artifact worker")
	// ErrAdmissionManagedEscape is emitted when a worker body performs an
	// explicit managed escape.
	ErrAdmissionManagedEscape = errors.New("HYB-E4704: explicit managed escape inside artifact worker")
	// ErrAdmissionUnorderedChildList is emitted when a worker returns a child
	// list that is not finite and ordered.
	ErrAdmissionUnorderedChildList = errors.New("HYB-E4705: child list is not finite and ordered")
)

// AdmissionDiagnostic is a single static rejection emitted by the Phase-1
// admission pass before lowering.
type AdmissionDiagnostic struct {
	Code    string
	Message string
}

// ambientTokens are substrings that indicate an ambient read or write. A worker
// in the admitted subset may not contain any of these.
var ambientTokens = []string{
	"time.now(", "time.since(", "time.unix(",
	"rand.", "math/rand",
	"os.getenv(", "os.readfile(", "os.writefile(", "os.open(",
	"net.", "http.get(", "http.post(",
	"readline(", "readfile(", "writefile(",
}

// channelTokens indicate ordinary (unmediated) channel operations.
var channelTokens = []string{
	"chan ", "make(chan", "<-", "chan_send(", "chan_recv(",
}

// CheckWorkerAdmission statically inspects a `.hy` worker body snippet intended
// for an `artifact_group` and fails closed on any source of nondeterminism or
// effect outside the admitted subset. It returns a slice of diagnostics; an
// empty slice means the worker is admitted.
func CheckWorkerAdmission(hySource string) []AdmissionDiagnostic {
	var diags []AdmissionDiagnostic
	lines := strings.Split(hySource, "\n")
	for idx, rawLine := range lines {
		line := strings.TrimSpace(rawLine)
		if line == "" || strings.HasPrefix(line, "//") {
			continue
		}
		lineNo := idx + 1
		if strings.Contains(line, "managed(") {
			diags = append(diags, AdmissionDiagnostic{
				Code:    "HYB-E4704",
				Message: fmt.Sprintf("line %d: explicit managed escape forbidden inside artifact worker", lineNo),
			})
		}
		for _, tok := range ambientTokens {
			if strings.Contains(line, tok) {
				diags = append(diags, AdmissionDiagnostic{
					Code:    "HYB-E4701",
					Message: fmt.Sprintf("line %d: ambient read/write %q inside artifact worker", lineNo, tok),
				})
			}
		}
		for _, tok := range channelTokens {
			if strings.Contains(line, tok) {
				diags = append(diags, AdmissionDiagnostic{
					Code:    "HYB-E4703",
					Message: fmt.Sprintf("line %d: ordinary channel operation %q inside artifact worker", lineNo, tok),
				})
			}
		}
		if strings.Contains(line, "opaque_call(") || strings.Contains(line, "ffi_call(") {
			diags = append(diags, AdmissionDiagnostic{
				Code:    "HYB-E4702",
				Message: fmt.Sprintf("line %d: opaque call forbidden inside artifact worker", lineNo),
			})
		}
	}
	return diags
}

// ---------------------------------------------------------------------------
// Core model: paths, values, tasks, results
// ---------------------------------------------------------------------------

// Path is an immutable, ordered sequence of child indices from the root. The
// root task has the empty path. A child gets its parent's path plus its index in
// the parent's ordered child list. Numeric path ordering is the manifest's
// canonical order.
type Path []int

// String renders a path in dotted notation ("1.2.0"). The root is "".
func (p Path) String() string {
	parts := make([]string, len(p))
	for i, n := range p {
		parts[i] = fmt.Sprintf("%d", n)
	}
	return strings.Join(parts, ".")
}

// Compare orders two paths numerically, element by element, with the shorter
// path first when one is a prefix of the other. This is the canonical manifest
// order.
func (p Path) Compare(q Path) int {
	n := len(p)
	if len(q) < n {
		n = len(q)
	}
	for i := 0; i < n; i++ {
		if p[i] < q[i] {
			return -1
		}
		if p[i] > q[i] {
			return 1
		}
	}
	if len(p) < len(q) {
		return -1
	}
	if len(p) > len(q) {
		return 1
	}
	return 0
}

// Append returns a new Path with idx appended, leaving the receiver immutable.
func (p Path) Append(idx int) Path {
	out := make(Path, len(p)+1)
	copy(out, p)
	out[len(p)] = idx
	return out
}

// Value is a deeply immutable artifact payload expressed as bytes. The model
// never inspects the bytes; it only hashes and orders them.
type Value []byte

// Failure is a typed task failure. Code is a stable identifier; Message is a
// human-readable detail. Both are immutable once constructed.
type Failure struct {
	Code    string
	Message string
}

// Result is the immutable outcome of a task: either a Value or a Failure.
type Result struct {
	Value   Value
	Failure *Failure
}

// Ok constructs a successful result.
func Ok(v Value) Result { return Result{Value: v} }

// Fail constructs a failed result.
func Fail(code, msg string) Result {
	return Result{Failure: &Failure{Code: code, Message: msg}}
}

// IsFailure reports whether the result is a failure.
func (r Result) IsFailure() bool { return r.Failure != nil }

// TaskInput is a deeply immutable input to a task at a given path.
type TaskInput struct {
	Path  Path
	Value Value
}

// Worker is an admitted, effect-checked task function. It receives an immutable
// input and returns an immutable result plus an ordered, finite list of child
// inputs. Workers must be pure: they may not read ambient state, perform I/O, or
// touch channels. The admission pass (CheckWorkerAdmission) enforces this
// statically at the source level.
type Worker func(in TaskInput) (Result, []TaskInput)

// Budget caps the total number of tasks and total staged bytes in a group. A
// group that exceeds either cap is out-of-budget and produces no manifest.
type Budget struct {
	MaxTasks int
	MaxBytes int
}

// ---------------------------------------------------------------------------
// Manifest
// ---------------------------------------------------------------------------

// Entry is one ordered record in the manifest: the path, whether it succeeded,
// the artifact bytes (on success), or the typed failure (on failure).
type Entry struct {
	Path    Path
	OK      bool
	Value   Value
	Failure *Failure
}

// Manifest is the single deterministic artifact manifest published after a group
// finishes. Entries are ordered by numeric path.
type Manifest struct {
	Entries []Entry
}

// Encode serializes the manifest into canonical, deterministic bytes. Success
// and failure records are both included and ordered by numeric path. The output
// is stable across all legal schedules for a fixed input.
func (m Manifest) Encode() []byte {
	var buf []byte
	var tmp [8]byte
	for _, e := range m.Entries {
		// Path length + path elements.
		binary.LittleEndian.PutUint32(tmp[:4], uint32(len(e.Path)))
		buf = append(buf, tmp[:4]...)
		for _, n := range e.Path {
			binary.LittleEndian.PutUint32(tmp[:4], uint32(n))
			buf = append(buf, tmp[:4]...)
		}
		if e.OK {
			buf = append(buf, 'O', 'K')
			binary.LittleEndian.PutUint64(tmp[:], uint64(len(e.Value)))
			buf = append(buf, tmp[:]...)
			buf = append(buf, e.Value...)
		} else {
			buf = append(buf, 'E', 'R')
			binary.LittleEndian.PutUint32(tmp[:4], uint32(len(e.Failure.Code)))
			buf = append(buf, tmp[:4]...)
			buf = append(buf, []byte(e.Failure.Code)...)
			binary.LittleEndian.PutUint32(tmp[:4], uint32(len(e.Failure.Message)))
			buf = append(buf, tmp[:4]...)
			buf = append(buf, []byte(e.Failure.Message)...)
		}
	}
	return buf
}

// Hash returns the SHA-256 of the canonical manifest encoding.
func (m Manifest) Hash() [32]byte {
	return sha256.Sum256(m.Encode())
}

// ---------------------------------------------------------------------------
// Coordinator
// ---------------------------------------------------------------------------

// SchedulePolicy selects which ready task to run next. The coordinator admits
// any policy; the manifest must be identical across all of them.
type SchedulePolicy int

const (
	// FIFO runs ready tasks in path order (breadth-ish, lowest path first).
	FIFO SchedulePolicy = iota
	// LIFO runs ready tasks in reverse path order (highest path first).
	LIFO
	// DepthFirst runs the deepest ready task first.
	DepthFirst
	// Random uses a seeded PRNG to pick among ready tasks.
	Random
)

// staged is the internal per-path record of a completed task.
type staged struct {
	result Result
}

// Coordinator runs a task group to completion under a chosen schedule policy
// and publishes a single deterministic manifest. It stages results by path and
// never exposes arrival order.
type Coordinator struct {
	Root      Worker
	Budget    Budget
	rootValue Value
	seed      uint64
	rngUsed   bool
}

// NewCoordinator constructs a coordinator for the given root worker and budget.
func NewCoordinator(root Worker, budget Budget) *Coordinator {
	return &Coordinator{Root: root, Budget: budget}
}

// SetSeed sets the PRNG seed used by the Random policy.
func (c *Coordinator) SetSeed(seed uint64) *Coordinator {
	c.seed = seed
	c.rngUsed = true
	return c
}

// nextRand is a SplitMix64 deterministic PRNG.
func (c *Coordinator) nextRand(n int) int {
	c.seed += 0x9e3779b97f4a7c15
	z := c.seed
	z = (z ^ (z >> 30)) * 0xbf58476d1ce4e5b9
	z = (z ^ (z >> 27)) * 0x94d049bb133111eb
	z = z ^ (z >> 31)
	return int(z % uint64(n))
}

// node is a pending or completed task in the group.
type node struct {
	path   Path
	input  TaskInput
	staged *staged // non-nil once completed
}

// Run executes the group to completion under policy and returns the manifest.
// If the group exceeds its task or byte budget, Run returns ErrBudgetExhausted
// and a nil manifest (an out-of-budget group produces no manifest).
func (c *Coordinator) Run(policy SchedulePolicy) (*Manifest, error) {
	// pending holds nodes whose children have not all been scheduled yet; we
	// schedule a node, then enqueue its children. A node is "ready" when it is
	// enqueued but not yet run.
	type workItem struct {
		input TaskInput
	}
	pending := []workItem{{input: TaskInput{Path: Path{}, Value: c.RootInput()}}}
	results := make(map[string]*staged)
	taskCount := 0
	byteCount := 0

	for len(pending) > 0 {
		idx := c.pick(len(pending), policy)
		item := pending[idx]
		pending = append(pending[:idx], pending[idx+1:]...)

		taskCount++
		if c.Budget.MaxTasks > 0 && taskCount > c.Budget.MaxTasks {
			return nil, ErrBudgetExhausted
		}

		res, children := c.Root(item.input)
		byteCount += len(res.Value)
		if c.Budget.MaxBytes > 0 && byteCount > c.Budget.MaxBytes {
			return nil, ErrBudgetExhausted
		}

		key := item.input.Path.String()
		results[key] = &staged{result: res}

		// Enqueue children with parent path + index. Children are finite and
		// ordered by construction of the worker's returned slice.
		for i, child := range children {
			child.Path = item.input.Path.Append(i)
			pending = append(pending, workItem{input: child})
		}
	}

	// Build the manifest ordered by numeric path.
	ordered := make([]Path, 0, len(results))
	for k := range results {
		ordered = append(ordered, parsePathKey(k))
	}
	sort.Slice(ordered, func(i, j int) bool { return ordered[i].Compare(ordered[j]) < 0 })

	entries := make([]Entry, 0, len(ordered))
	for _, p := range ordered {
		st := results[p.String()]
		if st.result.IsFailure() {
			entries = append(entries, Entry{Path: p, OK: false, Failure: st.result.Failure})
		} else {
			entries = append(entries, Entry{Path: p, OK: true, Value: st.result.Value})
		}
	}
	return &Manifest{Entries: entries}, nil
}

// RootInput is the root task's input value. The default root value is empty; a
// caller sets it via SetRootValue before Run.
func (c *Coordinator) RootInput() Value { return c.rootValue }

// SetRootValue sets the root task's input value.
func (c *Coordinator) SetRootValue(v Value) *Coordinator {
	c.rootValue = v
	return c
}

// pick selects an index in [0,n) according to policy. For deterministic
// policies the pending slice is kept in path order so FIFO/LIFO/DepthFirst are
// well defined; Random uses the PRNG.
func (c *Coordinator) pick(n int, policy SchedulePolicy) int {
	switch policy {
	case FIFO:
		return 0
	case LIFO:
		return n - 1
	case DepthFirst:
		// Deepest path last-enqueued tends to be at the end; pick last.
		return n - 1
	case Random:
		if !c.rngUsed {
			c.seed = 0x9e3779b97f4a7c15
			c.rngUsed = true
		}
		return c.nextRand(n)
	default:
		return 0
	}
}

// parsePathKey reconstructs a Path from its dotted string key.
func parsePathKey(key string) Path {
	if key == "" {
		return Path{}
	}
	parts := strings.Split(key, ".")
	p := make(Path, len(parts))
	for i, s := range parts {
		n := 0
		for _, r := range s {
			n = n*10 + int(r-'0')
		}
		p[i] = n
	}
	return p
}

// ErrBudgetExhausted is returned when a group exceeds its task or byte budget.
var ErrBudgetExhausted = errors.New("HYB-E4706: artifact group exceeded budget; no manifest published")

// ---------------------------------------------------------------------------
// Non-finite value rejection (M1)
// ---------------------------------------------------------------------------

// ErrNonFiniteValue is returned when a worker attempts to stage a NaN or Inf
// float payload. The admitted value subset is finite bytes only.
var ErrNonFiniteValue = errors.New("HYB-E4707: non-finite value rejected")

// CheckFinite rejects NaN/Inf float64 payloads before staging.
func CheckFinite(f float64) error {
	if math.IsNaN(f) || math.IsInf(f, 0) {
		return ErrNonFiniteValue
	}
	return nil
}
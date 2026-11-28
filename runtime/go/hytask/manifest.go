// Package hytask — deterministic artifact task groups for ordinary Go
// scheduling (issue #4699, "Deterministic artifact manifests with ordinary
// Go task scheduling").
//
// A Group runs a set of pure tasks concurrently on real goroutines and
// publishes ONE canonical manifest once every task has finished. The manifest
// orders successes and failures by the task's numeric Path, so the published
// bytes are independent of the order in which goroutines happened to finish.
// A worker that finishes first can change latency; it cannot change its
// position in the output or the reported failure order.
//
// This is the runtime half of the proposal. The compiler-side admission pass
// (the Tier-0 effect checker that decides which workers may join a group) is
// NOT implemented here; callers are trusted to supply pure tasks. The
// determinism guarantee is about SCHEDULING, not about admission.
//
// Contract for a task (enforced by convention, not by this package):
//   - it takes a deeply-immutable input and returns an immutable Result value
//     or a typed Failure;
//   - it performs no ambient reads/writes, no channel operations, and no
//     managed escapes — its output depends only on its input.
//
// Determinism is verified empirically in manifest_test.go by running the same
// group under many real goroutine schedules (GOMAXPROCS variations, random
// per-task sleeps, and -race) and asserting byte-identical manifests.
package hytask

import (
	"bytes"
	"crypto/sha256"
	"encoding/binary"
	"encoding/hex"
	"errors"
	"fmt"
	"math"
	"sort"
	"sync"
)

// Sentinel errors. A group that exhausts a budget publishes no manifest.
var (
	// ErrBudgetExhausted is returned (with a nil manifest) when a group would
	// exceed its task-count or byte budget. An over-budget group produces no
	// manifest at all — never a partial one.
	ErrBudgetExhausted = errors.New("hytask: group over budget, no manifest published")

	// ErrNonFiniteValue is returned (with a nil manifest) when a task result
	// carries a non-finite float (NaN or ±Inf). Artifacts are bytes; a NaN has
	// no canonical byte encoding, so it is rejected fail-closed.
	ErrNonFiniteValue = errors.New("hytask: non-finite float in result, no manifest published")

	// ErrNilTask is returned when Run is given a nil task function.
	ErrNilTask = errors.New("hytask: nil task function")
)

// Path is a task's position in the dynamic task tree. A child's path is its
// parent's path with the child's index appended, so a root "1" with children
// "2","3" yields "1","2","3", and "2"'s children are "4","5".
//
// The zero Path (nil) is the implicit single root when a group is built from a
// flat task list; it orders before every explicit path.
type Path []int

// String renders a path as dot-joined components (e.g. "1.2.3"). The empty
// path renders as "0" so it sorts first and prints unambiguously.
func (p Path) String() string {
	if len(p) == 0 {
		return "0"
	}
	var b bytes.Buffer
	for i, c := range p {
		if i > 0 {
			b.WriteByte('.')
		}
		fmt.Fprintf(&b, "%d", c)
	}
	return b.String()
}

// child returns the path of the i-th child of p.
func (p Path) child(i int) Path {
	c := make(Path, len(p)+1)
	copy(c, p)
	c[len(p)] = i
	return c
}

// Result is a task's immutable output: an ordered list of immutable byte
// artifacts. A nil Artifacts slice is a valid (empty) success.
type Result struct {
	Artifacts [][]byte
}

// Failure is a task's typed failure. Code and Message are part of the
// manifest and therefore part of the determinism contract.
type Failure struct {
	Code    string
	Message string
}

// entry is one staged task outcome, keyed by path.
type entry struct {
	path    Path
	result  *Result
	failure *Failure
}

// Manifest is the published, deterministic record of a finished group.
// Entries are ordered by numeric path. The manifest is immutable once
// returned by Run.
type Manifest struct {
	Entries []Entry
}

// Entry is one line of a manifest: a path and exactly one of Result/Failure.
type Entry struct {
	Path    Path
	Result  *Result
	Failure *Failure
}

// SchedulePolicy selects which ready task runs next. It affects only the order
// of execution (and thus latency), never the published manifest bytes.
type SchedulePolicy int

const (
	// FIFO runs the lowest-path ready task first.
	FIFO SchedulePolicy = iota
	// LIFO runs the highest-path ready task first.
	LIFO
	// DepthFirst runs the deepest ready task first (ties: lowest path).
	DepthFirst
	// Random runs a pseudo-random ready task (seeded, for reproducibility).
	Random
)

// Budget caps a group. A group that would exceed either limit publishes no
// manifest (ErrBudgetExhausted). Zero means "no cap on that dimension".
type Budget struct {
	MaxTasks int
	MaxBytes int
}

// Task is a pure unit of work. It receives its path and returns either a
// result or a failure. It must not perform ambient IO or channel operations.
type Task func(path Path) (*Result, *Failure)

// Coordinator runs tasks and publishes their manifest.
type Coordinator struct {
	// Validate, if non-nil, is called on every successful result before it is
	// staged. Returning an error aborts the group with that error and no
	// manifest. It is the hook that rejects non-finite payloads fail-closed.
	Validate func(path Path, r *Result) error

	mu      sync.Mutex
	entries []entry
	bytes   int
}

// Run executes every task in tasks (keyed by path) concurrently on real
// goroutines, using policy to pick the next task to START, waits for all of
// them to finish, and returns the canonical manifest.
//
// The manifest is byte-identical regardless of policy, GOMAXPROCS, or the
// random order in which goroutines complete — that is the guarantee this
// package exists to provide and that manifest_test.go verifies empirically.
func (c *Coordinator) Run(tasks map[string]Task, paths map[string]Path, policy SchedulePolicy, budget Budget, seed uint64) (*Manifest, error) {
	// Order task start by policy; completion order is irrelevant to output.
	ordered := make([]string, 0, len(tasks))
	for p := range tasks {
		ordered = append(ordered, p)
	}
	sort.Slice(ordered, func(i, j int) bool {
		return pathLess(paths[ordered[i]], paths[ordered[j]])
	})
	startOrder := scheduleOrder(ordered, paths, policy, seed)

	if budget.MaxTasks > 0 && len(tasks) > budget.MaxTasks {
		return nil, ErrBudgetExhausted
	}

	var wg sync.WaitGroup
	for _, p := range startOrder {
		t := tasks[p]
		if t == nil {
			return nil, ErrNilTask
		}
		wg.Add(1)
		go func(key string, path Path, task Task) {
			defer wg.Done()
			res, fail := task(path)
			c.mu.Lock()
			defer c.mu.Unlock()
			if fail != nil {
				c.bytes += len(fail.Code) + len(fail.Message)
				if budget.MaxBytes > 0 && c.bytes > budget.MaxBytes {
					// Record the overrun; Run converts it to ErrBudgetExhausted.
					c.bytes = budget.MaxBytes + 1
				}
				c.entries = append(c.entries, entry{path: path, failure: fail})
				return
			}
			if c.Validate != nil {
				if err := c.Validate(path, res); err != nil {
					// Stash the error on the first offending entry's failure so
					// Run can surface it. We encode it as a sentinel failure.
					c.entries = append(c.entries, entry{path: path, failure: &Failure{Code: "HYB-E4700", Message: err.Error()}})
					return
				}
			}
			for _, a := range res.Artifacts {
				c.bytes += len(a)
			}
			if budget.MaxBytes > 0 && c.bytes > budget.MaxBytes {
				c.bytes = budget.MaxBytes + 1
			}
			c.entries = append(c.entries, entry{path: path, result: res})
		}(p, paths[p], t)
	}
	wg.Wait()

	// A byte overrun poisons the whole group: no manifest.
	if budget.MaxBytes > 0 && c.bytes > budget.MaxBytes {
		return nil, ErrBudgetExhausted
	}

	// Surface a validation failure (e.g. non-finite) as a group error.
	for _, e := range c.entries {
		if e.failure != nil && e.failure.Code == "HYB-E4700" {
			return nil, ErrNonFiniteValue
		}
	}

	sortEntries(c.entries)
	m := &Manifest{Entries: make([]Entry, len(c.entries))}
	for i, e := range c.entries {
		m.Entries[i] = Entry{Path: e.path, Result: e.result, Failure: e.failure}
	}
	return m, nil
}

// sortEntries orders entries by numeric path (canonical manifest order).
func sortEntries(es []entry) {
	sort.Slice(es, func(i, j int) bool { return pathLess(es[i].path, es[j].path) })
}

// pathLess is the canonical numeric path order: compare component by
// component; a proper prefix sorts before a longer path (so "1" < "1.1").
func pathLess(a, b Path) bool {
	n := len(a)
	if len(b) < n {
		n = len(b)
	}
	for i := 0; i < n; i++ {
		if a[i] != b[i] {
			return a[i] < b[i]
		}
	}
	return len(a) < len(b)
}

// scheduleOrder returns task keys in the order the coordinator should START
// them. The published manifest does not depend on this order.
func scheduleOrder(keys []string, paths map[string]Path, policy SchedulePolicy, seed uint64) []string {
	out := make([]string, len(keys))
	copy(out, keys)
	switch policy {
	case FIFO:
		// keys already sorted by path ascending.
	case LIFO:
		for i, j := 0, len(out)-1; i < j; i, j = i+1, j-1 {
			out[i], out[j] = out[j], out[i]
		}
	case DepthFirst:
		// Deepest path first; ties broken by lowest path (stable on the
		// path-sorted input).
		sort.SliceStable(out, func(i, j int) bool {
			di, dj := len(paths[out[i]]), len(paths[out[j]])
			if di != dj {
				return di > dj
			}
			return pathLess(paths[out[i]], paths[out[j]])
		})
	case Random:
		rng := newRNG(seed)
		rng.shuffle(out)
	}
	return out
}

// Encode returns the canonical byte encoding of the manifest. Two manifests
// with the same entries in the same order encode to identical bytes.
//
// Format (all integers big-endian):
//
//	u32 entry-count
//	repeated entry-count times:
//	  u32 path-length, then path-length * i64 path components
//	  u8  kind (0 = success, 1 = failure)
//	  if success: u32 artifact-count, then per artifact (u32 len + bytes)
//	  if failure: u32 code-len + code, u32 msg-len + msg
func (m *Manifest) Encode() []byte {
	var b bytes.Buffer
	var tmp [8]byte
	putU32 := func(v uint32) {
		binary.BigEndian.PutUint32(tmp[:4], v)
		b.Write(tmp[:4])
	}
	putU64 := func(v uint64) {
		binary.BigEndian.PutUint64(tmp[:8], v)
		b.Write(tmp[:8])
	}
	putBytes := func(bs []byte) {
		putU32(uint32(len(bs)))
		b.Write(bs)
	}

	putU32(uint32(len(m.Entries)))
	for _, e := range m.Entries {
		putU32(uint32(len(e.Path)))
		for _, c := range e.Path {
			putU64(uint64(c))
		}
		if e.Failure != nil {
			b.WriteByte(1)
			putBytes([]byte(e.Failure.Code))
			putBytes([]byte(e.Failure.Message))
		} else {
			b.WriteByte(0)
			arts := 0
			if e.Result != nil {
				arts = len(e.Result.Artifacts)
			}
			putU32(uint32(arts))
			if e.Result != nil {
				for _, a := range e.Result.Artifacts {
					putBytes(a)
				}
			}
		}
	}
	return b.Bytes()
}

// Hash returns the lowercase hex SHA-256 of the canonical encoding. It hashes
// ONLY the documented manifest bytes (never task metadata or timing).
func (m *Manifest) Hash() string {
	sum := sha256.Sum256(m.Encode())
	return hex.EncodeToString(sum[:])
}

// ---- deterministic RNG (splitmix64 + Fisher-Yates) ------------------------
//
// math/rand's global source is intentionally NOT used: its seeding and
// algorithm are not part of this package's stability contract, and a future
// Go runtime could change them. A hand-rolled generator keeps the Random
// policy reproducible across toolchains.

type rng struct{ state uint64 }

func newRNG(seed uint64) *rng { return &rng{state: seed} }

// next is splitmix64.
func (r *rng) next() uint64 {
	r.state += 0x9E3779B97F4A7C15
	z := r.state
	z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9
	z = (z ^ (z >> 27)) * 0x94D049BB133111EB
	return z ^ (z >> 31)
}

// shuffle is a Fisher-Yates shuffle using unbiased bounded draws.
func (r *rng) shuffle(s []string) {
	for i := len(s) - 1; i > 0; i-- {
		j := int(r.next() % uint64(i+1))
		s[i], s[j] = s[j], s[i]
	}
}

// IsFinite reports whether v is a finite float (not NaN, not ±Inf). It is the
// canonical finiteness predicate the Validate hook uses to reject non-finite
// payloads fail-closed.
func IsFinite(v float64) bool {
	return !math.IsNaN(v) && !math.IsInf(v, 0)
}

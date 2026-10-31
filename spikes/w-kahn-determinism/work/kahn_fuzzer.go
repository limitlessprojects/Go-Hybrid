// Package kahnspike implements the adversarial schedule fuzzer and emitted-Go
// harness for Wildcard Proposal #2475:
// "Kahn determinism: prove that a checked task network computes the same output on every schedule".
//
// Every channel send, receive, and close in the emitted Go programs is instrumented
// with randomized `runtime.Gosched()` / micro-yield perturbations under varied
// `GOMAXPROCS` (1..8) and `GODEBUG=asyncpreemptoff=0` across N >= 1,000 runs.
package kahnspike

import (
	"fmt"
	"math/rand/v2"
	"runtime"
	"sort"
	"strings"
	"sync"
	"sync/atomic"
)

// Perturb injects adversarial schedule yields before/after every channel op.
func Perturb(seed uint64, step int) {
	if (seed^uint64(step*0x9e3779b9))%3 != 0 {
		runtime.Gosched()
	}
	if (seed^uint64(step*0x85ebca6b))%5 == 0 {
		for i := 0; i < int((seed>>uint(step%16))&3); i++ {
			runtime.Gosched()
		}
	}
}

// ============================================================================
// PART A: KAHN-ADMITTED SPSC / TASK PROGRAMS (H1 Soundness Target: 0 divergence)
// ============================================================================

// 1. examples/04-pipeline-backpressure (with select timeout removed -> pure SPSC Kahn network)
// Producer -> raw (cap 2) -> Squarer -> sq (cap 2) -> Main consumer.
func RunExample04PipelinePureKahn(seed uint64) string {
	raw := make(chan int, 2)
	sq := make(chan int, 2)

	// Producer (single producer on raw)
	go func() {
		for i := 1; i <= 8; i++ {
			Perturb(seed, i)
			raw <- i
			Perturb(seed, i+100)
		}
		close(raw)
	}()

	// Squarer (single consumer on raw, single producer on sq)
	go func() {
		step := 200
		for v := range raw {
			step++
			Perturb(seed, step)
			sq <- v * v
			Perturb(seed, step+50)
		}
		close(sq)
	}()

	total := 0
	step := 400
	for v := range sq {
		step++
		Perturb(seed, step)
		total += v
	}
	return fmt.Sprintf("total=%d", total)
}

// 2. Corpus 16-move-send-clean.hy (SPSC batch ingestion over `batches` & `acks`)
type row16 struct {
	key string
	val int
}

type batch16 struct {
	seq  int
	rows []row16
}

func RunCorpus16MoveSendClean(seed uint64) string {
	const nBatches = 8
	batches := make(chan batch16, 8)
	acks := make(chan int, 8)

	go func() {
		step := 10
		for b := range batches {
			step++
			Perturb(seed, step)
			_ = len(b.rows)
			acks <- b.seq
			Perturb(seed, step+100)
		}
	}()

	for seq := 0; seq < nBatches; seq++ {
		var rows []row16
		for i := 0; i < 4; i++ {
			rows = append(rows, row16{key: fmt.Sprintf("k%d-%d", seq, i), val: seq*10 + i})
		}
		Perturb(seed, seq+500)
		batches <- batch16{seq: seq, rows: rows}
	}
	close(batches)
	var ackOrder []int
	for i := 0; i < nBatches; i++ {
		Perturb(seed, i+700)
		ackOrder = append(ackOrder, <-acks)
	}
	return fmt.Sprintf("ingested %d batches acks=%v", nBatches, ackOrder)
}

// 3. Corpus 23-a1c-mapslot-copy-at-spawn.hy (SPSC `done` channel)
func RunCorpus23MapslotCopyAtSpawn(seed uint64) string {
	done := make(chan string, 1)
	current := 2
	go func(id string, n int, replicas int) {
		Perturb(seed, 1)
		done <- fmt.Sprintf("%s: %d", id, replicas+n)
	}("frontend", 3, current)
	Perturb(seed, 2)
	msg := <-done
	return msg
}

// 4. Corpus 41-wrapper-alias-own-move-control.hy (SPSC `results` and `done`)
func RunCorpus41WrapperAliasMoveControl(seed uint64) string {
	agg := map[string]int{"a": 1}
	results := make(chan map[string]int, 1)
	done := make(chan int, 1)

	go func() {
		Perturb(seed, 11)
		m := <-results
		Perturb(seed, 12)
		done <- len(m)
	}()

	before := len(agg)
	Perturb(seed, 13)
	results <- agg
	cnt := <-done
	return fmt.Sprintf("before: %d count: %d", before, cnt)
}

// 5. Corpus 44-copy-primitive-element-control.hy (SPSC `done`, disjoint slice backing)
func RunCorpus44CopyPrimitiveControl(seed uint64) string {
	src := []int{1}
	dst := make([]int, 1)
	copy(dst, src)
	done := make(chan int, 1)

	go func(d []int) {
		Perturb(seed, 21)
		d[0] = 9
		done <- d[0]
	}(dst)

	Perturb(seed, 22)
	src[0] = 5
	wVal := <-done
	return fmt.Sprintf("src=%d dst=%d", src[0], wVal)
}

// 6. Corpus 62-shadowing-binder-last-use.hy & 63-shadowing-binders-at-spawn.hy
func RunCorpus62ShadowingBinderLastUse(seed uint64) string {
	m := map[string]int{"a": 1}
	done := make(chan bool, 1)
	go func(moved map[string]int) {
		for i := 0; i < 16; i++ {
			if i%4 == 0 {
				Perturb(seed, i)
			}
			moved["w"] = i
		}
		done <- true
	}(m)

	total := 0
	for n := 0; n < 100; n++ {
		total += n
	}
	Perturb(seed, 99)
	<-done
	return fmt.Sprintf("total=%d", total)
}

// 7. Multi-stage SPSC Diamond Merge via dedicated per-edge channels (Pure Kahn Network)
// Source -> (Left branch chL, Right branch chR) -> Deterministic Joiner reading chL then chR.
func RunMultiStageKahnDiamond(seed uint64) string {
	chLIn := make(chan int, 2)
	chRIn := make(chan int, 2)
	chLOut := make(chan int, 2)
	chROut := make(chan int, 2)

	// Splitter (single producer on chLIn and chRIn)
	go func() {
		for i := 1; i <= 6; i++ {
			Perturb(seed, i)
			chLIn <- i * 2
			Perturb(seed, i+10)
			chRIn <- i * 3
		}
		close(chLIn)
		close(chRIn)
	}()

	// Left worker (SPSC on chLIn -> chLOut)
	go func() {
		for v := range chLIn {
			Perturb(seed, v+20)
			chLOut <- v + 1
		}
		close(chLOut)
	}()

	// Right worker (SPSC on chRIn -> chROut)
	go func() {
		for v := range chRIn {
			Perturb(seed, v+30)
			chROut <- v - 1
		}
		close(chROut)
	}()

	// Deterministic Kahn Joiner (blocks sequentially on chLOut then chROut per token)
	var pairs []string
	for {
		l, okL := <-chLOut
		r, okR := <-chROut
		if !okL && !okR {
			break
		}
		Perturb(seed, l+r)
		pairs = append(pairs, fmt.Sprintf("(%d,%d)", l, r))
	}
	return strings.Join(pairs, ",")
}

// ============================================================================
// PART B: KAHN-REJECTED PROGRAMS (H2 Precision: True Vary vs Commutative Sink)
// ============================================================================

// 1. Corpus select-two-recv-define-arms-accept.hy (`HYB-E3503: KAHN-SELECT`)
// Two buffered channels `a <- 1` and `b <- 2` ready simultaneously in `select`:
// Go runtime pseudo-randomly chooses arm `a` (total=1) or arm `b` (total=2).
func RunRejectedSelectTwoRecvArms(seed uint64) string {
	a := make(chan int, 1)
	b := make(chan int, 1)
	a <- 1
	b <- 2
	Perturb(seed, 1)
	total := 0
	select {
	case v := <-a:
		total += v
	case w, ok := <-b:
		if ok {
			total += w
		}
	}
	return fmt.Sprintf("total=%d", total)
}

// 2. Corpus 35-default-fallback.hy (`HYB-E3503: KAHN-SELECT` with `default` arm)
// When a concurrent receiver races with a non-blocking `select { case ch <- "ready": ... default: ... }`,
// output varies between "sent" and "idle" depending on goroutine scheduling.
func RunRejectedSelectDefaultRace(seed uint64) string {
	ch := make(chan string)
	done := make(chan struct{})
	go func() {
		if seed%2 == 0 {
			runtime.Gosched()
		}
		select {
		case <-ch:
		case <-done:
		}
	}()
	if seed%2 == 1 {
		runtime.Gosched()
	}
	var out string
	select {
	case ch <- "ready":
		out = "sent"
	default:
		out = "idle"
	}
	close(done)
	return out
}

// 3. Raw Multi-Producer Fan-In WITHOUT Commutative Sink (`HYB-E3501: KAHN-MULTI-PRODUCER`)
// Unsorted arrival order of worker IDs varies across schedules.
func RunRejectedMultiProducerArrivalOrder(seed uint64) string {
	const nWorkers = 4
	results := make(chan string, nWorkers)
	for id := 0; id < nWorkers; id++ {
		go func(w int) {
			if (seed>>uint(w))&1 == 1 {
				runtime.Gosched()
			}
			Perturb(seed, w+1)
			results <- fmt.Sprintf("w%d", w)
		}(id)
	}
	var order []string
	for i := 0; i < nWorkers; i++ {
		order = append(order, <-results)
	}
	return strings.Join(order, ",")
}

// 4. Shared Atomic Read/Write Interleaving (`HYB-E3505: KAHN-ATOMIC`)
// Models reading an `atomic.Int64` counter while concurrent probes increment it.
func RunRejectedAtomicInterleaving(seed uint64) string {
	var hits atomic.Int64
	var wg sync.WaitGroup
	for i := 0; i < 4; i++ {
		wg.Add(1)
		go func(id int) {
			defer wg.Done()
			if (seed>>uint(id))&1 == 1 {
				runtime.Gosched()
			}
			hits.Add(1)
		}(i)
	}
	if seed%3 == 0 {
		runtime.Gosched()
	}
	mid := hits.Load()
	wg.Wait()
	return fmt.Sprintf("mid=%d final=%d", mid, hits.Load())
}

// 5. Unordered Map Iteration Reaching Channel/Output (`HYB-E3506: KAHN-MAP-ITER`)
func RunRejectedMapIterationToChannel(seed uint64) string {
	m := map[string]int{"alpha": 1, "beta": 2, "gamma": 3, "delta": 4}
	ch := make(chan string, len(m))
	go func() {
		// In Go, map iteration order is randomized per execution; we also vary entry insertion
		for k, v := range m {
			Perturb(seed, v)
			ch <- fmt.Sprintf("%s=%d", k, v)
		}
		close(ch)
	}()
	var items []string
	for s := range ch {
		items = append(items, s)
	}
	return strings.Join(items, ";")
}

// 6. Canonical False Negative #1: `examples/03-worker-pool` (`HYB-E3501`/`HYB-E3502` + `sort.ints` sink)
// 4 workers share `jobs` (multi-consumer) and `results` (multi-producer), so channel
// arrival order varies, but `sort.Ints(got)` is a commutative sink that restores determinism!
func RunCanonicalFN_Example03WorkerPoolSorted(seed uint64) (sortedOut string, rawArrivalOrder string) {
	const nWorkers = 4
	inputs := []int{3, 1, 4, 1, 5, 9, 2, 6}
	jobs := make(chan int, 16)
	results := make(chan int, 16)

	for i := 0; i < nWorkers; i++ {
		go func(id int) {
			for j := range jobs {
				if (seed^uint64(id+j))%2 == 0 {
					runtime.Gosched()
				}
				Perturb(seed, id*10+j)
				results <- j * 2
			}
		}(i)
	}

	for _, v := range inputs {
		jobs <- v
	}
	close(jobs)

	var got []int
	for len(got) < len(inputs) {
		got = append(got, <-results)
	}
	rawArrivalOrder = fmt.Sprintf("%v", got)
	sorted := append([]int(nil), got...)
	sort.Ints(sorted)
	sortedOut = fmt.Sprintf("%v", sorted)
	return sortedOut, rawArrivalOrder
}

// 7. Canonical False Negative #2: `corpus/15-fanin-per-task-state.hy` (`HYB-E3501` + commutative map `+=` fold)
// 4 shards send `move local` maps into one `results` channel (multi-producer), so map arrival
// order varies, but `merged[w] += n` is an associative, commutative monoid fold (#2385).
func RunCanonicalFN_Corpus15FaninCommutativeMapMerge(seed uint64) (finalOut string, arrivalTrace string) {
	shards := [][]string{
		{"the", "quick", "brown", "fox"},
		{"jumps", "over", "the", "lazy", "dog"},
		{"the", "dog", "barks", "and", "the", "fox"},
		{"runs", "hybrid", "quick", "hybrid"},
	}
	type taggedMap struct {
		shardID int
		counts  map[string]int
	}
	results := make(chan taggedMap, len(shards))

	for i, sh := range shards {
		go func(id int, words []string) {
			local := make(map[string]int)
			for _, w := range words {
				local[w]++
			}
			if (seed>>uint(id))&1 == 1 {
				runtime.Gosched()
			}
			Perturb(seed, id+50)
			results <- taggedMap{shardID: id, counts: local}
		}(i, sh)
	}

	merged := make(map[string]int)
	var order []int
	for i := 0; i < len(shards); i++ {
		tm := <-results
		order = append(order, tm.shardID)
		for w, n := range tm.counts {
			merged[w] += n
		}
	}
	arrivalTrace = fmt.Sprintf("%v", order)
	finalOut = fmt.Sprintf("unique=%d hybrid=%d", len(merged), merged["hybrid"])
	return finalOut, arrivalTrace
}

// 8. Canonical False Negative #3: `examples/gomod-integration/calc.hy` (`HYB-E3501`/`HYB-E3502` + `total += <-out`)
// Shards share `jobs` and `out`, so shard sums arrive in varying order, and work partitioning across
// `jobs` varies, yet `int` addition is associative and commutative so `total` is always identical!
func RunCanonicalFN_GomodCalcConcurrentSum(seed uint64) (finalOut string, shardArrival string) {
	xs := []int{3, 1, 4, 1, 5, 9, 2, 6, 5, 3, 5, 8}
	const shards = 4
	jobs := make(chan int, 16)
	out := make(chan int, shards)

	for i := 0; i < shards; i++ {
		go func(id int) {
			acc := 0
			for x := range jobs {
				if (seed^uint64(id+x))%2 == 0 {
					runtime.Gosched()
				}
				acc += x
			}
			Perturb(seed, id+80)
			out <- acc
		}(i)
	}

	for _, x := range xs {
		jobs <- x
	}
	close(jobs)

	total := 0
	var parts []int
	for i := 0; i < shards; i++ {
		part := <-out
		parts = append(parts, part)
		total += part
	}
	shardArrival = fmt.Sprintf("%v", parts)
	finalOut = fmt.Sprintf("total=%d", total)
	return finalOut, shardArrival
}

// Helper to suppress unused import warning if rand is used by tests
var _ = rand.Uint64

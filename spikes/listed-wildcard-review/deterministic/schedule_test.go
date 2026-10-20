package deterministic

import (
	"runtime"
	"sync/atomic"
	"testing"
)

func sample() []Task {
	return []Task{{2, []Op{{"recv", 0, "q"}, {"emit", 2, ""}}}, {1, []Op{{"send", 7, "q"}, {"delay", 3, ""}, {"rand", 0, ""}}}}
}
func TestClosedEffectBoundaries(t *testing.T) {
	for _, kind := range []string{"time.Now", "rand.Read", "map.range", "go.callback"} {
		p := []Task{{1, []Op{{Kind: kind}}}}
		if _, _, err := Run(p, 42, 100); err == nil {
			t.Fatal("admitted", kind)
		}
	}
	if _, _, err := Run([]Task{{1, []Op{{"recv", 0, "missing"}}}}, 42, 100); err == nil {
		t.Fatal("missed deadlock")
	}
	if _, _, err := Run(sample(), 42, 1); err == nil {
		t.Fatal("missed budget")
	}
	p := sample()
	p[1].ID = 2
	if _, _, err := Run(p, 42, 100); err == nil {
		t.Fatal("duplicate ID admitted")
	}
	_, cert, err := Run(sample(), 42, 100)
	if err != nil {
		t.Fatal(err)
	}
	if err := Verify(sample(), cert, 100); err != nil {
		t.Fatal(err)
	}
	cert.Events = "corrupted"
	if err := Verify(sample(), cert, 100); err == nil {
		t.Fatal("accepted forged replay")
	}
}
func TestMillionScheduledEventsInTenThousandReplays(t *testing.T) {
	program := []Task{}
	for id := uint64(0); id < 10; id++ {
		ops := []Op{}
		for i := 0; i < 10; i++ {
			ops = append(ops, Op{"rand", 0, ""})
		}
		program = append(program, Task{id, ops})
	}
	_, expected, err := Run(program, 42, 100)
	if err != nil {
		t.Fatal(err)
	}
	var stop atomic.Bool
	done := make(chan struct{})
	go func() {
		defer close(done)
		for !stop.Load() {
			runtime.Gosched()
		}
	}()
	defer func() { stop.Store(true); <-done }()
	for run := 0; run < 10000; run++ {
		_, actual, err := Run(program, 42, 100)
		if err != nil || actual != expected {
			t.Fatalf("replay %d differs: %v", run, err)
		}
	}
	t.Log("10,000 replays and 1,000,000 scheduled effects matched under background CPU contention")
}
func BenchmarkClosedEffects(b *testing.B) {
	p := sample()
	for b.Loop() {
		if _, _, err := Run(p, 42, 100); err != nil {
			b.Fatal(err)
		}
	}
}

func TestBlockedReceiveCannotTravelBackInLogicalTime(t *testing.T) {
	p := []Task{{1, []Op{{"delay", 10, ""}, {"send", 7, "q"}}}, {2, []Op{{"recv", 0, "q"}}}}
	events, _, err := Run(p, 42, 100)
	if err != nil {
		t.Fatal(err)
	}
	for i := 1; i < len(events); i++ {
		if events[i].Tick < events[i-1].Tick {
			t.Fatal("logical clock went backwards", events)
		}
	}
}
func emitProgram() []Task {
	tasks := make([]Task, 10)
	for id := range tasks {
		tasks[id].ID = uint64(id)
		for n := 0; n < 100; n++ {
			tasks[id].Ops = append(tasks[id].Ops, Op{"emit", uint64(n), ""})
		}
	}
	return tasks
}
func BenchmarkCertifiedEmit1000(b *testing.B) {
	p := emitProgram()
	b.ReportAllocs()
	for b.Loop() {
		if _, _, err := Run(p, 42, 1000); err != nil {
			b.Fatal(err)
		}
	}
}
func BenchmarkGoEmit1000(b *testing.B) {
	b.ReportAllocs()
	for b.Loop() {
		events := make([]Event, 1000)
		done := make(chan struct{}, 10)
		for id := 0; id < 10; id++ {
			go func(id int) {
				for n := 0; n < 100; n++ {
					events[n*10+id] = Event{uint64(n), uint64(id), "emit", uint64(n)}
				}
				done <- struct{}{}
			}(id)
		}
		for id := 0; id < 10; id++ {
			<-done
		}
		_ = digest(events)
	}
}

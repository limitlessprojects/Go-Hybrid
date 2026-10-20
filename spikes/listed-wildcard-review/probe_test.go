package listedwildcardprobe

import (
 "testing"
)
type payload struct { N int; Reply chan int }
func (p payload) run(){p.Reply<-p.N}
func closure(p payload){go func(){p.Reply<-p.N}()}
func named(p payload){go p.run()}
func TestSpawnAllocations(t *testing.T){
 for name,spawn:=range map[string]func(payload){"closure":closure,"named":named}{
  reply:=make(chan int);p:=payload{7,reply}
  allocs:=testing.AllocsPerRun(1000,func(){spawn(p);if <-reply!=7 {t.Fatal("payload mismatch")}})
  t.Logf("%s: %.2f allocations per spawn+join",name,allocs)
 }
}
func BenchmarkClosure(b *testing.B){reply:=make(chan int);p:=payload{7,reply};b.ReportAllocs();for b.Loop(){closure(p);<-reply}}
func BenchmarkNamed(b *testing.B){reply:=make(chan int);p:=payload{7,reply};b.ReportAllocs();for b.Loop(){named(p);<-reply}}
func BenchmarkPersistentWorker(b *testing.B){work:=make(chan payload);reply:=make(chan int);done:=make(chan struct{});go func(){defer close(done);for p:=range work {p.run()}}();b.Cleanup(func(){close(work);<-done});p:=payload{7,reply};b.ReportAllocs();for b.Loop(){work<-p;<-reply}}
// This negative design example compiles: a phantom witness does not consume its source.
type Own[T any] struct { Value T; witness [0]func(T) }
func move[T any](value Own[T]) Own[T] { return value }
func TestPhantomDoesNotEnforceConsumption(t *testing.T){original:=Own[int]{Value:7};first:=move(original);second:=move(original);if first.Value+second.Value!=14 {t.Fatal("unexpected control result")}}

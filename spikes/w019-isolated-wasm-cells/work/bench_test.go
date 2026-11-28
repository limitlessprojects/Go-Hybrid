package w019

import "testing"

func BenchmarkNativeChannel_Tiny64B(b *testing.B) {
	ch := make(chan [64]byte, 1)
	var payload [64]byte
	payload[0] = 1
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		ch <- payload
		out := <-ch
		if out[0] != 1 {
			b.Fatal("corrupt payload")
		}
	}
}

func BenchmarkIsolatedCell_Tiny64B(b *testing.B) {
	coord := NewCoordinator()
	cell, err := coord.SpawnCell(ModuleDescriptor{
		Name:            "bench-tiny-64b",
		ExportsCellStep: true,
	}, func(_ *CellInstance, msg WireValue) (WireValue, error) {
		return msg, nil
	})
	if err != nil {
		b.Fatal(err)
	}
	msg := WireValue{Kind: KindBytes, Bytes: make([]byte, 64)}
	msg.Bytes[0] = 1
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		reply, err := coord.SendSnapshot(cell, msg)
		if err != nil || reply.Bytes[0] != 1 {
			b.Fatalf("step failed: %v", err)
		}
	}
}

func BenchmarkNativeChannel_Coarse32KiB(b *testing.B) {
	ch := make(chan []byte, 1)
	payload := make([]byte, 32*1024)
	payload[0] = 7
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		snapshot := make([]byte, len(payload))
		copy(snapshot, payload)
		ch <- snapshot
		out := <-ch
		if out[0] != 7 {
			b.Fatal("corrupt payload")
		}
	}
}

func BenchmarkIsolatedCell_Coarse32KiB(b *testing.B) {
	coord := NewCoordinator()
	cell, err := coord.SpawnCell(ModuleDescriptor{
		Name:            "bench-coarse-32kib",
		ExportsCellStep: true,
	}, func(_ *CellInstance, msg WireValue) (WireValue, error) {
		return msg, nil
	})
	if err != nil {
		b.Fatal(err)
	}
	msg := WireValue{Kind: KindBytes, Bytes: make([]byte, 32*1024)}
	msg.Bytes[0] = 7
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		reply, err := coord.SendSnapshot(cell, msg)
		if err != nil || reply.Bytes[0] != 7 {
			b.Fatalf("step failed: %v", err)
		}
	}
}

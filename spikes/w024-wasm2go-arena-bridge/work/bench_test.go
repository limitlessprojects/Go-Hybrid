package w024wasm2go

import "testing"

func BenchmarkArenaParseAndFree(b *testing.B) {
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		doc, err := Parse(samplePayload, uint64(i+1))
		if err != nil {
			b.Fatal(err)
		}
		root, _ := doc.Root()
		if _, err := root.GetBytesZeroCopy("service"); err != nil {
			b.Fatal(err)
		}
		doc.Free()
	}
}

func BenchmarkViewGetBytesZeroCopy(b *testing.B) {
	doc, err := Parse(samplePayload, 1)
	if err != nil {
		b.Fatal(err)
	}
	defer doc.Free()
	root, _ := doc.Root()

	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		s, err := root.GetBytesZeroCopy("version")
		if err != nil || len(s) == 0 {
			b.Fatal(err)
		}
	}
}

func BenchmarkHyNopCrossing(b *testing.B) {
	b.ReportAllocs()
	var sink uint64
	for i := 0; i < b.N; i++ {
		sink ^= HyNop(uint64(i))
	}
	if sink == 0xdeadbeefcafebabe {
		b.Fatal(sink)
	}
}

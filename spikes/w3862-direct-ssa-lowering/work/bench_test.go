package ssa

import (
	"fmt"
	"strings"
	"testing"
)

// BenchmarkDirectSSALowering measures the memory and CPU latency of direct in-memory SSA generation and verification.
func BenchmarkDirectSSALowering(b *testing.B) {
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		fn := NewFunction("bench_kernel")
		fn.StackCert = &StackCertificate{
			Leaf:       true,
			FrameBytes: 32,
			NoSplit:    true,
			ABI:        "ABI0",
		}

		p1 := fn.Param("x", TypeInt64)
		p2 := fn.Param("y", TypeInt64)
		b0 := fn.Entry

		v1 := fn.Add64(b0, p1, p2)
		v2 := fn.ConstInt(b0, 42)
		v3 := fn.Mul64(b0, v1, v2)
		fn.Return(b0, v3)

		if err := SanityCheck(fn); err != nil {
			b.Fatalf("SanityCheck failed: %v", err)
		}
	}
}

// BenchmarkPlainGoTextEmission measures the CPU latency and allocations of formatted Go text generation.
func BenchmarkPlainGoTextEmission(b *testing.B) {
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		var sb strings.Builder
		sb.WriteString("package main\n\n")
		sb.WriteString("import (\n")
		sb.WriteString("\t\"fmt\"\n")
		sb.WriteString(")\n\n")
		sb.WriteString("//go:nosplit\n")
		sb.WriteString("func bench_kernel(x, y int64) int64 {\n")
		fmt.Fprintf(&sb, "\tv1 := x + y\n")
		fmt.Fprintf(&sb, "\tv2 := int64(42)\n")
		fmt.Fprintf(&sb, "\tv3 := v1 * v2\n")
		sb.WriteString("\treturn v3\n")
		sb.WriteString("}\n")

		text := sb.String()
		if len(text) == 0 {
			b.Fatal("empty generated text")
		}
	}
}

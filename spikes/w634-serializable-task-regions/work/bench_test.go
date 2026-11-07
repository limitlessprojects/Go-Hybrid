package stm

import (
	"context"
	"testing"
)

func runBenchNodes(b *testing.B, n int, sparse bool) {
	r := NewRegion()
	token := SynthesizePureToken()
	cells := make([]*TxCell[int], n)
	for i := 0; i < n; i++ {
		cells[i] = NewTxCell(r, i*10, nil)
	}
	ctx := context.Background()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		_ = r.RunTask(ctx, token, func(tx *Tx) error {
			if sparse {
				v0, err := Load(tx, cells[0])
				if err != nil {
					return err
				}
				if err := Store(tx, cells[0], v0+1); err != nil {
					return err
				}
				vn, err := Load(tx, cells[n-1])
				if err != nil {
					return err
				}
				return Store(tx, cells[n-1], vn+1)
			}
			for _, c := range cells {
				v, err := Load(tx, c)
				if err != nil {
					return err
				}
				if err := Store(tx, c, v+1); err != nil {
					return err
				}
			}
			return nil
		})
	}
}

func BenchmarkSTM_Dense_1Node(b *testing.B)    { runBenchNodes(b, 1, false) }
func BenchmarkSTM_Dense_10Nodes(b *testing.B)  { runBenchNodes(b, 10, false) }
func BenchmarkSTM_Dense_1000Nodes(b *testing.B) { runBenchNodes(b, 1000, false) }
func BenchmarkSTM_Sparse_1000Nodes(b *testing.B) { runBenchNodes(b, 1000, true) }

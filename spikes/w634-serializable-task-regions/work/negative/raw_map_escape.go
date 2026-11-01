//go:build ignore

package main

import (
	"context"
	stm "github.com/limitlessprojects/Go-Hybrid/spikes/w634-serializable-task-regions/work"
)

func main() {
	r := stm.NewRegion()
	raw := map[string]int{"hits": 1}
	// Compile-fail: raw map cannot be passed to stm.Load (requires *stm.TxCell or *stm.TxMap)
	_ = r.RunTask(context.Background(), stm.SynthesizePureToken(), func(tx *stm.Tx) error {
		_, err := stm.Load(tx, raw)
		return err
	})
}

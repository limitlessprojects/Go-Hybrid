//go:build ignore

package main

import (
	"context"
	stm "github.com/limitlessprojects/Go-Hybrid/spikes/w634-serializable-task-regions/work"
)

func main() {
	r := stm.NewRegion()
	// Compile-fail: unverified impure closure cannot forge a PureBodyToken literal
	badToken := stm.PureBodyToken{nil}
	_ = r.RunTask(context.Background(), badToken, func(tx *stm.Tx) error {
		return nil
	})
}

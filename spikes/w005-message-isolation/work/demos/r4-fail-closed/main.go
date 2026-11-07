// r4 — fail-closed boundary: values that cannot be isolated surface at the
// boundary as errors; nothing silently falls back to sharing.
package main

import (
	"fmt"
	"hybrid.dev/spikes/w005"
)

type withFn struct {
	Name string
	Run  func() int
}

type hidden struct {
	Name string
	data map[string]int // unexported — reflection cannot clone honestly
}

func main() {
	_, err := iso.Clone(withFn{Name: "job", Run: func() int { return 1 }})
	fmt.Println("func-holding struct:", err)

	_, err = iso.Clone(hidden{Name: "h", data: map[string]int{"k": 0}})
	fmt.Println("unexported-field struct:", err)

	err = iso.Spawn(func(f withFn) { _ = f.Run() }, withFn{Name: "job", Run: func() int { return 2 }})
	fmt.Println("task spawn of func-holding struct:", err)
}

package main

import "fmt"

//go:noescape
func identity(value uint64) uint64

func frameRoot()

// Keep the indirect call so the linker retains the ABIInternal wrapper.
//
//go:noinline
func invoke(f func(uint64) uint64, value uint64) uint64 {
	return f(value)
}

func main() {
	for _, value := range []uint64{0, 1, 42, ^uint64(0)} {
		if got := invoke(identity, value); got != value {
			panic("ABI0 identity returned the wrong value")
		}
	}
	frameRoot()
	fmt.Println("ABI0 identity: 4 values OK; NOSPLIT chain: returned")
}

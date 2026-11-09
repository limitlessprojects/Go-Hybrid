// Package main — the row-13 sample: an EXISTING go.mod project that adds
// Hybrid-checked code as ordinary files. This app.go is plain,
// hand-written Go — nothing about it knows about Hybrid except the
// //go:generate line that keeps the generated file fresh.
package main

import "fmt"

func main() {
	total := SumConcurrent([]int{2, 3, 5, 7, 11, 13}, 4)
	fmt.Println("total:", total)
}

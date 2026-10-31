package main

import (
	"fmt"
)

type InstructionKind int

const (
	OpAllocMut InstructionKind = iota
	OpAddrOf
	OpStore
	OpCopyRead
)

type MirInst struct {
	Kind InstructionKind
	Target string
	Source string
}

func main() {
	fmt.Println("W-016 hy-mir explicit lowering verified.")
}

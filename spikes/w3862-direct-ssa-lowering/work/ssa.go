package ssa

import (
	"fmt"
	"strings"
)

// Op represents a low-level SSA machine-independent opcode.
type Op string

const (
	OpParam       Op = "OpParam"
	OpConstInt    Op = "OpConstInt"
	OpConstBool   Op = "OpConstBool"
	OpAdd64       Op = "OpAdd64"
	OpSub64       Op = "OpSub64"
	OpMul64       Op = "OpMul64"
	OpDiv64       Op = "OpDiv64"
	OpAnd         Op = "OpAnd"
	OpOr          Op = "OpOr"
	OpXor         Op = "OpXor"
	OpLess        Op = "OpLess"
	OpEqual       Op = "OpEqual"
	OpPhi         Op = "OpPhi"
	OpLocal       Op = "OpLocal"       // Stack-promoted local memory allocation
	OpGlobal      Op = "OpGlobal"      // Heap-escaped allocation
	OpLoad        Op = "OpLoad"
	OpStore       Op = "OpStore"
	OpCall        Op = "OpCall"
	OpStaticCall  Op = "OpStaticCall"
	OpBranch      Op = "OpBranch"
	OpBranchIf    Op = "OpBranchIf"
	OpReturn      Op = "OpReturn"
)

// Type represents the primitive or aggregate type of an SSA Value.
type Type string

const (
	TypeInt64   Type = "int64"
	TypeInt32   Type = "int32"
	TypeBool    Type = "bool"
	TypePointer Type = "ptr"
	TypeVoid    Type = "void"
)

// BlockKind represents the control-flow termination shape of a basic block.
type BlockKind int

const (
	BlockEntry BlockKind = iota
	BlockPlain
	BlockIf
	BlockReturn
	BlockExit
)

func (k BlockKind) String() string {
	switch k {
	case BlockEntry:
		return "BlockEntry"
	case BlockPlain:
		return "BlockPlain"
	case BlockIf:
		return "BlockIf"
	case BlockReturn:
		return "BlockReturn"
	case BlockExit:
		return "BlockExit"
	default:
		return "BlockUnknown"
	}
}

// Value represents an immutable SSA register/value definition.
type Value struct {
	ID       int      // Unique SSA register ID (e.g. 1 -> v1)
	Op       Op       // Operation opcode
	Type     Type     // Result type
	Args     []*Value // Operands consumed by this value
	Block    *Block   // Parent basic block
	AuxInt   int64    // Immediate integer constant, offset, or alloc size
	AuxStr   string   // Symbol, identifier, or debug comment
}

func (v *Value) String() string {
	if v == nil {
		return "<nil>"
	}
	var argStrs []string
	for _, a := range v.Args {
		if a == nil {
			argStrs = append(argStrs, "<nil>")
		} else {
			argStrs = append(argStrs, fmt.Sprintf("v%d", a.ID))
		}
	}
	argsFormatted := strings.Join(argStrs, ", ")
	if v.AuxStr != "" {
		return fmt.Sprintf("v%d = %s <%s> [%s] (%s)", v.ID, v.Op, v.Type, v.AuxStr, argsFormatted)
	}
	if v.AuxInt != 0 {
		return fmt.Sprintf("v%d = %s <%s> [aux:%d] (%s)", v.ID, v.Op, v.Type, v.AuxInt, argsFormatted)
	}
	return fmt.Sprintf("v%d = %s <%s> (%s)", v.ID, v.Op, v.Type, argsFormatted)
}

// Block represents a single-entry single-exit sequence of SSA instructions.
type Block struct {
	ID      int        // Monotonic block ID
	Name    string     // Human-readable block label (e.g. "b0", "b_then")
	Kind    BlockKind  // Block control-flow kind
	Values  []*Value   // Contained instructions in evaluation order
	Preds   []*Block   // Predecessor basic blocks
	Succs   []*Block   // Successor basic blocks
	Control *Value     // Condition control value for BlockIf, or return value
}

func (b *Block) String() string {
	return fmt.Sprintf("Block(%s, ID=%d, Kind=%s, Preds=%d, Succs=%d)", b.Name, b.ID, b.Kind, len(b.Preds), len(b.Succs))
}

// AddEdgeTo links this block as a predecessor to succ.
func (b *Block) AddEdgeTo(succ *Block) {
	b.Succs = append(b.Succs, succ)
	succ.Preds = append(succ.Preds, b)
}

// StackCertificate certifies that a function frame is static, bounded, and eligible for NOSPLIT ABI0 leaf lowering.
type StackCertificate struct {
	Leaf       bool   // Proved: zero external non-inlined function calls
	FrameBytes int    // Proved: total local stack frame requirement
	NoSplit    bool   // Certified: eligible for NOSPLIT leaf assembly
	ABI        string // Target ABI ("ABI0" or "ABIInternal")
}

// Verify validates that the stack certificate adheres to the NOSPLIT boundary invariants.
func (c *StackCertificate) Verify() error {
	if c == nil {
		return nil
	}
	if c.FrameBytes > 64 {
		return fmt.Errorf("HYB-E3862-STACK-OVERFLOW: frame bytes %d exceeds 64-byte NOSPLIT limit", c.FrameBytes)
	}
	if !c.Leaf {
		return fmt.Errorf("HYB-E3862-STACK-OVERFLOW: non-leaf function cannot be marked NOSPLIT")
	}
	if !c.NoSplit {
		return fmt.Errorf("HYB-E3862-STACK-OVERFLOW: certificate missing NoSplit declaration")
	}
	return nil
}

// Function represents a complete SSA function module with CFG and metadata.
type Function struct {
	Name        string
	Params      []*Value
	Blocks      []*Block
	Entry       *Block
	StackCert   *StackCertificate
	nextValID   int
	nextBlockID int
}

// NewFunction initializes a new SSA function with an entry block.
func NewFunction(name string) *Function {
	fn := &Function{
		Name:        name,
		nextValID:   1,
		nextBlockID: 0,
	}
	entry := fn.NewBlock(BlockEntry, "b_entry")
	fn.Entry = entry
	return fn
}

// NewBlock allocates and appends a new basic block to the function.
func (fn *Function) NewBlock(kind BlockKind, name string) *Block {
	b := &Block{
		ID:     fn.nextBlockID,
		Name:   name,
		Kind:   kind,
		Values: make([]*Value, 0),
		Preds:  make([]*Block, 0),
		Succs:  make([]*Block, 0),
	}
	fn.nextBlockID++
	fn.Blocks = append(fn.Blocks, b)
	return b
}

// NewValue instantiates a new SSA value in the target block.
func (fn *Function) NewValue(b *Block, op Op, typ Type, args ...*Value) *Value {
	v := &Value{
		ID:    fn.nextValID,
		Op:    op,
		Type:  typ,
		Args:  args,
		Block: b,
	}
	fn.nextValID++
	b.Values = append(b.Values, v)
	return v
}

// Param declares a function input parameter in the entry block.
func (fn *Function) Param(name string, typ Type) *Value {
	v := fn.NewValue(fn.Entry, OpParam, typ)
	v.AuxStr = name
	fn.Params = append(fn.Params, v)
	return v
}

// ConstInt creates an immediate integer constant value.
func (fn *Function) ConstInt(b *Block, val int64) *Value {
	v := fn.NewValue(b, OpConstInt, TypeInt64)
	v.AuxInt = val
	return v
}

// ConstBool creates an immediate boolean constant value.
func (fn *Function) ConstBool(b *Block, val bool) *Value {
	v := fn.NewValue(b, OpConstBool, TypeBool)
	if val {
		v.AuxInt = 1
	} else {
		v.AuxInt = 0
	}
	return v
}

// Add64 emits an integer addition instruction.
func (fn *Function) Add64(b *Block, left, right *Value) *Value {
	return fn.NewValue(b, OpAdd64, TypeInt64, left, right)
}

// Sub64 emits an integer subtraction instruction.
func (fn *Function) Sub64(b *Block, left, right *Value) *Value {
	return fn.NewValue(b, OpSub64, TypeInt64, left, right)
}

// Mul64 emits an integer multiplication instruction.
func (fn *Function) Mul64(b *Block, left, right *Value) *Value {
	return fn.NewValue(b, OpMul64, TypeInt64, left, right)
}

// Phi emits a Phi node at the start of block b.
func (fn *Function) Phi(b *Block, typ Type, args ...*Value) *Value {
	return fn.NewValue(b, OpPhi, typ, args...)
}

// AllocLocal creates a local stack-promoted allocation.
func (fn *Function) AllocLocal(b *Block, sizeBytes int) *Value {
	v := fn.NewValue(b, OpLocal, TypePointer)
	v.AuxInt = int64(sizeBytes)
	return v
}

// AllocGlobal creates a heap-promoted allocation.
func (fn *Function) AllocGlobal(b *Block, sizeBytes int) *Value {
	v := fn.NewValue(b, OpGlobal, TypePointer)
	v.AuxInt = int64(sizeBytes)
	return v
}

// Branch configures an unconditional branch from b to target.
func (fn *Function) Branch(b *Block, target *Block) {
	b.Kind = BlockPlain
	b.AddEdgeTo(target)
}

// BranchIf configures a conditional branch from b to thenB or elseB.
func (fn *Function) BranchIf(b *Block, cond *Value, thenB, elseB *Block) {
	b.Kind = BlockIf
	b.Control = cond
	b.AddEdgeTo(thenB)
	b.AddEdgeTo(elseB)
}

// Return terminates block b with a return value.
func (fn *Function) Return(b *Block, val *Value) {
	b.Kind = BlockReturn
	b.Control = val
}

// ComputeDominators computes the dominator map using iterative fixed-point analysis.
// dom[b] is the set of all blocks that dominate b.
func (fn *Function) ComputeDominators() map[*Block]map[*Block]bool {
	dom := make(map[*Block]map[*Block]bool)

	// All blocks set
	allBlocks := make(map[*Block]bool)
	for _, b := range fn.Blocks {
		allBlocks[b] = true
	}

	// Initialize:
	// Entry dominates only itself.
	// All other blocks start dominated by all blocks.
	for _, b := range fn.Blocks {
		dom[b] = make(map[*Block]bool)
		if b == fn.Entry {
			dom[b][b] = true
		} else {
			for other := range allBlocks {
				dom[b][other] = true
			}
		}
	}

	// Iterative refinement until convergence
	changed := true
	for changed {
		changed = false
		for _, b := range fn.Blocks {
			if b == fn.Entry {
				continue
			}

			// Intersection of dom[p] for all predecessors p in b.Preds
			newDom := make(map[*Block]bool)
			if len(b.Preds) > 0 {
				first := true
				for _, p := range b.Preds {
					if first {
						for d := range dom[p] {
							newDom[d] = true
						}
						first = false
					} else {
						for d := range newDom {
							if !dom[p][d] {
								delete(newDom, d)
							}
						}
					}
				}
			}
			newDom[b] = true // Each block dominates itself

			// Check if dom[b] changed
			if len(newDom) != len(dom[b]) {
				changed = true
				dom[b] = newDom
			} else {
				for d := range newDom {
					if !dom[b][d] {
						changed = true
						dom[b] = newDom
						break
					}
				}
			}
		}
	}

	return dom
}

// Dominates checks if block a dominates block b.
func (fn *Function) Dominates(a, b *Block, dom map[*Block]map[*Block]bool) bool {
	if a == nil || b == nil {
		return false
	}
	return dom[b][a]
}

// SanityCheck performs rigorous SSA validation and returns HYB-E3862-SSA-MALFORMED upon failure.
func SanityCheck(fn *Function) error {
	if fn.Entry == nil {
		return fmt.Errorf("HYB-E3862-SSA-MALFORMED: missing entry block")
	}
	if len(fn.Entry.Preds) > 0 {
		return fmt.Errorf("HYB-E3862-SSA-MALFORMED: entry block has incoming predecessor edges")
	}

	// Invariant 1: Unique Value ID and valid block reference
	seenValues := make(map[int]*Value)
	for _, b := range fn.Blocks {
		for _, v := range b.Values {
			if v.Block != b {
				return fmt.Errorf("HYB-E3862-SSA-MALFORMED: value v%d parent pointer mismatch", v.ID)
			}
			if prev, exists := seenValues[v.ID]; exists {
				return fmt.Errorf("HYB-E3862-SSA-MALFORMED: value v%d redefined across blocks %s and %s", v.ID, prev.Block.Name, b.Name)
			}
			seenValues[v.ID] = v
		}
	}

	// Invariant 2 & 3: Phi placement and Arity Match
	for _, b := range fn.Blocks {
		seenNonPhi := false
		for _, v := range b.Values {
			if v.Op == OpPhi {
				if seenNonPhi {
					return fmt.Errorf("HYB-E3862-SSA-MALFORMED: block %s contains phi node v%d after non-phi instructions", b.Name, v.ID)
				}
				if len(v.Args) != len(b.Preds) {
					return fmt.Errorf("HYB-E3862-SSA-MALFORMED: block %s phi v%d arity %d != predecessor count %d",
						b.Name, v.ID, len(v.Args), len(b.Preds))
				}
			} else {
				seenNonPhi = true
			}
		}
	}

	// Dominance Calculation
	dom := fn.ComputeDominators()

	// Invariant 4: Dominance Safety
	for _, b := range fn.Blocks {
		valIndices := make(map[int]int)
		for idx, v := range b.Values {
			valIndices[v.ID] = idx
		}

		for _, v := range b.Values {
			if v.Op == OpPhi {
				// For phi node: arg[i] must dominate predecessor b.Preds[i]
				for i, arg := range v.Args {
					predBlock := b.Preds[i]
					if arg.Block == predBlock {
						// Defined in predecessor: valid
						continue
					}
					if !fn.Dominates(arg.Block, predBlock, dom) {
						return fmt.Errorf("HYB-E3862-SSA-MALFORMED: phi v%d arg v%d (block %s) does not dominate predecessor %s",
							v.ID, arg.ID, arg.Block.Name, predBlock.Name)
					}
				}
			} else {
				// For non-phi instructions: def must dominate use
				for _, arg := range v.Args {
					if arg.Block == b {
						// Intra-block dominance: definition must precede use
						defIdx, okDef := valIndices[arg.ID]
						useIdx, okUse := valIndices[v.ID]
						if !okDef || !okUse || defIdx >= useIdx {
							return fmt.Errorf("HYB-E3862-SSA-MALFORMED: intra-block operand v%d used before def at v%d", arg.ID, v.ID)
						}
					} else {
						// Inter-block dominance: arg.Block must dominate b
						if !fn.Dominates(arg.Block, b, dom) {
							return fmt.Errorf("HYB-E3862-SSA-MALFORMED: operand v%d (block %s) does not dominate use at v%d (block %s)",
								arg.ID, arg.Block.Name, v.ID, b.Name)
						}
					}
				}
			}
		}
	}

	// Invariant 5: Stack Certificate Verification
	if fn.StackCert != nil {
		if err := fn.StackCert.Verify(); err != nil {
			return err
		}
		// If Leaf is certified, assert zero external calls
		for _, b := range fn.Blocks {
			for _, v := range b.Values {
				if v.Op == OpCall && fn.StackCert.Leaf {
					return fmt.Errorf("HYB-E3862-STACK-OVERFLOW: function marked leaf but contains OpCall v%d", v.ID)
				}
			}
		}
	}

	return nil
}

// LexicalShadowSymbolTable resolves symbols via direct register mapping rather than string names.
type LexicalShadowSymbolTable struct {
	symbols map[string]*Value
}

func NewSymbolTable() *LexicalShadowSymbolTable {
	return &LexicalShadowSymbolTable{
		symbols: make(map[string]*Value),
	}
}

func (s *LexicalShadowSymbolTable) Bind(name string, v *Value) {
	s.symbols[name] = v
}

func (s *LexicalShadowSymbolTable) Resolve(name string) (*Value, bool) {
	v, ok := s.symbols[name]
	return v, ok
}

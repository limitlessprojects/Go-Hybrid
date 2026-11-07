package ssa

import (
	"strings"
	"testing"
)

// TestSSA_LinearAdmitted verifies basic arithmetic lowering into SSA with valid StackCertificate.
func TestSSA_LinearAdmitted(t *testing.T) {
	fn := NewFunction("add_kernel")
	fn.StackCert = &StackCertificate{
		Leaf:       true,
		FrameBytes: 16,
		NoSplit:    true,
		ABI:        "ABI0",
	}

	p1 := fn.Param("a", TypeInt64)
	p2 := fn.Param("b", TypeInt64)

	b0 := fn.Entry
	vAdd := fn.Add64(b0, p1, p2)
	vConst := fn.ConstInt(b0, 100)
	vRes := fn.Mul64(b0, vAdd, vConst)
	fn.Return(b0, vRes)

	if err := SanityCheck(fn); err != nil {
		t.Fatalf("expected legal linear SSA to pass sanity check, got: %v", err)
	}
}

// TestSSA_DiamondPhiAdmitted verifies if-then-else diamond control flow with a valid Phi node.
func TestSSA_DiamondPhiAdmitted(t *testing.T) {
	fn := NewFunction("max_val")
	fn.StackCert = &StackCertificate{
		Leaf:       true,
		FrameBytes: 32,
		NoSplit:    true,
		ABI:        "ABI0",
	}

	pA := fn.Param("x", TypeInt64)
	pB := fn.Param("y", TypeInt64)

	bEntry := fn.Entry
	bThen := fn.NewBlock(BlockPlain, "b_then")
	bElse := fn.NewBlock(BlockPlain, "b_else")
	bMerge := fn.NewBlock(BlockReturn, "b_merge")

	vCond := fn.NewValue(bEntry, OpLess, TypeBool, pA, pB)
	fn.BranchIf(bEntry, vCond, bThen, bElse)

	// b_then branch
	vThenVal := fn.Add64(bThen, pA, pB)
	fn.Branch(bThen, bMerge)

	// b_else branch
	vElseVal := fn.Sub64(bElse, pA, pB)
	fn.Branch(bElse, bMerge)

	// b_merge phi node
	vPhi := fn.Phi(bMerge, TypeInt64, vThenVal, vElseVal)
	fn.Return(bMerge, vPhi)

	if err := SanityCheck(fn); err != nil {
		t.Fatalf("expected legal diamond phi SSA to pass sanity check, got: %v", err)
	}
}

// TestSSA_LexicalShadowImmunity tests that identifier names colliding with stdlib packages (fmt, os)
// do not interfere with SSA lowering by virtual register index.
func TestSSA_LexicalShadowImmunity(t *testing.T) {
	fn := NewFunction("shadow_immunity")
	symTab := NewSymbolTable()

	b0 := fn.Entry

	// User defines variable named "fmt"
	vFmt := fn.ConstInt(b0, 42)
	symTab.Bind("fmt", vFmt)

	// User defines variable named "os"
	vOs := fn.ConstBool(b0, true)
	symTab.Bind("os", vOs)

	// Direct symbol resolution against SSA value definitions
	resolvedFmt, okFmt := symTab.Resolve("fmt")
	resolvedOs, okOs := symTab.Resolve("os")

	if !okFmt || resolvedFmt.ID != vFmt.ID {
		t.Fatalf("failed to resolve shadowed 'fmt' register")
	}
	if !okOs || resolvedOs.ID != vOs.ID {
		t.Fatalf("failed to resolve shadowed 'os' register")
	}

	vRes := fn.Add64(b0, resolvedFmt, resolvedFmt)
	fn.Return(b0, vRes)

	if err := SanityCheck(fn); err != nil {
		t.Fatalf("expected lexical shadow immune SSA to pass, got: %v", err)
	}
}

// TestSSA_MalformedPhiArityRejected verifies rejection when Phi arity does not match predecessor count.
func TestSSA_MalformedPhiArityRejected(t *testing.T) {
	fn := NewFunction("malformed_phi_arity")
	bEntry := fn.Entry
	bLeft := fn.NewBlock(BlockPlain, "b_left")
	bRight := fn.NewBlock(BlockPlain, "b_right")
	bJoin := fn.NewBlock(BlockReturn, "b_join")

	vCond := fn.ConstBool(bEntry, true)
	fn.BranchIf(bEntry, vCond, bLeft, bRight)

	vLeft := fn.ConstInt(bLeft, 1)
	fn.Branch(bLeft, bJoin)

	vRight := fn.ConstInt(bRight, 2)
	fn.Branch(bRight, bJoin)

	// Error: bJoin has 2 predecessors (bLeft, bRight), but phi only provides 1 argument
	vPhi := fn.Phi(bJoin, TypeInt64, vLeft)
	_ = vRight
	fn.Return(bJoin, vPhi)

	err := SanityCheck(fn)
	if err == nil {
		t.Fatalf("expected SanityCheck to reject malformed phi arity")
	}
	if !strings.Contains(err.Error(), "HYB-E3862-SSA-MALFORMED") {
		t.Fatalf("expected HYB-E3862-SSA-MALFORMED error code, got: %v", err)
	}
}

// TestSSA_PhiAfterNonPhiRejected verifies that placing a Phi node after non-phi instructions is rejected.
func TestSSA_PhiAfterNonPhiRejected(t *testing.T) {
	fn := NewFunction("phi_after_non_phi")
	bEntry := fn.Entry
	bTarget := fn.NewBlock(BlockReturn, "b_target")

	fn.Branch(bEntry, bTarget)

	// Non-phi instruction placed first
	vNonPhi := fn.ConstInt(bTarget, 10)
	// Phi instruction placed after non-phi
	vPhi := fn.Phi(bTarget, TypeInt64, vNonPhi)
	fn.Return(bTarget, vPhi)

	err := SanityCheck(fn)
	if err == nil {
		t.Fatalf("expected SanityCheck to reject phi after non-phi")
	}
	if !strings.Contains(err.Error(), "HYB-E3862-SSA-MALFORMED") {
		t.Fatalf("expected HYB-E3862-SSA-MALFORMED error code, got: %v", err)
	}
}

// TestSSA_DominanceViolationRejected verifies that using an operand before its definition or from
// a non-dominating block is caught.
func TestSSA_DominanceViolationRejected(t *testing.T) {
	fn := NewFunction("dominance_violation")
	bEntry := fn.Entry
	bLeft := fn.NewBlock(BlockPlain, "b_left")
	bRight := fn.NewBlock(BlockPlain, "b_right")
	bExit := fn.NewBlock(BlockReturn, "b_exit")

	vCond := fn.ConstBool(bEntry, true)
	fn.BranchIf(bEntry, vCond, bLeft, bRight)

	// vLeft is defined ONLY on the left branch
	vLeft := fn.ConstInt(bLeft, 99)
	fn.Branch(bLeft, bExit)
	fn.Branch(bRight, bExit)

	// In bExit, using vLeft directly without a phi node is invalid because bLeft does NOT dominate bExit!
	vBadUse := fn.Add64(bExit, vLeft, vLeft)
	fn.Return(bExit, vBadUse)

	err := SanityCheck(fn)
	if err == nil {
		t.Fatalf("expected SanityCheck to reject dominance violation")
	}
	if !strings.Contains(err.Error(), "HYB-E3862-SSA-MALFORMED") {
		t.Fatalf("expected HYB-E3862-SSA-MALFORMED error code, got: %v", err)
	}
}

// TestSSA_StackCertificateOverflowRejected verifies rejection when FrameBytes exceeds 64 bytes.
func TestSSA_StackCertificateOverflowRejected(t *testing.T) {
	fn := NewFunction("stack_overflow_func")
	fn.StackCert = &StackCertificate{
		Leaf:       true,
		FrameBytes: 128, // Exceeds 64B limit
		NoSplit:    true,
		ABI:        "ABI0",
	}

	b0 := fn.Entry
	vConst := fn.ConstInt(b0, 1)
	fn.Return(b0, vConst)

	err := SanityCheck(fn)
	if err == nil {
		t.Fatalf("expected SanityCheck to reject stack overflow")
	}
	if !strings.Contains(err.Error(), "HYB-E3862-STACK-OVERFLOW") {
		t.Fatalf("expected HYB-E3862-STACK-OVERFLOW error code, got: %v", err)
	}
}

// TestSSA_StackCertificateNonLeafCallRejected verifies rejection when a leaf certificate contains OpCall.
func TestSSA_StackCertificateNonLeafCallRejected(t *testing.T) {
	fn := NewFunction("non_leaf_call_func")
	fn.StackCert = &StackCertificate{
		Leaf:       true,
		FrameBytes: 16,
		NoSplit:    true,
		ABI:        "ABI0",
	}

	b0 := fn.Entry
	// External call violates Leaf certificate
	vCall := fn.NewValue(b0, OpCall, TypeInt64)
	fn.Return(b0, vCall)

	err := SanityCheck(fn)
	if err == nil {
		t.Fatalf("expected SanityCheck to reject OpCall in leaf certificate")
	}
	if !strings.Contains(err.Error(), "HYB-E3862-STACK-OVERFLOW") {
		t.Fatalf("expected HYB-E3862-STACK-OVERFLOW error code, got: %v", err)
	}
}

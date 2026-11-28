package ir

import "fmt"

// Codes is the closed catalogue of rejection diagnostics. The names are part of
// the deliverable: every refused construct must be attributable to exactly one.
var Codes = map[string]string{
	"R001_ASSIGN_CONST":          "constant assignment x = k is not injective",
	"R002_LOSSY_MASK":            "x &= k (k != ^0) clears bits, not injective",
	"R003_LOSSY_SHIFT":           "shl/shr by non-zero n drops bits",
	"R004_DISCRIMINATOR_CLOBBERED": "branch body writes the discrimination register, so the branch cannot be reconstructed from the exit state",
	"R005_DISCRIMINATOR_FOREIGN": "branch discriminates on a value that is not owned/retained state",
	"R006_ALIAS_ESCAPE":          "mutable slice/pointer alias escapes the region",
	"R007_EFFECT_CHANSEND":       "channel send inside a reversible region",
	"R008_FOREIGN_CALL":          "indirect/foreign call without an inverse contract",
	"R009_TASK_CAPTURE":          "spawn or task capture inside a reversible region",
	"R010_NONINVERTIBLE_PRIM":    "degenerate primitive (x^=x, x-=x, rot by 0, non-bijective perm, swap x,x)",
	"R011_UNBOUNDED_LOOP":        "unbounded loop: iteration count is not a function of state",
	"R012_PRIM_NOT_IN_ALGEBRA":   "primitive outside the declared algebra (multiply mixing, arbitrary lookup)",
	"R013_WRITE_IMMUTABLE_KEY":   "write to the immutable key word",
	"R014_SBOX_NOT_BIJECTIVE":    "declared substitution table is not bijective, or is undeclared",
}

// Diag is one rejection.
type Diag struct {
	Code string
	Line int
	Msg  string
}

func (d Diag) String() string {
	return fmt.Sprintf("%s (line %d): %s", d.Code, d.Line, d.Msg)
}

// Admitted reports whether the program passes every admission rule.
func Admitted(p *Program) bool { return len(Check(p)) == 0 }

// Check returns every rejection in source order. Empty means admitted.
//
// Admission is a whitelist: a statement form that is not a reversible primitive
// is refused, and the branch rule is syntactic (no write to the discriminator
// register inside either arm), which is deliberately more conservative than a
// semantic bit-level proof.
func Check(p *Program) []Diag {
	var out []Diag
	checkBlock(p, p.Stmts, &out)
	return out
}

func diag(out *[]Diag, code string, line int, format string, args ...any) {
	*out = append(*out, Diag{Code: code, Line: line, Msg: fmt.Sprintf(format, args...)})
}

func checkBlock(p *Program, stmts []*Stmt, out *[]Diag) {
	for _, st := range stmts {
		switch st.Op {
		case OpAdd, OpSub, OpXorI:
			if st.Reg == KeyReg {
				diag(out, "R013_WRITE_IMMUTABLE_KEY", st.Line, "key is a retained read-only value")
			}
		case OpRotl, OpRotr:
			if st.Reg == KeyReg {
				diag(out, "R013_WRITE_IMMUTABLE_KEY", st.Line, "key is a retained read-only value")
			} else if st.Imm%64 == 0 {
				diag(out, "R010_NONINVERTIBLE_PRIM", st.Line, "rotate by %d is the identity, its inverse is not unique", st.Imm%64)
			}
		case OpSbox:
			sb, ok := p.Sboxes[st.Sbox]
			if st.Reg == KeyReg {
				diag(out, "R013_WRITE_IMMUTABLE_KEY", st.Line, "key is a retained read-only value")
			} else if !ok {
				diag(out, "R014_SBOX_NOT_BIJECTIVE", st.Line, "table %q is used but never declared", st.Sbox)
			} else if !sb.Bijective {
				diag(out, "R014_SBOX_NOT_BIJECTIVE", st.Line, "table %q (affine A=0x%02x) is not a bijection", st.Sbox, sb.A)
			}
		case OpAddV, OpSubV, OpXor:
			if st.Reg == KeyReg {
				diag(out, "R013_WRITE_IMMUTABLE_KEY", st.Line, "key is a retained read-only value")
			} else if st.Reg == st.Reg2 {
				diag(out, "R010_NONINVERTIBLE_PRIM", st.Line, "%s x%d with itself collapses the register (x0 ^= x0 or x0 -= x0 zeroes it; doubling has no inverse)", st.Op, st.Reg)
			}
		case OpSwap:
			if st.Reg == KeyReg || st.Reg2 == KeyReg {
				diag(out, "R013_WRITE_IMMUTABLE_KEY", st.Line, "key is a retained read-only value")
			} else if st.Reg == st.Reg2 {
				diag(out, "R010_NONINVERTIBLE_PRIM", st.Line, "swap x%d,x%d is the identity and hides an intent bug", st.Reg, st.Reg2)
			}
		case OpPerm:
			if !isPermutation(st.Perm) {
				diag(out, "R010_NONINVERTIBLE_PRIM", st.Line, "perm %v is not a bijection on 8 registers", st.Perm)
			}
		case OpIf:
			if st.Foreign {
				diag(out, "R005_DISCRIMINATOR_FOREIGN", st.Line, "discriminator is not owned state; the inverse cannot re-evaluate it")
			} else if st.Bit < 0 || st.Bit > 63 {
				diag(out, "R005_DISCRIMINATOR_FOREIGN", st.Line, "discriminator bit %d is outside the word", st.Bit)
			} else {
				set := map[int]bool{}
				writtenRegs(st.Then, set)
				writtenRegs(st.Else, set)
				if set[st.Reg] {
					diag(out, "R004_DISCRIMINATOR_CLOBBERED", st.Line,
						"branch arms write x%d, the discriminator (bit %d); it must survive to the exit state", st.Reg, st.Bit)
				}
			}
			checkBlock(p, st.Then, out)
			checkBlock(p, st.Else, out)
		case OpSet:
			diag(out, "R001_ASSIGN_CONST", st.Line, "x%d = 0x%x destroys the preimage", st.Reg, st.Imm)
		case OpMask:
			if st.Imm != ^uint64(0) {
				diag(out, "R002_LOSSY_MASK", st.Line, "x%d &= 0x%x clears %d bit(s)", st.Reg, st.Imm, popcount(^st.Imm))
			}
		case OpShl, OpShr:
			if st.Imm%64 != 0 {
				diag(out, "R003_LOSSY_SHIFT", st.Line, "x%d %s %d drops %d bit(s)", st.Reg, st.Op, st.Imm%64, st.Imm%64)
			}
		case OpMul:
			diag(out, "R012_PRIM_NOT_IN_ALGEBRA", st.Line, "x%d *= 0x%x: modular multiply is not in the declared primitive algebra", st.Reg, st.Imm)
		case OpLookup:
			diag(out, "R012_PRIM_NOT_IN_ALGEBRA", st.Line, "x%d = table[x%d]: arbitrary lookup table has no proven inverse contract", st.Reg, st.Reg)
		case OpLoop:
			diag(out, "R011_UNBOUNDED_LOOP", st.Line, "loop %d: trip count is not a function of state", st.Imm)
		case OpCall:
			diag(out, "R008_FOREIGN_CALL", st.Line, "call %q has no declared inverse contract", st.Sbox)
		case OpChanSend:
			diag(out, "R007_EFFECT_CHANSEND", st.Line, "channel send is an external effect and cannot be undone by inverse execution")
		case OpAlias:
			diag(out, "R006_ALIAS_ESCAPE", st.Line, "alias %q escapes the region; the inverse would mutate state it does not own", st.Sbox)
		case OpCapture:
			diag(out, "R009_TASK_CAPTURE", st.Line, "capture %q lets another task observe mid-region state", st.Sbox)
		}
	}
}

// writtenRegs collects every register a block may write.
func writtenRegs(stmts []*Stmt, set map[int]bool) {
	for _, st := range stmts {
		switch st.Op {
		case OpAdd, OpSub, OpXorI, OpRotl, OpRotr, OpSbox, OpSet, OpMask, OpShl, OpShr, OpMul, OpLookup:
			set[st.Reg] = true
		case OpAddV, OpSubV, OpXor, OpSwap:
			set[st.Reg] = true
			set[st.Reg2] = true
		case OpPerm:
			for i := 0; i < NumRegs; i++ {
				set[i] = true
			}
		case OpIf:
			writtenRegs(st.Then, set)
			writtenRegs(st.Else, set)
		}
	}
}

func isPermutation(p [8]int) bool {
	var seen [8]bool
	for _, v := range p {
		if v < 0 || v > 7 || seen[v] {
			return false
		}
		seen[v] = true
	}
	return true
}

func popcount(v uint64) int {
	n := 0
	for ; v != 0; v &= v - 1 {
		n++
	}
	return n
}

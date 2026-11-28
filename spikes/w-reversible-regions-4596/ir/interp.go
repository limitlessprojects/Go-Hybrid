package ir

import (
	"fmt"
	"math/bits"
)

// Decision records one branch choice: if with stable ID took the then-arm.
type Decision struct {
	ID    int
	Taken bool
}

// ExecProgram runs the program forward and returns the resulting state plus the
// branch trace (needed for coverage assertions).
func ExecProgram(p *Program, s State) (State, []Decision, error) {
	var trace []Decision
	if err := execBlock(p, p.Stmts, &s, &trace); err != nil {
		return s, trace, err
	}
	return s, trace, nil
}

func execBlock(p *Program, stmts []*Stmt, s *State, trace *[]Decision) error {
	for _, st := range stmts {
		switch st.Op {
		case OpAdd:
			s[st.Reg] += st.Imm
		case OpSub:
			s[st.Reg] -= st.Imm
		case OpAddV:
			s[st.Reg] += s[st.Reg2]
		case OpSubV:
			s[st.Reg] -= s[st.Reg2]
		case OpXor:
			s[st.Reg] ^= s[st.Reg2]
		case OpXorI:
			s[st.Reg] ^= st.Imm
		case OpRotl:
			s[st.Reg] = bits.RotateLeft64(s[st.Reg], int(st.Imm%64))
		case OpRotr:
			s[st.Reg] = bits.RotateLeft64(s[st.Reg], -int(st.Imm%64))
		case OpSwap:
			s[st.Reg], s[st.Reg2] = s[st.Reg2], s[st.Reg]
		case OpPerm:
			var old State
			copy(old[:], s[:])
			for i := 0; i < NumRegs; i++ {
				s[i] = old[st.Perm[i]]
			}
		case OpSbox:
			sb, ok := p.Sboxes[st.Sbox]
			if !ok {
				return fmt.Errorf("line %d: undeclared table %q", st.Line, st.Sbox)
			}
			if !sb.Bijective {
				return fmt.Errorf("line %d: table %q is not bijective", st.Line, st.Sbox)
			}
			if st.InvSbox {
				s[st.Reg] = ByteWiseSbox(s[st.Reg], &sb.Inv)
			} else {
				s[st.Reg] = ByteWiseSbox(s[st.Reg], &sb.Table)
			}
		case OpIf:
			if st.Foreign || st.Reg < 0 {
				return fmt.Errorf("line %d: foreign discriminator", st.Line)
			}
			taken := (s[st.Reg]>>uint(st.Bit))&1 == st.BitVal
			if trace != nil {
				*trace = append(*trace, Decision{ID: st.ID, Taken: taken})
			}
			arm := st.Else
			if taken {
				arm = st.Then
			}
			if err := execBlock(p, arm, s, trace); err != nil {
				return err
			}
		default:
			return fmt.Errorf("line %d: refused statement %s reached the interpreter", st.Line, st.Op)
		}
	}
	return nil
}

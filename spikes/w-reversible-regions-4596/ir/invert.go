package ir

// Invert mechanically derives the inverse program: composition reverses, each
// primitive is replaced by its declared algebraic inverse, and a conditional
// keeps its discriminator (which admission proved survives to the exit state, so
// the same test selects the same arm when running backwards).
func Invert(p *Program) *Program {
	q := &Program{
		Name:    p.Name,
		Sboxes:  p.Sboxes,
		Meta:    p.Meta,
		IfCount: p.IfCount,
	}
	q.Stmts = invertBlock(p.Stmts)
	return q
}

func invertBlock(stmts []*Stmt) []*Stmt {
	out := make([]*Stmt, 0, len(stmts))
	for i := len(stmts) - 1; i >= 0; i-- {
		out = append(out, invertOne(stmts[i]))
	}
	return out
}

func invertOne(st *Stmt) *Stmt {
	c := *st
	switch st.Op {
	case OpAdd:
		c.Op = OpSub
	case OpSub:
		c.Op = OpAdd
	case OpAddV:
		c.Op = OpSubV
	case OpSubV:
		c.Op = OpAddV
	case OpXor, OpXorI:
		// self-inverse
	case OpRotl:
		c.Op = OpRotr
	case OpRotr:
		c.Op = OpRotl
	case OpSwap:
		// self-inverse
	case OpPerm:
		var q [8]int
		for i := 0; i < NumRegs; i++ {
			q[st.Perm[i]] = i
		}
		c.Perm = q
	case OpSbox:
		c.InvSbox = !st.InvSbox
	case OpIf:
		c.Then = invertBlock(st.Then)
		c.Else = invertBlock(st.Else)
	default:
		// Refused statements never reach Invert (admission runs first); keep the
		// statement so a bug shows up as a build error rather than silent loss.
	}
	return &c
}

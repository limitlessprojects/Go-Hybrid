package compiler

import "go/token"

// Pos represents a position in the source code.
type Pos token.Pos

// Type represents a Go-Hybrid type representation.
type Type interface {
	String() string
}

// JoinHandle represents an affine task handle that must be joined before scope exit.
type JoinHandle struct {
	T Type
}

func (j JoinHandle) String() string {
	if j.T != nil {
		return "JoinHandle[" + j.T.String() + "]"
	}
	return "JoinHandle"
}

// ChanDir represents the channel transfer direction.
type ChanDir int

const (
	SendDir ChanDir = 1 << iota
	RecvDir
)

// Chan represents a channel type.
type Chan struct {
	Dir  ChanDir
	Elem Type
}

func (c Chan) String() string {
	return "chan"
}

// Expr represents an AST expression node.
type Expr interface {
	Pos() Pos
}

// Stmt represents an AST statement node.
type Stmt interface {
	Pos() Pos
}

// IfStmt represents a conditional branch.
type IfStmt struct {
	Cond Expr
	Then Stmt
	Else Stmt
}

func (s *IfStmt) Pos() Pos { return 0 }

// ForStmt represents a loop statement.
type ForStmt struct {
	Pos  Pos
	Init Stmt
	Cond Expr
	Post Stmt
	Body Stmt
}

func (s *ForStmt) Pos() Pos { return s.Pos }

// BlockStmt represents a sequence of statements inside braces.
type BlockStmt struct {
	Pos   Pos
	Stmts []Stmt
}

func (s *BlockStmt) Pos() Pos { return s.Pos }

// ScopeStmt represents a structured task scope block (`scope { ... }`).
// Any task spawned inside must be joined or drained before the block exits.
type ScopeStmt struct {
	Pos  Pos
	Body []Stmt
}

func (s *ScopeStmt) Pos() Pos { return s.Pos }

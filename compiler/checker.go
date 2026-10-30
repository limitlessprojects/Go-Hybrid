package compiler

import "fmt"

// Checker verifies compile-time invariants and guarantees for task lifetimes.
type Checker struct {
	unjoinedTasks int
	errors        []string
}

func (c *Checker) errorf(pos Pos, format string, args ...any) {
	msg := fmt.Sprintf("[%d] %s", pos, fmt.Sprintf(format, args...))
	c.errors = append(c.errors, msg)
}

func (c *Checker) checkStmt(stmt Stmt) {
	if stmt == nil {
		return
	}
	switch s := stmt.(type) {
	case *IfStmt:
		c.checkIfStmt(s)
	case *ForStmt:
		c.checkForStmt(s)
	case *ScopeStmt:
		c.checkScopeStmt(s)
	case *BlockStmt:
		for _, child := range s.Stmts {
			c.checkStmt(child)
		}
	default:
		c.errorf(stmt.Pos(), "unexpected statement type")
	}
}

func (c *Checker) checkIfStmt(stmt *IfStmt) {
	if stmt == nil {
		return
	}
	c.checkExpr(stmt.Cond)
	c.checkStmt(stmt.Then)
	c.checkStmt(stmt.Else)
}

func (c *Checker) checkExpr(expr Expr) {
	// Expression verification stub
}

func (c *Checker) checkForStmt(stmt *ForStmt) {
	if stmt == nil {
		return
	}
	c.checkStmt(stmt.Init)
	c.checkExpr(stmt.Cond)
	c.checkStmt(stmt.Post)
	c.checkStmt(stmt.Body)
}

// checkScopeStmt implements the M1 Structured Concurrency invariant:
// all affine task handles must be joined before scope completion.
func (c *Checker) checkScopeStmt(stmt *ScopeStmt) {
	if stmt == nil {
		return
	}
	initialTasks := c.unjoinedTasks
	for _, s := range stmt.Body {
		c.checkStmt(s)
	}
	// Check for unjoined tasks escaping lexical scope
	if c.unjoinedTasks > initialTasks {
		c.errorf(stmt.Pos, "HYB-E3501: unjoined task escape: %d task(s) unjoined at scope exit", c.unjoinedTasks-initialTasks)
	}
}

// TaskSpawned registers an affine JoinHandle inside the active scope.
func (c *Checker) TaskSpawned() {
	c.unjoinedTasks++
}

// TaskJoined decrements active unjoined tasks on JoinHandle.Join().
func (c *Checker) TaskJoined() {
	if c.unjoinedTasks > 0 {
		c.unjoinedTasks--
	}
}

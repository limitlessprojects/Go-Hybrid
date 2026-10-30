package compiler

type Checker struct {
	unjoinedTasks int
}

func (c *Checker) checkStmt(stmt Stmt) {
	switch stmt := stmt.(type) {
	case *IfStmt:
		c.checkIfStmt(stmt)
	case *ForStmt:
		c.checkForStmt(stmt)
	case *ScopeStmt:
		c.checkScopeStmt(stmt)
	default:
		c.errorf(stmt.Pos(), "unexpected statement type")
	}
}

func (c *Checker) checkForStmt(stmt *ForStmt) {
	c.checkStmt(stmt.Init)
	c.checkExpr(stmt.Cond)
	c.checkStmt(stmt.Post)
	c.checkStmt(stmt.Body)
}

func (c *Checker) checkScopeStmt(stmt *ScopeStmt) {
	for _, s := range stmt.Body {
		c.checkStmt(s)
	}
	// Check for unjoined tasks
	if len(c.unjoinedTasks) > 0 {
		c.errorf(stmt.Pos, "HYB-E3501: unjoined task escape")
	}
}

type JoinHandle struct {
	T Type
}

func (c *Checker) taskSpawned() {
	c.unjoinedTasks++
}

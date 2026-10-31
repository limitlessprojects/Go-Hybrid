package compiler

// TokenType represents lexical tokens supported by Go-Hybrid.
type TokenType int

const (
	TokEOF TokenType = iota
	TokIf
	TokFor
	TokScope
	TokLBrace
	TokRBrace
	TokSemicolon
)

// Parser parses Go-Hybrid source into structured task AST nodes.
type Parser struct {
	tok    TokenType
	tokPos Pos
}

func (p *Parser) next() {
	// Lexer step stub
}

func (p *Parser) expect(tok TokenType) {
	if p.tok == tok {
		p.next()
	}
}

func (p *Parser) errorf(pos Pos, format string, args ...any) {
	// Parser error reporting stub
}

func (p *Parser) parseStmt() Stmt {
	switch p.tok {
	case TokIf:
		return p.parseIfStmt()
	case TokFor:
		return p.parseForStmt()
	case TokScope:
		return p.parseScopeStmt()
	default:
		p.errorf(p.tokPos, "unexpected token")
		return nil
	}
}

func (p *Parser) parseIfStmt() *IfStmt {
	p.next() // consume if
	cond := p.parseExpr()
	thenBody := p.parseStmt()
	return &IfStmt{Cond: cond, Then: thenBody}
}

func (p *Parser) parseExpr() Expr {
	return nil
}

func (p *Parser) parseBlockStmt() *BlockStmt {
	pos := p.tokPos
	p.expect(TokLBrace)
	var stmts []Stmt
	for p.tok != TokRBrace && p.tok != TokEOF {
		stmts = append(stmts, p.parseStmt())
	}
	p.expect(TokRBrace)
	return &BlockStmt{Pos: pos, Stmts: stmts}
}

func (p *Parser) parseForStmt() *ForStmt {
	pos := p.tokPos
	p.next() // consume FOR
	init := p.parseStmt()
	cond := p.parseExpr()
	p.expect(TokSemicolon)
	post := p.parseStmt()
	body := p.parseBlockStmt()
	return &ForStmt{Pos: pos, Init: init, Cond: cond, Post: post, Body: body}
}

// parseScopeStmt parses lexically bounded structured concurrency scopes.
func (p *Parser) parseScopeStmt() *ScopeStmt {
	pos := p.tokPos
	p.next() // consume SCOPE
	p.expect(TokLBrace)
	var stmts []Stmt
	for p.tok != TokRBrace && p.tok != TokEOF {
		stmts = append(stmts, p.parseStmt())
	}
	p.expect(TokRBrace)
	return &ScopeStmt{Pos: pos, Body: stmts}
}

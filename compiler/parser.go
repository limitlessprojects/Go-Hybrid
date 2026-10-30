package compiler

import (
	"token"
)

type Parser struct {
	tok    token.Token
	tokPos Pos
}

func (p *Parser) parseStmt() Stmt {
	switch p.tok {
	case token.IF:
		return p.parseIfStmt()
	case token.FOR:
		return p.parseForStmt()
	case token.SCOPE:
		return p.parseScopeStmt()
	default:
		p.errorf(p.tokPos, "unexpected token: %s", p.tok)
	}
}

func (p *Parser) parseForStmt() *ForStmt {
	pos := p.tokPos
	p.next() // consume FOR
	init := p.parseStmt()
	cond := p.parseExpr()
	p.next() // consume SEMICOLON
	post := p.parseStmt()
	body := p.parseBlockStmt()
	return &ForStmt{Pos: pos, Init: init, Cond: cond, Post: post, Body: body}
}

func (p *Parser) parseScopeStmt() *ScopeStmt {
	pos := p.tokPos
	p.next() // consume SCOPE
	p.expect(token.LBRACE)
	var stmts []Stmt
	for p.tok != token.RBRACE && p.tok != token.EOF {
		stmts = append(stmts, p.parseStmt())
	}
	p.expect(token.RBRACE)
	return &ScopeStmt{Pos: pos, Body: stmts}
}

type ScopeStmt struct {
	Pos  Pos
	Body []Stmt
}

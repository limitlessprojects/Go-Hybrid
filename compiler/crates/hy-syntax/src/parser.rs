//! Recursive-descent parser for the Hybrid language, producing the contract
//! AST (`crate::ast` — lead-owned, unchanged here).
//!
//! Grammar sources: SPEC-LANGUAGE.md §1.2–1.4 (skeleton) + the 20 checker
//! corpus programs + 10 spike-6 programs + examples 01–03 (ground truth
//! where the skeleton is silent; e.g. the corpus spells `if`-inits with a
//! real `;` separator and match arms with `:` or `=>`).
//!
//! Structural-literal ambiguity rule (Go's): a `{` attaches to an expression
//! ONLY as a composite literal `T{…}` when (a) the expression so far is a
//! pure type-name chain (Ident / Ident(.Ident)*), (b) it is the operand
//! start (never a postfix on a call/index), and (c) the parse context allows
//! literals (`if`/`for`/`match` headers disable them — `if x {` is a block).
//! Parentheses re-enable literals, so `(foo{…})` works anywhere.
//!
//! # Contract gaps (AST has no node; approximations are pinned by tests and
//! # listed in the parser agent's report — the lead may extend the AST)
//!   * `type X = T` (spike-6 08 / examples 08)  → `Decl::Struct` with zero
//!     fields; the aliased type is NOT representable.
//!   * named call args `seed=7` (spike-6 10)    → name dropped, value kept.
//!   * `pub` visibility                         → marker dropped.
//!   * generic type args in type/expr position  → folded into the name
//!     string (`lru_cache[k, v]`, callee field name `new[string, int]`).
//!   * `set[T]{…}` (spike-6 08)                 → `StructLit{ty: Named("set[T]"), ..}`.
//!   * `panic expr` prefix form (spike-6 04)    → `Expr::Call{Ident("panic")}`.
//!   * `break loop` label (spike-6 04)          → `Stmt::Break{label: Some("loop")}`
//!     (representable since the MVP-0 emitter batch — example 04 needs to
//!     break the LOOP from inside a select; user-named loop labels are
//!     future surface, gated on real newline-aware label grammar).
//!   * `..=` inclusive ranges (spike-6 04)      → `ForKind::Range{inclusive: true}`
//!     (representable since the MVP-0 emitter batch).
//!   * type-as-value (`make(chan bool, n)`)     → empty literal of the type
//!     (`StructLit{ty: Some(chan bool), fields: []}` / `SliceLit{elem: Some(T)}`).
//!   * fn-literal params with inferred types    → `TypeExpr::Named{name: ""}`.
//!   * associated functions (`fn greeting.build`) carry a dotted `name`,
//!     `receiver: None` (methods use `Receiver`).
//!   * `T(x)` conversions (corpus 14 `[]byte("trailer")`, spike-6 08
//!     `node_id(len(g.nodes))`) → ordinary `Expr::Call` whose callee is the
//!     type-as-value encoding above — the checker recognizes call-on-type.
//!
//! Error-code note: generic parse errors carry `HYB-E0002` (the intended
//! parse-error slot; not yet in hy-diag's table, so it passes through with
//! no fix-its). `HYB-E0001` is reserved by the registry for LX2
//! identifier-case violations, which the LEXER emits — using it for generic
//! parse errors would attach the identifier-case fix-it to unrelated
//! diagnostics.
//!
//! Receiver shape decision: `Receiver.ty` carries the receiver's BASE type
//! (`greeting`, `lru_cache[k, v]`) and `is_pointer` carries the `*` — the
//! contract's flag is the pointer-ness carrier, not a redundant copy.

use crate::ast::*;
use crate::lexer::{lex, Tok, Token, PARSE_ERR_CODE};
use hy_diag::{Diagnostic, Pos, Span};

/// Parse one source file. On success the full AST; on failure every parse
/// diagnostic collected (lexer errors first, then parser errors).
pub fn parse_file(path: &str, src: &str) -> Result<File, Vec<Diagnostic>> {
    let toks = lex(path, src)?;
    let mut p = Parser::new(path, toks);
    let file = p.parse_file_body();
    if p.diags.is_empty() {
        Ok(file)
    } else {
        Err(p.diags)
    }
}

/// Debug facility: the whole file, Debug-formatted (checker/emitter tooling).
pub fn dump_ast(file: &File) -> String {
    format!("{:#?}", file)
}

/// Unit error for `?`-style bailing; the message is recorded at the throw
/// site, recovery happens at the statement/declaration loop.
struct ParseErr;
type PResult<T> = Result<T, ParseErr>;

struct Parser {
    path: String,
    toks: Vec<Token>,
    pos: usize,
    diags: Vec<Diagnostic>,
}

impl Parser {
    fn new(path: &str, toks: Vec<Token>) -> Self {
        Parser {
            path: path.to_string(),
            toks,
            pos: 0,
            diags: Vec::new(),
        }
    }

    // -- token cursor ---------------------------------------------------------

    fn cur(&self) -> &Tok {
        &self.toks[self.pos].tok
    }

    fn nth(&self, n: usize) -> &Tok {
        let i = (self.pos + n).min(self.toks.len() - 1);
        &self.toks[i].tok
    }

    fn bump(&mut self) {
        if self.pos + 1 < self.toks.len() {
            self.pos += 1;
        }
    }

    fn at_punct(&self, p: &str) -> bool {
        matches!(self.cur(), Tok::Punct(x) if *x == p)
    }

    /// Newline sensitivity (#77): is the current token on the same source
    /// line as the end of the previous token? Spans carry line/col
    /// (hy_diag::Pos), so no lexer change is needed. Used by the
    /// expression-statement `<-` fusion guard — Hybrid has no Go-style
    /// automatic semicolon insertion, so statements are newline-delimited
    /// wherever a continuation would change meaning.
    fn same_line_as_prev(&self) -> bool {
        self.pos > 0
            && self.toks[self.pos].span.start.line == self.toks[self.pos - 1].span.end.line
    }

    fn at_kw(&self, k: &str) -> bool {
        matches!(self.cur(), Tok::Kw(x) if *x == k)
    }

    fn at_ident(&self, s: &str) -> bool {
        matches!(self.cur(), Tok::Ident(x) if x == s)
    }

    fn at_eof(&self) -> bool {
        matches!(self.cur(), Tok::Eof)
    }

    fn eat_punct(&mut self, p: &str) -> bool {
        if self.at_punct(p) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn eat_kw(&mut self, k: &str) -> bool {
        if self.at_kw(k) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn span_from(&self, start: usize) -> Span {
        let s = self.toks[start].span.start;
        let e = if self.pos > start {
            self.toks[self.pos - 1].span.end
        } else {
            self.toks[start].span.end
        };
        Span::new(s, e)
    }

    fn describe_cur(&self) -> String {
        match self.cur() {
            Tok::Eof => "end of file".to_string(),
            Tok::Ident(s) => format!("identifier `{}`", s),
            Tok::Kw(k) => format!("keyword `{}`", k),
            Tok::Int(s) | Tok::Float(s) => format!("number `{}`", s),
            Tok::CharLit(c) => format!("char literal '{}'", c),
            Tok::StrChunk(_) => "string chunk".to_string(),
            Tok::StrEnd => "end of string".to_string(),
            Tok::InterpOpen => "`{` (interpolation)".to_string(),
            Tok::InterpClose => "`}` (interpolation end)".to_string(),
            Tok::RawStr(_) => "raw string".to_string(),
            Tok::Punct(p) => format!("`{}`", p),
        }
    }

    fn err<T>(&mut self, msg: impl Into<String>) -> PResult<T> {
        let span = Span::point(self.toks[self.pos].span.start);
        self.diags.push(Diagnostic::error(
            PARSE_ERR_CODE,
            self.path.clone(),
            span,
            format!(
                "parse error: {} (at {}:{})",
                msg.into(),
                span.start.line,
                span.start.col
            ),
        ));
        Err(ParseErr)
    }

    fn expect_punct(&mut self, p: &str) -> PResult<()> {
        if self.eat_punct(p) {
            Ok(())
        } else {
            self.err(format!("expected `{}`, found {}", p, self.describe_cur()))
        }
    }

    fn expect_ident(&mut self) -> PResult<String> {
        if let Tok::Ident(s) = self.cur() {
            let s = s.clone();
            self.bump();
            Ok(s)
        } else {
            self.err(format!(
                "expected identifier, found {}",
                self.describe_cur()
            ))
        }
    }

    /// The contextual `in` of a range/iterate clause (an ordinary identifier
    /// — spike-6 04 uses `in` as a channel name).
    fn expect_in(&mut self) -> PResult<()> {
        if self.at_ident("in") {
            self.bump();
            Ok(())
        } else {
            self.err(format!("expected `in`, found {}", self.describe_cur()))
        }
    }

    // =========================================================================
    // File + declarations
    // =========================================================================

    fn parse_file_body(&mut self) -> File {
        let mut decls = Vec::new();
        // optional package clause — dropped (File carries no package field;
        // no corpus/spike/example file has one)
        if self.eat_kw("package") {
            let _ = self.expect_ident();
        }
        while !self.at_eof() {
            match self.parse_decl() {
                Ok(d) => decls.push(d),
                Err(_) => self.sync_decl(),
            }
        }
        File {
            path: self.path.clone(),
            decls,
        }
    }

    fn sync_decl(&mut self) {
        // Force-progress invariant (same as sync_stmt): the decl loop retries
        // parse_decl after this returns — returning without consuming can
        // spin it forever (found 2026-10-14: a brace mis-nest left a `}` the
        // decl loop saw forever). When we are still ON the token we entered
        // with, consume it before returning: recovery always advances.
        let start = self.pos;
        let mut depth = 0usize;
        loop {
            match self.cur() {
                Tok::Eof => return,
                Tok::Punct("{") => {
                    depth += 1;
                    self.bump();
                }
                Tok::Punct("}") => {
                    if depth == 0 {
                        if self.pos == start {
                            self.bump(); // progress guarantee (see above)
                        }
                        return;
                    }
                    depth -= 1;
                    self.bump();
                }
                _ if depth == 0 && self.is_decl_starter() => {
                    if self.pos == start {
                        self.bump(); // progress guarantee (see above)
                    }
                    return;
                }
                _ => {
                    self.bump();
                }
            }
        }
    }

    /// Top-level decl head? (used only for error recovery synchronization).
    fn is_decl_starter(&self) -> bool {
        match self.cur() {
            Tok::Kw("struct")
            | Tok::Kw("const")
            | Tok::Kw("enum")
            | Tok::Kw("type")
            | Tok::Kw("import")
            | Tok::Kw("pub") => true,
            Tok::Kw("fn") => self.is_fn_decl_shape(),
            _ => false,
        }
    }

    /// Distinguish `fn name(…)` / `fn (r T) name(…)` declarations from
    /// `fn(params) {…}` literals during recovery lookahead: a declaration has
    /// an identifier after the (possibly receiver) paren group.
    fn is_fn_decl_shape(&self) -> bool {
        if matches!(self.nth(1), Tok::Ident(_)) {
            return true;
        }
        if matches!(self.nth(1), Tok::Punct("(")) {
            // skip the balanced paren group starting at pos+1
            let mut k = self.pos + 1;
            let mut depth = 0usize;
            while k < self.toks.len() {
                match &self.toks[k].tok {
                    Tok::Punct("(") => depth += 1,
                    Tok::Punct(")") => {
                        depth -= 1;
                        if depth == 0 {
                            return matches!(
                                self.toks.get(k + 1).map(|t| &t.tok),
                                Some(Tok::Ident(_))
                            );
                        }
                    }
                    Tok::Eof => return false,
                    _ => {}
                }
                k += 1;
            }
        }
        false
    }

    fn parse_decl(&mut self) -> PResult<Decl> {
        let start = self.pos;
        // Visibility marker: `pub fn` carries it into the AST (package
        // emission exports via M4's mapping); other decls drop it (same
        // behavior as before the flag existed).
        let mut is_pub = false;
        if self.at_kw("pub") {
            is_pub = true;
            self.bump();
        }
        if self.at_kw("import") {
            self.bump();
            if self.at_punct("(") {
                return self.err("grouped `import ( … )` is not in the corpus surface — write one import per line");
            }
            let (path_str, _) = self.parse_string_lit()?;
            return Ok(Decl::Import {
                path: path_str,
                span: self.span_from(start),
            });
        }
        if self.at_kw("const") {
            let c = self.parse_const_decl()?;
            return Ok(Decl::Const(c));
        }
        if self.at_kw("struct") {
            let s = self.parse_struct_decl()?;
            return Ok(Decl::Struct(s));
        }
        if self.at_kw("enum") {
            let e = self.parse_enum_decl()?;
            return Ok(Decl::Enum(e));
        }
        if self.at_kw("type") {
            return self.parse_type_alias();
        }
        if self.at_kw("fn") {
            let f = self.parse_fn_decl(is_pub)?;
            return Ok(Decl::Fn(f));
        }
        self.err(format!(
            "expected declaration (import/const/struct/enum/type/fn), found {}",
            self.describe_cur()
        ))
    }

    fn parse_const_decl(&mut self) -> PResult<ConstDecl> {
        let start = self.pos;
        self.bump(); // const
        let name = self.expect_ident()?;
        let ty = self.opt_type_after_name()?;
        self.expect_punct("=")?;
        let value = self.parse_expr(true)?;
        Ok(ConstDecl {
            name,
            ty,
            value,
            span: self.span_from(start),
        })
    }

    fn parse_struct_decl(&mut self) -> PResult<StructDecl> {
        let start = self.pos;
        self.bump(); // struct
        let name = self.expect_ident()?;
        let type_params = if self.at_punct("[") {
            self.parse_type_params()?
        } else {
            Vec::new()
        };
        self.expect_punct("{")?;
        let mut fields = Vec::new();
        while !self.at_punct("}") && !self.at_eof() {
            let fstart = self.pos;
            let fname = self.expect_ident()?;
            let ty = self.parse_type()?;
            fields.push(FieldDecl {
                name: fname,
                ty,
                span: self.span_from(fstart),
            });
            if !self.at_punct("}") && !self.at_eof() {
                // fields are newline-separated; no separators required
            }
        }
        self.expect_punct("}")?;
        Ok(StructDecl {
            name,
            type_params,
            fields,
            span: self.span_from(start),
        })
    }

    fn parse_enum_decl(&mut self) -> PResult<EnumDecl> {
        let start = self.pos;
        self.bump(); // enum
        let name = self.expect_ident()?;
        let type_params = if self.at_punct("[") {
            self.parse_type_params()?
        } else {
            Vec::new()
        };
        let _ = type_params;
        self.expect_punct("{")?;
        let mut variants = Vec::new();
        while !self.at_punct("}") && !self.at_eof() {
            let vname = self.expect_ident()?;
            // discriminant `= Int` — not representable in EnumVariant; skipped
            if self.eat_punct("=") {
                if !matches!(self.cur(), Tok::Int(_)) {
                    return self.err("expected integer discriminant after `=`");
                }
                self.bump();
            }
            let payload = if self.eat_punct("(") {
                let t = self.parse_type()?;
                if !self.at_punct(")") {
                    return self.err("multiple payload types are outside the corpus surface");
                }
                self.expect_punct(")")?;
                Some(t)
            } else {
                None
            };
            variants.push(EnumVariant {
                name: vname,
                payload,
            });
            self.eat_punct(",");
        }
        self.expect_punct("}")?;
        Ok(EnumDecl {
            name,
            variants,
            span: self.span_from(start),
        })
    }

    /// `type X = T` — a transparent alias (Go `type X = Y`), representable
    /// since the emitter batch (example 08's arena-id discipline needs the
    /// aliased type: `node_id(len(...))` is an int conversion, not a
    /// struct conversion). MUST-PARSE: spike-6 08 and examples 08.
    fn parse_type_alias(&mut self) -> PResult<Decl> {
        let start = self.pos;
        self.bump(); // type
        let name = self.expect_ident()?;
        self.expect_punct("=")?;
        let ty = self.parse_type()?;
        Ok(Decl::TypeAlias(TypeAliasDecl {
            name,
            ty,
            span: self.span_from(start),
        }))
    }

    fn parse_type_params(&mut self) -> PResult<Vec<TypeParam>> {
        self.expect_punct("[")?;
        let mut v = Vec::new();
        while !self.at_punct("]") && !self.at_eof() {
            let name = self.expect_ident()?;
            let constraint = if matches!(self.cur(), Tok::Ident(_)) {
                Some(self.expect_ident()?)
            } else {
                None
            };
            v.push(TypeParam { name, constraint });
            if !self.at_punct(",") && !self.at_punct("]") {
                return self.err("expected ',' or ']' in type parameter list");
            }
            self.eat_punct(",");
        }
        self.expect_punct("]")?;
        Ok(v)
    }

    fn parse_fn_decl(&mut self, is_pub: bool) -> PResult<FnDecl> {
        let start = self.pos;
        self.bump(); // fn
        let receiver = if self.at_punct("(") {
            Some(self.parse_receiver()?)
        } else {
            None
        };
        let mut name = self.expect_ident()?;
        if self.eat_punct(".") {
            // associated function (`fn greeting.build(...)`): dotted name,
            // receiver stays None — the checker matches Field-chain callees.
            let method = self.expect_ident()?;
            name = format!("{}.{}", name, method);
        }
        let type_params = if self.at_punct("[") {
            self.parse_type_params()?
        } else {
            Vec::new()
        };
        self.expect_punct("(")?;
        let params = self.parse_params()?;
        self.expect_punct(")")?;
        let results = self.parse_results()?;
        let body = if self.at_punct("{") {
            Some(self.parse_block()?)
        } else {
            None
        };
        Ok(FnDecl {
            is_pub,
            name,
            receiver,
            type_params,
            params,
            results,
            body,
            span: self.span_from(start),
        })
    }

    /// `fn (g *graph) connect(...)` / `fn (g greeting) render(...)` —
    /// ground truth shape is `(` ident Type `)` (the name comes FIRST).
    /// The contract carries pointer-ness as the `is_pointer` flag, so a `*T`
    /// receiver stores the BASE type `T` (the flag is not redundant).
    fn parse_receiver(&mut self) -> PResult<Receiver> {
        self.expect_punct("(")?;
        let name = self.expect_ident()?;
        let ty = self.parse_type()?;
        self.expect_punct(")")?;
        match ty {
            TypeExpr::Pointer { to, .. } => Ok(Receiver {
                name,
                ty: *to,
                is_pointer: true,
            }),
            ty => Ok(Receiver {
                name,
                ty,
                is_pointer: false,
            }),
        }
    }

    /// Parameter list inside `(...)`. Corpus params are always named; fn
    /// literals may omit types (`fn(w, r) { … }`, LX15) — inferred types are
    /// encoded as `TypeExpr::Named{name: ""}`.
    fn parse_params(&mut self) -> PResult<Vec<Param>> {
        let mut params = Vec::new();
        while !self.at_punct(")") && !self.at_eof() {
            let pstart = self.pos;
            let name = self.expect_ident()?;
            let ty = if self.is_type_start() {
                self.parse_type()?
            } else {
                TypeExpr::Named {
                    name: String::new(),
                    span: self.toks[pstart].span,
                }
            };
            params.push(Param {
                name,
                ty,
                span: self.span_from(pstart),
            });
            if !self.at_punct(",") && !self.at_punct(")") {
                return self.err(format!(
                    "expected ',' or ')' in parameter list, found {}",
                    self.describe_cur()
                ));
            }
            self.eat_punct(",");
        }
        Ok(params)
    }

    /// Result: bare type, or parenthesized list with optional names
    /// (`(total int, err error)` / `(int, error)` / `(code int)`).
    fn parse_results(&mut self) -> PResult<Vec<FnResult>> {
        if self.at_punct("(") {
            self.bump();
            let mut results = Vec::new();
            while !self.at_punct(")") && !self.at_eof() {
                let named =
                    matches!(self.cur(), Tok::Ident(_)) && matches!(self.nth(1), Tok::Ident(_));
                if named {
                    let name = self.expect_ident()?;
                    let ty = self.parse_type()?;
                    results.push(FnResult {
                        name: Some(name),
                        ty,
                    });
                } else {
                    let ty = self.parse_type()?;
                    results.push(FnResult { name: None, ty });
                }
                if !self.at_punct(",") && !self.at_punct(")") {
                    return self.err("expected ',' or ')' in result list");
                }
                self.eat_punct(",");
            }
            self.expect_punct(")")?;
            Ok(results)
        } else if self.is_type_start() {
            let ty = self.parse_type()?;
            Ok(vec![FnResult { name: None, ty }])
        } else {
            Ok(Vec::new())
        }
    }

    /// Optional type ascription after a bound name (`var x T`, `const n int`).
    fn opt_type_after_name(&mut self) -> PResult<Option<TypeExpr>> {
        if !self.at_punct("=") && !self.at_punct(":=") && self.is_type_start() {
            Ok(Some(self.parse_type()?))
        } else {
            Ok(None)
        }
    }

    /// Can the current token begin a type?
    fn is_type_start(&self) -> bool {
        match self.cur() {
            Tok::Punct("*") | Tok::Punct("[") | Tok::Kw("chan") | Tok::Kw("fn") => true,
            Tok::Punct("<-") => matches!(self.nth(1), Tok::Kw("chan")),
            Tok::Ident(_) => true,
            _ => false,
        }
    }

    // =========================================================================
    // Types
    // =========================================================================

    fn parse_type(&mut self) -> PResult<TypeExpr> {
        let start = self.pos;
        let t = match self.cur() {
            Tok::Punct("*") => {
                self.bump();
                let to = Box::new(self.parse_type()?);
                TypeExpr::Pointer {
                    to,
                    span: self.span_from(start),
                }
            }
            Tok::Punct("<-") if matches!(self.nth(1), Tok::Kw("chan")) => {
                self.bump();
                self.bump();
                let elem = Box::new(self.parse_type()?);
                TypeExpr::Chan {
                    dir: ChanDir::Recv,
                    elem,
                    span: self.span_from(start),
                }
            }
            Tok::Kw("chan") => {
                self.bump();
                let dir = if self.eat_punct("<-") {
                    ChanDir::Send
                } else {
                    ChanDir::Both
                };
                let elem = Box::new(self.parse_type()?);
                TypeExpr::Chan {
                    dir,
                    elem,
                    span: self.span_from(start),
                }
            }
            Tok::Punct("[") => {
                self.bump();
                if self.eat_punct("]") {
                    let elem = Box::new(self.parse_type()?);
                    TypeExpr::Slice {
                        elem,
                        span: self.span_from(start),
                    }
                } else if self.eat_punct("...") {
                    self.expect_punct("]")?;
                    let elem = Box::new(self.parse_type()?);
                    TypeExpr::Array {
                        len: None,
                        elem,
                        span: self.span_from(start),
                    }
                } else {
                    let len = Some(Box::new(self.parse_expr(true)?));
                    self.expect_punct("]")?;
                    let elem = Box::new(self.parse_type()?);
                    TypeExpr::Array {
                        len,
                        elem,
                        span: self.span_from(start),
                    }
                }
            }
            Tok::Kw("fn") => {
                self.bump();
                self.expect_punct("(")?;
                let params = self.parse_params()?;
                self.expect_punct(")")?;
                let mut results = Vec::new();
                while self.is_type_start() && self.same_line_as_prev() {
                    results.push(self.parse_type()?);
                }
                TypeExpr::Fn {
                    params,
                    results,
                    span: self.span_from(start),
                }
            }
            Tok::Ident(name) => {
                let name = name.clone();
                self.bump();
                if name == "map" && self.at_punct("[") {
                    self.bump();
                    let key = Box::new(self.parse_type()?);
                    self.expect_punct("]")?;
                    let value = Box::new(self.parse_type()?);
                    TypeExpr::Map {
                        key,
                        value,
                        span: self.span_from(start),
                    }
                } else if name == "option" && self.at_punct("[") {
                    self.bump();
                    let elem = Box::new(self.parse_type()?);
                    self.expect_punct("]")?;
                    TypeExpr::Option {
                        elem,
                        span: self.span_from(start),
                    }
                } else {
                    let mut full = name;
                    while self.at_punct(".") {
                        self.bump();
                        let seg = self.expect_ident()?;
                        full = format!("{}.{}", full, seg);
                    }
                    // generic instantiation `lru_cache[k, v]` — TypeExpr has
                    // no type-argument node (contract gap): folded into the
                    // name string, exactly as written modulo spacing.
                    if self.at_punct("[") {
                        self.bump();
                        let mut args = vec![self.parse_type()?];
                        while self.eat_punct(",") {
                            args.push(self.parse_type()?);
                        }
                        self.expect_punct("]")?;
                        let joined = args
                            .iter()
                            .map(|t| t.render())
                            .collect::<Vec<_>>()
                            .join(", ");
                        full = format!("{}[{}]", full, joined);
                    }
                    TypeExpr::Named {
                        name: full,
                        span: self.span_from(start),
                    }
                }
            }
            _ => {
                return self.err(format!("expected type, found {}", self.describe_cur()));
            }
        };
        Ok(t)
    }

    // =========================================================================
    // Statements
    // =========================================================================

    fn parse_block(&mut self) -> PResult<Block> {
        let start = self.pos;
        self.expect_punct("{")?;
        let mut stmts = Vec::new();
        while !self.at_punct("}") {
            if self.at_eof() {
                return self.err("unterminated block: expected '}'");
            }
            match self.parse_stmt() {
                Ok(s) => stmts.push(s),
                Err(_) => self.sync_stmt(),
            }
        }
        self.bump(); // }
        Ok(Block {
            stmts,
            span: self.span_from(start),
        })
    }

    fn sync_stmt(&mut self) {
        // Force-progress invariant: the caller retries parse_stmt after this
        // returns, so returning WITHOUT consuming can spin it (found by the
        // O4.9 test battery 2026-10-14: a select arm starting `case x:`
        // hung the arm loop — `case` is a stmt-starter, so sync returned
        // without progress every time). On the stmt-starter arm, consuming
        // the token we entered with guarantees progress.
        //
        // The `}` arm deliberately does NOT consume: a depth-0 `}` belongs
        // to the enclosing block, whose loop terminates on it — every
        // parse_stmt caller (block, select arm, unbraced select body) ends
        // its loop at `}`, so leaving it is safe AND keeps single-diagnostic
        // recovery for the common `x := }` typo shape (the brace survives to
        // close the block instead of being eaten and reported missing).
        let start = self.pos;
        let mut depth = 0usize;
        loop {
            match self.cur() {
                Tok::Eof => return,
                Tok::Punct(";") => {
                    self.bump();
                    return;
                }
                Tok::Punct("{") => {
                    depth += 1;
                    self.bump();
                }
                Tok::Punct("}") => {
                    if depth == 0 {
                        return; // belongs to the enclosing block (see above)
                    }
                    depth -= 1;
                    self.bump();
                }
                _ if depth == 0 && self.is_stmt_starter() => {
                    if self.pos == start {
                        self.bump(); // progress guarantee (see above)
                    }
                    return;
                }
                _ => {
                    self.bump();
                }
            }
        }
    }

    fn is_stmt_starter(&self) -> bool {
        matches!(
            self.cur(),
            Tok::Kw("let")
                | Tok::Kw("var")
                | Tok::Kw("const")
                | Tok::Kw("if")
                | Tok::Kw("for")
                | Tok::Kw("loop")
                | Tok::Kw("match")
                | Tok::Kw("select")
                | Tok::Kw("return")
                | Tok::Kw("task")
                | Tok::Kw("managed")
                | Tok::Kw("defer")
                | Tok::Kw("break")
                | Tok::Kw("continue")
        )
    }

    fn parse_stmt(&mut self) -> PResult<Stmt> {
        let start = self.pos;
        match self.cur() {
            Tok::Kw("let") => self.parse_local(LocalKind::Let, start),
            Tok::Kw("var") => self.parse_local(LocalKind::Var, start),
            Tok::Kw("const") => self.parse_local(LocalKind::Const, start),
            Tok::Kw("if") => self.parse_if(start),
            Tok::Kw("for") => self.parse_for(start),
            Tok::Kw("loop") => {
                self.bump();
                let body = self.parse_block()?;
                Ok(Stmt::For {
                    kind: ForKind::Infinite,
                    body,
                    span: self.span_from(start),
                })
            }
            Tok::Kw("match") => self.parse_match(start),
            Tok::Kw("select") => self.parse_select(start),
            Tok::Kw("return") => {
                self.bump();
                let values = if self.at_punct("}") || self.at_eof() || self.at_punct(";") {
                    Vec::new()
                } else {
                    self.parse_expr_list(true)?
                };
                Ok(Stmt::Return {
                    values,
                    span: self.span_from(start),
                })
            }
            Tok::Kw("task") => {
                self.bump();
                let e = self.parse_expr(true)?;
                match e {
                    Expr::Call { callee, args, .. } => Ok(Stmt::Task {
                        callee: *callee,
                        args,
                        span: self.span_from(start),
                    }),
                    _ => self.err("task requires a call expression: `task name(args)`"),
                }
            }
            Tok::Kw("managed") => {
                self.bump();
                // Row 14 (discussion #4): `managed("reason", "category",
                // tickets...) { ... }`. The POLICY (reason required, closed
                // category set) is the checker's — a bare block parses here
                // with empty fields and gets coded rejects (HYB-E0003/4)
                // from the checker, so every managed violation surfaces
                // through one diagnostic path. The parser enforces only the
                // payload SHAPE: string literals (computed payloads would
                // make the audit un-greppable).
                let mut payload: Vec<String> = Vec::new();
                if matches!(self.cur(), Tok::Punct("(")) {
                    self.bump();
                    loop {
                        match self.cur() {
                            Tok::Punct(")") => {
                                self.bump();
                                break;
                            }
                            Tok::Punct(",") => {
                                self.bump();
                            }
                            Tok::StrChunk(_) | Tok::RawStr(_) => {
                                let (s, _) = self.parse_string_lit()?;
                                payload.push(s);
                            }
                            _ => {
                                return self.err(format!(
                                    "managed payload takes string literals only (reason, category, tickets...); found {}",
                                    self.describe_cur()
                                ));
                            }
                        }
                    }
                }
                let body = self.parse_block()?;
                let empty = String::new();
                let reason = payload.first().unwrap_or(&empty);
                let category = payload.get(1).unwrap_or(&empty);
                Ok(Stmt::Managed {
                    reason: reason.clone(),
                    category: category.clone(),
                    tickets: payload[2.min(payload.len())..].to_vec(),
                    body,
                    span: self.span_from(start),
                })
            }
            Tok::Kw("defer") => {
                self.bump();
                let call = self.parse_expr(true)?;
                Ok(Stmt::Defer {
                    call,
                    span: self.span_from(start),
                })
            }
            Tok::Kw("break") => {
                self.bump();
                // `break loop` — the loop-keyword form (example 04: break
                // the LOOP from inside a select). The lexer skips newlines,
                // so a bare-ident label is ambiguous with the next
                // statement's first token — only `loop` is captured;
                // user-named labels arrive with real loop labels.
                let label = if self.at_kw("loop") {
                    self.bump();
                    Some("loop".to_string())
                } else {
                    None
                };
                Ok(Stmt::Break {
                    span: self.span_from(start),
                    label,
                })
            }
            Tok::Kw("continue") => {
                self.bump();
                let label = if self.at_kw("loop") {
                    self.bump();
                    Some("loop".to_string())
                } else {
                    None
                };
                Ok(Stmt::Continue {
                    span: self.span_from(start),
                    label,
                })
            }
            Tok::Punct("{") => {
                let block = self.parse_block()?;
                let span = block.span;
                Ok(Stmt::Block { block, span })
            }
            Tok::Ident(s) if s == "panic" && self.panic_prefix_follows() => {
                // `panic expr` (prefix form, spike-6 04 / examples 04).
                // `panic` is a predeclared identifier; the prefix call shape
                // is encoded as Expr::Call{Ident("panic")}.
                self.bump();
                let vstart = self.pos;
                let value = self.parse_expr(true)?;
                let vspan = self.span_from(vstart);
                let span = self.span_from(start);
                Ok(Stmt::Expr {
                    expr: Expr::Call {
                        callee: Box::new(Expr::Ident {
                            name: "panic".to_string(),
                            span: self.span_from(start),
                        }),
                        args: vec![CallArg {
                            name: None,
                            value,
                            spread: false,
                            span: vspan,
                        }],
                        span,
                    },
                    span,
                })
            }
            _ => self.parse_simple_stmt(start),
        }
    }

    fn panic_prefix_follows(&self) -> bool {
        matches!(self.nth(1), Tok::StrChunk(_)) || matches!(self.nth(1), Tok::RawStr(_))
    }

    /// `let`/`var`/`const` local. `let x := e` and `let x = e` both normalize
    /// to `LocalKind::Let` (D14 — immutable binding either way).
    fn parse_local(&mut self, kind: LocalKind, start: usize) -> PResult<Stmt> {
        self.bump(); // let / var / const
        let name = self.expect_ident()?;
        let ty = self.opt_type_after_name()?;
        let mut values = Vec::new();
        if self.eat_punct(":=") || self.eat_punct("=") {
            values = self.parse_expr_list(true)?;
        }
        Ok(Stmt::Local(LocalDecl {
            kind,
            names: vec![name],
            ty,
            values,
            span: self.span_from(start),
        }))
    }

    /// Expression statement / define / assignment / inc-dec / channel send —
    /// newlines are whitespace, so the statement ends wherever the grammar
    /// says (no semicolon scanning).
    fn parse_simple_stmt(&mut self, start: usize) -> PResult<Stmt> {
        let mut exprs = vec![self.parse_expr(true)?];
        while self.at_punct(",") {
            self.bump();
            exprs.push(self.parse_expr(true)?);
        }

        if self.eat_punct(":=") {
            let mut names = Vec::with_capacity(exprs.len());
            for e in exprs {
                match e {
                    Expr::Ident { name, .. } => names.push(name),
                    _ => return self.err("left side of `:=` must be identifiers"),
                }
            }
            let values = self.parse_expr_list(true)?;
            return Ok(Stmt::Local(LocalDecl {
                kind: LocalKind::Define,
                names,
                ty: None,
                values,
                span: self.span_from(start),
            }));
        }

        if let Some(op) = self.cur_assign_op() {
            self.bump();
            let mut targets = Vec::with_capacity(exprs.len());
            for e in exprs {
                targets.push(self.place_of(e)?);
            }
            let values = self.parse_expr_list(true)?;
            return Ok(Stmt::Assign {
                targets,
                op,
                values,
                span: self.span_from(start),
            });
        }

        if self.at_punct("++") || self.at_punct("--") {
            let op = if self.at_punct("++") {
                AssignOp::Inc
            } else {
                AssignOp::Dec
            };
            self.bump();
            if exprs.len() != 1 {
                return self.err("++/-- applies to a single place");
            }
            let target = self.place_of(exprs.pop().unwrap())?;
            return Ok(Stmt::Assign {
                targets: vec![target],
                op,
                values: Vec::new(),
                span: self.span_from(start),
            });
        }

        // Newline-sensitive send fusion (#77): `<-` continues the statement
        // as a SEND only when it sits on the SAME line as the end of the
        // expression list (Go ASI-style). A `<-` on the next line starts a
        // RECEIVE statement of its own — `noop()` / `<-done` must parse as
        // two statements, not fuse into `noop() <- done` (which emitted Go
        // that failed `go build`).
        if self.at_punct("<-") && self.same_line_as_prev() {
            self.bump();
            if exprs.len() != 1 {
                return self.err("channel send takes a single channel");
            }
            let chan = exprs.pop().unwrap();
            let value = self.parse_expr(true)?;
            let (is_move, value) = match value {
                Expr::Move { operand, .. } => (true, *operand),
                v => (false, v),
            };
            return Ok(Stmt::Send {
                chan,
                value,
                is_move,
                span: self.span_from(start),
            });
        }

        if exprs.len() != 1 {
            return self.err("expected `:=` or an assignment operator after the expression list");
        }
        let span = self.span_from(start);
        let expr = exprs.pop().unwrap();
        Ok(Stmt::Expr { expr, span })
    }

    fn cur_assign_op(&self) -> Option<AssignOp> {
        match self.cur() {
            Tok::Punct("=") => Some(AssignOp::Assign),
            Tok::Punct("+=") => Some(AssignOp::AddAssign),
            Tok::Punct("-=") => Some(AssignOp::SubAssign),
            Tok::Punct("*=") => Some(AssignOp::MulAssign),
            Tok::Punct("/=") => Some(AssignOp::DivAssign),
            Tok::Punct("%=") => Some(AssignOp::ModAssign),
            Tok::Punct("|=") => Some(AssignOp::BitOrAssign),
            Tok::Punct("&=") => Some(AssignOp::BitAndAssign),
            Tok::Punct("^=") => Some(AssignOp::BitXorAssign),
            Tok::Punct("<<=") => Some(AssignOp::ShlAssign),
            Tok::Punct(">>=") => Some(AssignOp::ShrAssign),
            _ => None,
        }
    }

    /// `if [init;] cond { } else …` — the header expression list is parsed
    /// with struct literals DISABLED (`if x {` is a block, not a composite
    /// literal); initializer VALUES re-enable them.
    fn parse_if(&mut self, start: usize) -> PResult<Stmt> {
        self.bump(); // if
        let mut exprs = vec![self.parse_expr(false)?];
        while self.at_punct(",") {
            self.bump();
            exprs.push(self.parse_expr(false)?);
        }
        let (init, cond) = if self.eat_punct(":=") {
            let vstart = self.pos;
            let mut names = Vec::with_capacity(exprs.len());
            for e in exprs {
                match e {
                    Expr::Ident { name, .. } => names.push(name),
                    _ => return self.err("left side of `:=` must be identifiers"),
                }
            }
            let values = self.parse_expr_list(true)?;
            let init = Stmt::Local(LocalDecl {
                kind: LocalKind::Define,
                names,
                ty: None,
                values,
                span: self.span_from(vstart),
            });
            self.expect_punct(";")?;
            let cond = self.parse_expr(false)?;
            (Some(Box::new(init)), cond)
        } else if self.eat_punct(";") {
            if exprs.len() != 1 {
                return self.err("expression initializer takes one expression");
            }
            let span = self.span_from(start);
            let init = Stmt::Expr {
                expr: exprs.pop().unwrap(),
                span,
            };
            let cond = self.parse_expr(false)?;
            (Some(Box::new(init)), cond)
        } else {
            if exprs.len() != 1 {
                return self.err("expected `;` after the if initializer");
            }
            (None, exprs.pop().unwrap())
        };

        let then = self.parse_block()?;
        let otherwise = if self.eat_kw("else") {
            if self.at_kw("if") {
                let istart = self.pos;
                Some(Box::new(self.parse_if(istart)?))
            } else {
                let block = self.parse_block()?;
                let span = block.span;
                Some(Box::new(Stmt::Block { block, span }))
            }
        } else {
            None
        };
        Ok(Stmt::If {
            init,
            cond,
            then,
            otherwise,
            span: self.span_from(start),
        })
    }

    fn parse_for(&mut self, start: usize) -> PResult<Stmt> {
        self.bump(); // for
        if self.at_punct("{") {
            let body = self.parse_block()?;
            return Ok(Stmt::For {
                kind: ForKind::Infinite,
                body,
                span: self.span_from(start),
            });
        }
        if self.peek_in_clause() {
            let mut vars = vec![self.expect_ident()?];
            while self.eat_punct(",") {
                vars.push(self.expect_ident()?);
            }
            self.expect_in()?;
            // iterate source / range lo — struct literals disabled: the loop
            // body's `{` must not attach to a bare identifier.
            let src = self.parse_expr(false)?;
            if self.at_punct("..") || self.at_punct("..=") {
                // `..` exclusive, `..=` inclusive (both representable —
                // example 04's `for i in 1..=8`).
                let inclusive = self.at_punct("..=");
                self.bump();
                let hi = self.parse_expr(false)?;
                if vars.len() != 1 {
                    return self.err("a range clause takes exactly one variable");
                }
                let var = vars.pop().unwrap();
                let body = self.parse_block()?;
                return Ok(Stmt::For {
                    kind: ForKind::Range {
                        var,
                        lo: src,
                        hi,
                        inclusive,
                    },
                    body,
                    span: self.span_from(start),
                });
            }
            let body = self.parse_block()?;
            return Ok(Stmt::For {
                kind: ForKind::Iterate { vars, expr: src },
                body,
                span: self.span_from(start),
            });
        }
        let cond = self.parse_expr(false)?;
        let body = self.parse_block()?;
        Ok(Stmt::For {
            kind: ForKind::While { cond },
            body,
            span: self.span_from(start),
        })
    }

    /// `Ident (, Ident)* in` lookahead (contextual `in`).
    fn peek_in_clause(&self) -> bool {
        if !matches!(self.cur(), Tok::Ident(_)) {
            return false;
        }
        let mut k = 1;
        loop {
            match self.nth(k) {
                Tok::Ident(s) if s == "in" => return true,
                Tok::Punct(",") if matches!(self.nth(k + 1), Tok::Ident(_)) => k += 2,
                _ => return false,
            }
        }
    }

    fn parse_match(&mut self, start: usize) -> PResult<Stmt> {
        self.bump(); // match
        let subject = self.parse_expr(false)?;
        self.expect_punct("{")?;
        let mut arms = Vec::new();
        while !self.at_punct("}") && !self.at_eof() {
            let pattern = self.parse_pattern()?;
            let guard = if self.at_kw("if") {
                self.bump();
                Some(self.parse_expr(false)?)
            } else {
                None
            };
            // arm separator: BOTH spellings accepted (spec `=>`, corpus `:`)
            if !(self.eat_punct(":") || self.eat_punct("=>")) {
                return self.err(format!(
                    "expected ':' or '=>' after match arm pattern, found {}",
                    self.describe_cur()
                ));
            }
            let body = if self.at_punct("{") {
                self.parse_block()?
            } else {
                // ArmBody = Statement (spec §1.3): one statement, no braces
                let s = self.parse_stmt()?;
                let span = s.span();
                Block {
                    stmts: vec![s],
                    span,
                }
            };
            arms.push(MatchArm {
                pattern,
                guard,
                body,
            });
        }
        self.expect_punct("}")?;
        Ok(Stmt::Match {
            subject,
            arms,
            span: self.span_from(start),
        })
    }

    fn parse_pattern(&mut self) -> PResult<MatchPattern> {
        match self.cur().clone() {
            Tok::Ident(s) => {
                self.bump();
                if s == "_" {
                    return Ok(MatchPattern::Wildcard);
                }
                let mut segments = vec![s];
                while self.at_punct(".") {
                    self.bump();
                    segments.push(self.expect_ident()?);
                }
                let binding = if self.eat_punct("(") {
                    let b = self.expect_ident()?;
                    if !self.at_punct(")") {
                        return self.err("a pattern binding takes one identifier");
                    }
                    self.expect_punct(")")?;
                    Some(b)
                } else {
                    None
                };
                Ok(MatchPattern::Path { segments, binding })
            }
            Tok::Int(s) => {
                self.bump();
                Ok(MatchPattern::Lit(LitKind::Int(s)))
            }
            Tok::Float(s) => {
                self.bump();
                Ok(MatchPattern::Lit(LitKind::Float(s)))
            }
            Tok::CharLit(c) => {
                self.bump();
                Ok(MatchPattern::Lit(LitKind::Char(c)))
            }
            Tok::RawStr(s) => {
                self.bump();
                Ok(MatchPattern::Lit(LitKind::RawStr(s)))
            }
            Tok::StrChunk(_) => {
                let (s, _) = self.parse_string_lit()?;
                Ok(MatchPattern::Lit(LitKind::Str(s)))
            }
            Tok::Kw("true") => {
                self.bump();
                Ok(MatchPattern::Lit(LitKind::Bool(true)))
            }
            Tok::Kw("false") => {
                self.bump();
                Ok(MatchPattern::Lit(LitKind::Bool(false)))
            }
            _ => self.err(format!(
                "expected match pattern, found {}",
                self.describe_cur()
            )),
        }
    }

    fn parse_select(&mut self, start: usize) -> PResult<Stmt> {
        self.bump(); // select
        self.expect_punct("{")?;
        let mut arms = Vec::new();
        while !self.at_punct("}") && !self.at_eof() {
            let kind = if self.at_select_default() {
                self.bump();
                SelectArmKind::Default
            } else if self.at_punct("<-") {
                // bare receive arm head: `<-deadline:`
                let e = self.parse_expr(true)?;
                match e {
                    Expr::Receive { chan, .. } => SelectArmKind::Recv {
                        names: Vec::new(),
                        chan: *chan,
                    },
                    _ => return self.err("select receive arm must be `<-chan`"),
                }
            } else {
                // statement-shaped head: `v, ok := <-sq:` / `ch <- v:`
                let stmt = self.parse_stmt()?;
                match stmt {
                    Stmt::Local(l) if l.kind == LocalKind::Define && l.values.len() == 1 => {
                        match l.values.into_iter().next().unwrap() {
                            Expr::Receive { chan, .. } => SelectArmKind::Recv {
                                names: l.names,
                                chan: *chan,
                            },
                            _ => return self.err("select arm must be send, receive, or default"),
                        }
                    }
                    Stmt::Send {
                        chan,
                        value,
                        is_move,
                        ..
                    } => SelectArmKind::Send {
                        chan,
                        value,
                        is_move,
                    },
                    Stmt::Expr {
                        expr: Expr::Receive { chan, .. },
                        ..
                    } => SelectArmKind::Recv {
                        names: Vec::new(),
                        chan: *chan,
                    },
                    _ => {
                        return self.err("select arm must be send, receive, or default");
                    }
                }
            };
            if !(self.eat_punct(":") || self.eat_punct("=>")) {
                return self.err(format!(
                    "expected ':' or '=>' after select arm, found {}",
                    self.describe_cur()
                ));
            }
            let body = if self.at_punct("{") {
                self.parse_block()?
            } else {
                // unbraced arm body (spike-6 04): statements until the next
                // arm head or the closing `}`
                let bstart = self.pos;
                let mut stmts = Vec::new();
                while !self.at_punct("}") && !self.at_eof() && !self.looks_like_select_arm_head() {
                    match self.parse_stmt() {
                        Ok(s) => stmts.push(s),
                        Err(_) => self.sync_stmt(),
                    }
                }
                Block {
                    stmts,
                    span: self.span_from(bstart),
                }
            };
            arms.push(SelectArm { kind, body });
        }
        self.expect_punct("}")?;
        Ok(Stmt::Select {
            arms,
            span: self.span_from(start),
        })
    }

    /// `default` is not a reserved lexer keyword (LX3). Recognize it only
    /// in an arm head, without consuming an ordinary channel/local name.
    fn at_select_default(&self) -> bool {
        self.at_kw("default")
            || (self.at_ident("default") && matches!(self.nth(1), Tok::Punct(":" | "=>")))
    }

    /// Recovery/loop lookahead: does a new select arm start here?
    fn looks_like_select_arm_head(&self) -> bool {
        if self.at_select_default() {
            return true;
        }
        match self.cur() {
            Tok::Punct("<-") => true,
            Tok::Ident(_) => match self.nth(1) {
                Tok::Punct("<-") => true, // send arm: ch <- v:
                // recv-define arm: v := <-ch: — a `:`/`=>` must terminate the
                // channel expression, otherwise this is an arm-BODY statement
                // (`v := <-m` inside a bare-recv arm's body, recv_target.rs).
                Tok::Punct(":=") => {
                    matches!(self.nth(2), Tok::Punct("<-")) && self.recv_arm_terminator_after(2)
                }
                Tok::Punct(",") => {
                    // recv-define arm: a, b := <-ch:
                    let mut k = 1;
                    loop {
                        match self.nth(k) {
                            Tok::Punct(",") if matches!(self.nth(k + 1), Tok::Ident(_)) => k += 2,
                            _ => break,
                        }
                    }
                    // after the walk k lands on `:=` (the last name was at k-1)
                    matches!(self.nth(k), Tok::Punct(":="))
                        && matches!(self.nth(k + 1), Tok::Punct("<-"))
                        && self.recv_arm_terminator_after(k + 1)
                }
                _ => false,
            },
            _ => false,
        }
    }

    /// Lookahead from the `<-` at offset `arrow_at`: skip the channel
    /// expression (identifier with postfix selector/index/paren chains) and
    /// require the select-arm terminator `:` or `=>`. Body statements like
    /// `v := <-m` end in anything else (next statement's token), so they are
    /// not arm heads.
    fn recv_arm_terminator_after(&self, arrow_at: usize) -> bool {
        let mut k = arrow_at + 1;
        // channel expression head
        match self.nth(k) {
            Tok::Ident(_) => k += 1,
            _ => return false,
        }
        // postfix chains: .name, [expr], (args)
        loop {
            match self.nth(k) {
                Tok::Punct(".") if matches!(self.nth(k + 1), Tok::Ident(_)) => k += 2,
                Tok::Punct("[") => {
                    let mut depth = 0i32;
                    loop {
                        match self.nth(k) {
                            Tok::Punct("[") => depth += 1,
                            Tok::Punct("]") => {
                                depth -= 1;
                                if depth == 0 {
                                    k += 1;
                                    break;
                                }
                            }
                            Tok::Eof => return false,
                            _ => {}
                        }
                        k += 1;
                    }
                }
                Tok::Punct("(") => {
                    let mut depth = 0i32;
                    loop {
                        match self.nth(k) {
                            Tok::Punct("(") => depth += 1,
                            Tok::Punct(")") => {
                                depth -= 1;
                                if depth == 0 {
                                    k += 1;
                                    break;
                                }
                            }
                            Tok::Eof => return false,
                            _ => {}
                        }
                        k += 1;
                    }
                }
                _ => break,
            }
        }
        matches!(self.nth(k), Tok::Punct(":" | "=>"))
    }

    fn place_of(&mut self, e: Expr) -> PResult<Place> {
        let span = e.span();
        match e {
            Expr::Ident { name, .. } => Ok(Place {
                kind: PlaceKind::Ident(name),
                span,
            }),
            Expr::Field { base, name, .. } => {
                let base = Box::new(self.place_of(*base)?);
                Ok(Place {
                    kind: PlaceKind::Field { base, name },
                    span,
                })
            }
            Expr::Index { base, index, .. } => {
                let base = Box::new(self.place_of(*base)?);
                Ok(Place {
                    kind: PlaceKind::Index {
                        base,
                        index: *index,
                    },
                    span,
                })
            }
            Expr::AddrOf { operand, .. } => {
                let base = Box::new(self.place_of(*operand)?);
                Ok(Place {
                    kind: PlaceKind::AddrOf { base },
                    span,
                })
            }
            _ => self.err(format!("`{}` is not assignable", e.render())),
        }
    }

    // =========================================================================
    // Expressions
    // =========================================================================

    fn parse_expr_list(&mut self, lit: bool) -> PResult<Vec<Expr>> {
        let mut v = vec![self.parse_expr(lit)?];
        while self.at_punct(",") {
            self.bump();
            v.push(self.parse_expr(lit)?);
        }
        Ok(v)
    }

    fn parse_expr(&mut self, lit: bool) -> PResult<Expr> {
        self.parse_binary(lit, 1)
    }

    /// Precedence: `||` < `&&` < comparisons < `+ -` < `* / %` (Go's set).
    fn parse_binary(&mut self, lit: bool, min_prec: u8) -> PResult<Expr> {
        let start = self.pos;
        let mut lhs = self.parse_unary(lit)?;
        while let Some((op, prec)) = bin_op(self.cur()) {
            if prec < min_prec {
                break;
            }
            self.bump();
            let rhs = self.parse_binary(lit, prec + 1)?;
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span: self.span_from(start),
            };
        }
        Ok(lhs)
    }

    fn parse_unary(&mut self, lit: bool) -> PResult<Expr> {
        let start = self.pos;
        match self.cur() {
            Tok::Punct("!") => {
                self.bump();
                let operand = Box::new(self.parse_unary(lit)?);
                Ok(Expr::Unary {
                    op: UnOp::Not,
                    operand,
                    span: self.span_from(start),
                })
            }
            Tok::Punct("-") => {
                self.bump();
                let operand = Box::new(self.parse_unary(lit)?);
                Ok(Expr::Unary {
                    op: UnOp::Neg,
                    operand,
                    span: self.span_from(start),
                })
            }
            Tok::Punct("&") => {
                // `&stats`, `&reg.counters[i]` — AddrOf is THE spelling used
                // by this parser (synonymous with Unary{Addr} in the contract).
                self.bump();
                let operand = Box::new(self.parse_unary(lit)?);
                Ok(Expr::AddrOf {
                    operand,
                    span: self.span_from(start),
                })
            }
            Tok::Punct("<-") if !matches!(self.nth(1), Tok::Kw("chan")) => {
                self.bump();
                let chan = Box::new(self.parse_unary(lit)?);
                Ok(Expr::Receive {
                    chan,
                    span: self.span_from(start),
                })
            }
            Tok::Kw("move") => {
                self.bump();
                let operand = Box::new(self.parse_unary(lit)?);
                Ok(Expr::Move {
                    operand,
                    span: self.span_from(start),
                })
            }
            _ => self.parse_primary(lit),
        }
    }

    fn parse_primary(&mut self, lit: bool) -> PResult<Expr> {
        let start = self.pos;
        let e = match self.cur() {
            Tok::Int(s) => {
                let s = s.clone();
                self.bump();
                Expr::Lit {
                    kind: LitKind::Int(s),
                    span: self.span_from(start),
                }
            }
            Tok::Float(s) => {
                let s = s.clone();
                self.bump();
                Expr::Lit {
                    kind: LitKind::Float(s),
                    span: self.span_from(start),
                }
            }
            Tok::CharLit(c) => {
                let c = *c;
                self.bump();
                Expr::Lit {
                    kind: LitKind::Char(c),
                    span: self.span_from(start),
                }
            }
            Tok::RawStr(s) => {
                let s = s.clone();
                self.bump();
                Expr::Lit {
                    kind: LitKind::RawStr(s),
                    span: self.span_from(start),
                }
            }
            Tok::Kw("true") => {
                self.bump();
                Expr::Lit {
                    kind: LitKind::Bool(true),
                    span: self.span_from(start),
                }
            }
            Tok::Kw("false") => {
                self.bump();
                Expr::Lit {
                    kind: LitKind::Bool(false),
                    span: self.span_from(start),
                }
            }
            Tok::StrChunk(_) => self.parse_interp_string()?,
            Tok::Punct("(") => {
                self.bump();
                let inner = Box::new(self.parse_expr(true)?); // parens re-enable literals
                self.expect_punct(")")?;
                Expr::Paren {
                    inner,
                    span: self.span_from(start),
                }
            }
            Tok::Punct("[") => self.parse_bracket_literal(start)?,
            Tok::Punct("{") if lit => {
                // type-elided composite literal `{replicas: 2, …}` inside a
                // typed context (operand start only — never postfix)
                self.parse_struct_lit(None, start)?
            }
            Tok::Kw("fn") => self.parse_fn_lit()?,
            Tok::Kw("chan") => {
                // type-as-value: `make(chan bool, n)`. No Expr node carries a
                // bare TypeExpr (contract gap): an empty literal of the type
                // is the standing encoding.
                let ty = self.parse_type()?;
                Expr::StructLit {
                    ty: Some(ty),
                    fields: Vec::new(),
                    span: self.span_from(start),
                }
            }
            Tok::Punct("<-") if matches!(self.nth(1), Tok::Kw("chan")) => {
                let ty = self.parse_type()?;
                Expr::StructLit {
                    ty: Some(ty),
                    fields: Vec::new(),
                    span: self.span_from(start),
                }
            }
            Tok::Ident(name) => {
                let name = name.clone();
                self.bump();
                match name.as_str() {
                    "nil" => Expr::Lit {
                        kind: LitKind::Nil,
                        span: self.span_from(start),
                    },
                    "map" if self.at_punct("[") => self.parse_map_lit(start)?,
                    "set" if self.at_punct("[") => {
                        // `set[T]{…}` (spike-6 08) — no SetLit in the AST
                        // (contract gap): folded into the name; with `{…}` it
                        // becomes StructLit{ty: Named("set[T]")} via the
                        // type-name-chain rule below.
                        self.bump(); // [
                        let t = self.parse_type()?;
                        self.expect_punct("]")?;
                        Expr::Ident {
                            name: format!("set[{}]", t.render()),
                            span: self.span_from(start),
                        }
                    }
                    _ => Expr::Ident {
                        name,
                        span: self.span_from(start),
                    },
                }
            }
            _ => {
                return self.err(format!(
                    "expected expression, found {}",
                    self.describe_cur()
                ));
            }
        };
        self.parse_postfix(e, lit)
    }

    /// A `"…"` string: `StrChunk (InterpOpen expr InterpClose | StrChunk)* StrEnd`.
    /// Without interpolation this is `Lit::Str`; otherwise `Expr::Interp`.
    fn parse_interp_string(&mut self) -> PResult<Expr> {
        let start = self.pos;
        let first = match self.cur() {
            Tok::StrChunk(s) => {
                let s = s.clone();
                self.bump();
                s
            }
            _ => return self.err("expected string literal"),
        };
        let mut parts = Vec::new();
        if !first.is_empty() {
            parts.push(InterpPart::Lit(first.clone()));
        }
        let mut saw_interp = false;
        loop {
            match self.cur() {
                Tok::InterpOpen => {
                    self.bump();
                    saw_interp = true;
                    // embedded expressions parse with literals enabled
                    // (nested strings / struct lits are legal inside)
                    let e = self.parse_expr(true)?;
                    if !matches!(self.cur(), Tok::InterpClose) {
                        return self.err(format!(
                            "expected '}}' closing interpolation, found {}",
                            self.describe_cur()
                        ));
                    }
                    self.bump();
                    parts.push(InterpPart::Expr(e));
                }
                Tok::StrChunk(s) => {
                    let s = s.clone();
                    self.bump();
                    if !s.is_empty() {
                        parts.push(InterpPart::Lit(s));
                    }
                }
                Tok::StrEnd => {
                    self.bump();
                    break;
                }
                _ => return self.err("unterminated string literal"),
            }
        }
        let span = self.span_from(start);
        if saw_interp {
            Ok(Expr::Interp { parts, span })
        } else {
            Ok(Expr::Lit {
                kind: LitKind::Str(first),
                span,
            })
        }
    }

    /// A plain (non-interpolated) string — import paths, patterns.
    fn parse_string_lit(&mut self) -> PResult<(String, Span)> {
        let start = self.pos;
        match self.cur().clone() {
            Tok::StrChunk(s) => {
                self.bump();
                if !matches!(self.cur(), Tok::StrEnd) {
                    return self.err("expected a plain string literal");
                }
                self.bump();
                Ok((s, self.span_from(start)))
            }
            Tok::RawStr(s) => {
                self.bump();
                Ok((s, self.span_from(start)))
            }
            _ => self.err(format!(
                "expected string literal, found {}",
                self.describe_cur()
            )),
        }
    }

    /// Operand-start `[`: `[]T{…}` typed slice literal, `[]T` type-as-value
    /// (make argument), `[N]T{…}` / `[...]T{…}` typed array composite literal,
    /// or `[a, b, c]` untyped composite literal.
    fn parse_bracket_literal(&mut self, start: usize) -> PResult<Expr> {
        self.bump(); // [
        if self.eat_punct("]") {
            let elem = Some(Box::new(self.parse_type()?));
            if self.at_punct("{") {
                self.bump();
                let mut items = Vec::new();
                while !self.at_punct("}") && !self.at_eof() {
                    items.push(self.parse_expr(true)?);
                    if !self.at_punct(",") && !self.at_punct("}") {
                        return self.err("expected ',' or '}' in slice literal");
                    }
                    self.eat_punct(",");
                }
                self.expect_punct("}")?;
                Ok(Expr::SliceLit {
                    elem,
                    items,
                    span: self.span_from(start),
                })
            } else {
                // `[]byte` as a value (make's first argument)
                Ok(Expr::SliceLit {
                    elem,
                    items: Vec::new(),
                    span: self.span_from(start),
                })
            }
        } else if self.eat_punct("...") {
            self.expect_punct("]")?;
            let inner_elem = Box::new(self.parse_type()?);
            let ty_span = self.span_from(start);
            self.expect_punct("{")?;
            let mut items = Vec::new();
            while !self.at_punct("}") && !self.at_eof() {
                items.push(self.parse_expr(true)?);
                if !self.at_punct(",") && !self.at_punct("}") {
                    return self.err("expected ',' or '}' in array literal");
                }
                self.eat_punct(",");
            }
            self.expect_punct("}")?;
            Ok(Expr::SliceLit {
                elem: Some(Box::new(TypeExpr::Array {
                    len: None,
                    elem: inner_elem,
                    span: ty_span,
                })),
                items,
                span: self.span_from(start),
            })
        } else {
            let mut items = Vec::new();
            let mut trailing_comma = false;
            while !self.at_punct("]") && !self.at_eof() {
                items.push(self.parse_expr(true)?);
                if !self.at_punct(",") && !self.at_punct("]") {
                    return self.err("expected ',' or ']' in composite literal");
                }
                trailing_comma = self.eat_punct(",");
            }
            self.expect_punct("]")?;
            // Issue #643 (SPEC T6): `[N]T{…}` array composite literal.
            // When `[` held a single expression `N` with no trailing comma and
            // `]` is immediately followed on the same line by a type `T` and `{`,
            // parse the construct as a typed array composite literal.
            if items.len() == 1
                && !trailing_comma
                && self.same_line_as_prev()
                && self.is_type_start()
            {
                let saved_pos = self.pos;
                let saved_diags = self.diags.len();
                if let Ok(inner_elem) = self.parse_type() {
                    if self.at_punct("{") {
                        let ty_span = self.span_from(start);
                        let len_expr = items.pop().expect("single array length expr");
                        self.bump(); // {
                        let mut arr_items = Vec::new();
                        while !self.at_punct("}") && !self.at_eof() {
                            arr_items.push(self.parse_expr(true)?);
                            if !self.at_punct(",") && !self.at_punct("}") {
                                return self.err("expected ',' or '}' in array literal");
                            }
                            self.eat_punct(",");
                        }
                        self.expect_punct("}")?;
                        return Ok(Expr::SliceLit {
                            elem: Some(Box::new(TypeExpr::Array {
                                len: Some(Box::new(len_expr)),
                                elem: Box::new(inner_elem),
                                span: ty_span,
                            })),
                            items: arr_items,
                            span: self.span_from(start),
                        });
                    }
                }
                self.pos = saved_pos;
                self.diags.truncate(saved_diags);
            }
            Ok(Expr::SliceLit {
                elem: None,
                items,
                span: self.span_from(start),
            })
        }
    }

    /// `map[K]V{…}` — the operand `map` and `[` are already consumed.
    fn parse_map_lit(&mut self, start: usize) -> PResult<Expr> {
        self.bump(); // [
        let key = Box::new(self.parse_type()?);
        self.expect_punct("]")?;
        let value = Box::new(self.parse_type()?);
        self.expect_punct("{")?;
        let mut entries = Vec::new();
        while !self.at_punct("}") && !self.at_eof() {
            let k = self.parse_expr(true)?;
            self.expect_punct(":")?;
            // values allow type-elided struct literals (`"frontend": {…}`)
            let v = self.parse_expr(true)?;
            entries.push((k, v));
            if !self.at_punct(",") && !self.at_punct("}") {
                return self.err("expected ',' or '}' in map literal");
            }
            self.eat_punct(",");
        }
        self.expect_punct("}")?;
        Ok(Expr::MapLit {
            key,
            value,
            entries,
            span: self.span_from(start),
        })
    }

    fn parse_struct_lit(&mut self, ty: Option<TypeExpr>, start: usize) -> PResult<Expr> {
        self.expect_punct("{")?;
        // Type-elided positional composite literal `{1, 2}` inside a nested
        // array/slice/map literal (`[2][2]int{{1, 2}, {3, 4}}`).
        if ty.is_none()
            && !self.at_punct("}")
            && !(matches!(self.cur(), Tok::Ident(_)) && matches!(self.nth(1), Tok::Punct(":")))
        {
            let mut items = Vec::new();
            while !self.at_punct("}") && !self.at_eof() {
                items.push(self.parse_expr(true)?);
                if !self.at_punct(",") && !self.at_punct("}") {
                    return self.err("expected ',' or '}' in composite literal");
                }
                self.eat_punct(",");
            }
            self.expect_punct("}")?;
            return Ok(Expr::SliceLit {
                elem: None,
                items,
                span: self.span_from(start),
            });
        }
        let mut fields = Vec::new();
        while !self.at_punct("}") && !self.at_eof() {
            let fstart = self.pos;
            let name = self.expect_ident()?;
            self.expect_punct(":")?;
            let value = self.parse_expr(true)?;
            fields.push(FieldInit {
                name,
                value,
                span: self.span_from(fstart),
            });
            if !self.at_punct(",") && !self.at_punct("}") {
                return self.err("expected ',' or '}' in struct literal");
            }
            self.eat_punct(",");
        }
        self.expect_punct("}")?;
        Ok(Expr::StructLit {
            ty,
            fields,
            span: self.span_from(start),
        })
    }

    fn parse_fn_lit(&mut self) -> PResult<Expr> {
        let start = self.pos;
        self.bump(); // fn
        self.expect_punct("(")?;
        let params = self.parse_params()?;
        self.expect_punct(")")?;
        // SPEC EBNF: `FuncLit = "fn" [Signature] Block` — the result list is
        // part of the surface (`fn() int {…}`, `fn(a, b int) (int, error)`).
        let results = if self.at_punct("{") {
            Vec::new()
        } else {
            self.parse_results()?
        };
        if !self.at_punct("{") {
            return self.err("expected `{` to start the fn literal body");
        }
        let body = self.parse_block()?;
        Ok(Expr::FnLit {
            params,
            results,
            body,
            span: self.span_from(start),
        })
    }

    fn parse_postfix(&mut self, mut e: Expr, lit: bool) -> PResult<Expr> {
        loop {
            // composite literal `T{…}`: attaches only to a pure type-name
            // chain at a literal-allowing position (see module docs).
            if self.at_punct("{") && lit {
                if let Some(name) = type_name_chain(&e) {
                    let ty = TypeExpr::Named {
                        name,
                        span: e.span(),
                    };
                    e = self.parse_struct_lit(Some(ty), self.pos)?;
                    continue;
                }
            }
            match self.cur() {
                Tok::Punct(".") => {
                    // Newline sensitivity (#659): a `.` that opens a new line
                    // must not fold into the expression on the previous line.
                    if !self.same_line_as_prev() {
                        break;
                    }
                    self.bump();
                    let name = self.expect_ident()?;
                    let span = self.span_to_prev(e.span().start);
                    e = Expr::Field {
                        base: Box::new(e),
                        name,
                        span,
                    };
                }
                Tok::Punct("(") => {
                    // A `(` that OPENS a new source line starts a statement of
                    // its own — it is never an argument list for the expression
                    // above. Same newline-sensitivity rule as the `<-` send
                    // fusion (#77): a continuation that changes how the program
                    // parses has to sit on the same line as what it continues.
                    // Without this, `f()` / `(x)` fused into `f()(x)` and
                    // `x := one` / `(2)` into `x := one(2)` — the second
                    // statement vanished, and both forms passed `hyb check`
                    // (the second one also built, silently meaning something
                    // else). There is no `;` to force a break: the lexer has no
                    // statement terminator, so the newline is the only signal
                    // and `(` is the only postfix that can also begin a
                    // statement.
                    if !self.same_line_as_prev() {
                        break;
                    }
                    let span_start = e.span().start;
                    let args = self.parse_call_args()?;
                    let span = self.span_to_prev(span_start);
                    e = Expr::Call {
                        callee: Box::new(e),
                        args,
                        span,
                    };
                }
                Tok::Punct("?") => {
                    self.bump();
                    let span = self.span_to_prev(e.span().start);
                    e = Expr::Try {
                        operand: Box::new(e),
                        span,
                    };
                }
                Tok::Punct("[") => {
                    if self.peek_generic_args() {
                        // `lru_cache[k, v]{…}` / `lru_cache.new[string, int](2)`:
                        // type arguments folded into the callee name (contract gap)
                        self.bump(); // [
                        let mut args = vec![self.parse_type()?];
                        while self.eat_punct(",") {
                            args.push(self.parse_type()?);
                        }
                        self.expect_punct("]")?;
                        let joined = args
                            .iter()
                            .map(|t| t.render())
                            .collect::<Vec<_>>()
                            .join(", ");
                        e = self.fold_type_args(e, &joined)?;
                        continue;
                    }
                    self.bump(); // [
                                 // `buf[..n]` (LX19): a leading `..` (or `:`) omits the
                                 // low bound — 0 by Go semantics.
                    let lo = if self.at_punct(":") || self.at_punct("..") {
                        None
                    } else {
                        Some(Box::new(self.parse_expr(lit)?))
                    };
                    if self.eat_punct(":") || self.eat_punct("..") {
                        let hi = if self.at_punct("]") {
                            None
                        } else {
                            Some(Box::new(self.parse_expr(lit)?))
                        };
                        self.expect_punct("]")?;
                        let span = self.span_to_prev(e.span().start);
                        e = Expr::Slice {
                            base: Box::new(e),
                            lo,
                            hi,
                            span,
                        };
                    } else {
                        match lo {
                            Some(index) => {
                                self.expect_punct("]")?;
                                let span = self.span_to_prev(e.span().start);
                                e = Expr::Index {
                                    base: Box::new(e),
                                    index,
                                    span,
                                };
                            }
                            None => return self.err("expected expression in index"),
                        }
                    }
                }
                _ => break,
            }
        }
        Ok(e)
    }

    fn prev_span_end(&self) -> Pos {
        if self.pos > 0 {
            self.toks[self.pos - 1].span.end
        } else {
            self.toks[0].span.end
        }
    }

    /// A node span from its start position through the end of the token just
    /// consumed (postfix nodes end where their last token ended).
    fn span_to_prev(&self, start: Pos) -> Span {
        Span::new(start, self.prev_span_end())
    }

    /// Generic type arguments in expression position: a `[…]` containing a
    /// top-level comma and followed by `(` or `{`. Single-argument
    /// instantiation (`foo[int](x)`) is deliberately NOT recognized — it
    /// would collide with indexing; nothing in the corpus spells it.
    /// Type-args lookahead: `[` ... `]` followed by `(` or `{` is generic
    /// args when (a) it has a comma (`lru_cache[k, v]{…}`) or (b) the single
    /// bracket-group element is TYPE-SHAPED (idents + dots only — a literal
    /// or operator means an index expression like `thresholds[3]`).
    /// Ambiguity note: `m[k](x)` (index-then-call on a func-valued map) is
    /// not in the curated surface; generics win when the content is a bare
    /// type-shaped ident. `box[t]{v: 1}` (example 07's twin shapes) needs (b).
    fn peek_generic_args(&self) -> bool {
        if !self.at_punct("[") {
            return false;
        }
        let mut depth = 0usize;
        let mut has_comma = false;
        let mut type_shaped = false;
        let mut index_shaped = false;
        let mut k = self.pos;
        while k < self.toks.len() {
            match &self.toks[k].tok {
                Tok::Punct("[") | Tok::Punct("(") | Tok::Punct("{") => depth += 1,
                Tok::Punct("]") => {
                    if depth == 1 {
                        let after = self.toks.get(k + 1).map(|t| &t.tok);
                        if !matches!(after, Some(Tok::Punct("(")) | Some(Tok::Punct("{"))) {
                            return false;
                        }
                        return has_comma || (type_shaped && !index_shaped);
                    }
                    depth -= 1;
                }
                Tok::Punct(",") if depth == 1 => has_comma = true,
                Tok::Ident(_) | Tok::Kw("chan") | Tok::Kw("fn") if depth >= 1 => {
                    type_shaped = true
                }
                Tok::Punct("<-") if depth >= 1 => {
                    let prev_is_chan = k > 0
                        && matches!(self.toks.get(k - 1).map(|t| &t.tok), Some(Tok::Kw("chan")));
                    let next_is_chan =
                        matches!(self.toks.get(k + 1).map(|t| &t.tok), Some(Tok::Kw("chan")));
                    if prev_is_chan || next_is_chan {
                        type_shaped = true;
                    } else if depth == 1 {
                        index_shaped = true;
                    }
                }
                Tok::Punct(".") if depth >= 1 => {}
                Tok::Eof => return false,
                other => {
                    // Operators, literals, keywords inside the brackets: an
                    // index expression's shape — unless nested (type args can
                    // nest `map[k]v` inside `[…]`).
                    if depth == 1 {
                        index_shaped = true;
                        let _ = other;
                    }
                }
            }
            k += 1;
        }
        false
    }

    fn fold_type_args(&mut self, e: Expr, rendered: &str) -> PResult<Expr> {
        let suffix = format!("[{}]", rendered);
        match e {
            Expr::Ident { name, span } => Ok(Expr::Ident {
                name: format!("{}{}", name, suffix),
                span,
            }),
            Expr::Field { base, name, span } => Ok(Expr::Field {
                base,
                name: format!("{}{}", name, suffix),
                span,
            }),
            _ => self.err("type arguments are only supported on named callees"),
        }
    }

    fn parse_call_args(&mut self) -> PResult<Vec<CallArg>> {
        self.bump(); // (
        let mut args = Vec::new();
        while !self.at_punct(")") && !self.at_eof() {
            let astart = self.pos;
            // named argument `name = expr` (example 10: `seed=7`). The name
            // rides on CallArg (contract extension, logged) — Go has no
            // named args, so the emitter places it by descriptor order.
            let name =
                if matches!(self.cur(), Tok::Ident(_)) && matches!(self.nth(1), Tok::Punct("=")) {
                    let n = match self.cur() {
                        Tok::Ident(s) => s.clone(),
                        _ => unreachable!(),
                    };
                    self.bump();
                    self.bump();
                    Some(n)
                } else {
                    None
                };
            let value = self.parse_expr(true)?;
            let spread = self.eat_punct("...");
            let span = self.span_from(astart);
            args.push(CallArg {
                name,
                value,
                spread,
                span,
            });
            if !self.at_punct(",") && !self.at_punct(")") {
                return self.err("expected ',' or ')' in argument list");
            }
            self.eat_punct(",");
        }
        self.expect_punct(")")?;
        Ok(args)
    }
}

fn bin_op(t: &Tok) -> Option<(BinOp, u8)> {
    match t {
        Tok::Punct("||") => Some((BinOp::Or, 1)),
        Tok::Punct("&&") => Some((BinOp::And, 2)),
        Tok::Punct("==") => Some((BinOp::Eq, 3)),
        Tok::Punct("!=") => Some((BinOp::Ne, 3)),
        Tok::Punct("<") => Some((BinOp::Lt, 3)),
        Tok::Punct("<=") => Some((BinOp::Le, 3)),
        Tok::Punct(">") => Some((BinOp::Gt, 3)),
        Tok::Punct(">=") => Some((BinOp::Ge, 3)),
        Tok::Punct("+") => Some((BinOp::Add, 4)),
        Tok::Punct("-") => Some((BinOp::Sub, 4)),
        // AddOp continues with the bit-ops (SPEC-LANGUAGE 1.4): `|` `^`.
        Tok::Punct("|") => Some((BinOp::BitOr, 4)),
        Tok::Punct("^") => Some((BinOp::BitXor, 4)),
        Tok::Punct("*") => Some((BinOp::Mul, 5)),
        Tok::Punct("/") => Some((BinOp::Div, 5)),
        Tok::Punct("%") => Some((BinOp::Mod, 5)),
        // MulOp continues with `&` and the shifts (SPEC-LANGUAGE 1.4).
        Tok::Punct("&") => Some((BinOp::BitAnd, 5)),
        Tok::Punct("<<") => Some((BinOp::Shl, 5)),
        Tok::Punct(">>") => Some((BinOp::Shr, 5)),
        _ => None,
    }
}

/// The pure type-name chains a composite literal can attach to
/// (`pool_stats`, `serde_json.value`).
fn type_name_chain(e: &Expr) -> Option<String> {
    match e {
        Expr::Ident { name, .. } => Some(name.clone()),
        Expr::Field { base, name, .. } => type_name_chain(base).map(|b| format!("{}.{}", b, name)),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Unit tests — the shapes the checker agent and lead rely on
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_src(src: &str) -> File {
        match parse_file("test.hy", src) {
            Ok(f) => f,
            Err(d) => panic!("parse failed: {:#?}", d),
        }
    }

    /// First statement of the first fn body in the file.
    fn first_stmt(f: &File) -> &Stmt {
        &f.fns()
            .next()
            .expect("a fn")
            .body
            .as_ref()
            .expect("body")
            .stmts[0]
    }

    #[test]
    fn nested_quote_interpolation() {
        // corpus 19: fmt.println("fox count: {m["fox"]}")
        let f = parse_src(r#"fn main() { fmt.println("fox count: {m["fox"]}") }"#);
        let stmt = first_stmt(&f);
        let Stmt::Expr {
            expr: Expr::Call { args, .. },
            ..
        } = stmt
        else {
            panic!("call")
        };
        let Expr::Interp { parts, .. } = &args[0].value else {
            panic!("interp")
        };
        assert_eq!(parts.len(), 2);
        assert!(matches!(&parts[0], InterpPart::Lit(s) if s == "fox count: "));
        let InterpPart::Expr(Expr::Index { base, index, .. }) = &parts[1] else {
            panic!("index expr in interp")
        };
        assert!(matches!(&**base, Expr::Ident { name, .. } if name == "m"));
        assert!(
            matches!(&**index, Expr::Lit { kind: LitKind::Str(s), .. } if s == "fox"),
            "nested string inside interpolation"
        );
    }

    #[test]
    fn interp_with_binary_expr_and_escapes() {
        let f = parse_src(r#"fn main() { fmt.fprintf(w, "{a}+{b}={a + b}\n") }"#);
        let stmt = first_stmt(&f);
        let Stmt::Expr {
            expr: Expr::Call { args, .. },
            ..
        } = stmt
        else {
            panic!("call")
        };
        let Expr::Interp { parts, .. } = &args[1].value else {
            panic!("interp")
        };
        // {a} "+" {b} "=" {a + b} "\n"
        assert_eq!(parts.len(), 6);
        assert!(matches!(&parts[0], InterpPart::Expr(Expr::Ident { name, .. }) if name == "a"));
        assert!(matches!(&parts[1], InterpPart::Lit(s) if s == "+"));
        assert!(matches!(
            &parts[4],
            InterpPart::Expr(Expr::Binary { op: BinOp::Add, .. })
        ));
        assert!(matches!(&parts[5], InterpPart::Lit(s) if s == "\n"));
    }

    #[test]
    fn for_two_var_iterate() {
        // corpus 15: for w, n in shard_counts
        let f = parse_src("fn main() { for w, n in shard_counts { merged[w] += n } }");
        let stmt = first_stmt(&f);
        let Stmt::For {
            kind: ForKind::Iterate { vars, expr },
            ..
        } = stmt
        else {
            panic!("for")
        };
        assert_eq!(vars, &vec!["w".to_string(), "n".to_string()]);
        assert!(matches!(expr, Expr::Ident { name, .. } if name == "shard_counts"));
    }

    #[test]
    fn let_both_spellings_normalize_to_let() {
        // D14: `let x := e` and `let x = e` → LocalKind::Let (immutable)
        let f = parse_src("fn main() { let arr := serde_json.new_array() let b = 2 }");
        let fns: Vec<&FnDecl> = f.fns().collect();
        let body = fns[0].body.as_ref().unwrap();
        assert_eq!(body.stmts.len(), 2);
        assert!(matches!(
            &body.stmts[0],
            Stmt::Local(LocalDecl { kind: LocalKind::Let, names, values, .. })
                if names == &vec!["arr".to_string()] && values.len() == 1
        ));
        assert!(matches!(
            &body.stmts[1],
            Stmt::Local(LocalDecl {
                kind: LocalKind::Let,
                ..
            })
        ));
    }

    #[test]
    fn move_send_unwraps_move() {
        // corpus 15/16: results <- move local
        let f = parse_src("fn main() { results <- move local }");
        let stmt = first_stmt(&f);
        let Stmt::Send {
            chan,
            value,
            is_move,
            ..
        } = stmt
        else {
            panic!("send")
        };
        assert!(*is_move);
        assert!(matches!(chan, Expr::Ident { name, .. } if name == "results"));
        assert!(matches!(value, Expr::Ident { name, .. } if name == "local"));
    }

    #[test]
    fn plain_send() {
        let f = parse_src("fn main() { jobs <- url }");
        let stmt = first_stmt(&f);
        match stmt {
            Stmt::Send { is_move, value, .. } => {
                assert!(!is_move);
                assert!(matches!(value, Expr::Ident { name, .. } if name == "url"));
            }
            s => panic!("send, got {:?}", s),
        }
    }

    #[test]
    fn managed_block() {
        // corpus 02/18: managed("reason", "category") { … } — row 14 payload
        let f = parse_src(
            "fn main() { managed(\"shared map across tasks\", \"mutex\", \"PROJ-1\") { counts := map[string]int{} } }",
        );
        let stmt = first_stmt(&f);
        let Stmt::Managed {
            reason,
            category,
            tickets,
            body,
            ..
        } = stmt
        else {
            panic!("managed")
        };
        assert_eq!(reason, "shared map across tasks");
        assert_eq!(category, "mutex");
        assert_eq!(tickets, &vec!["PROJ-1".to_string()]);
        assert_eq!(body.stmts.len(), 1);
        assert!(matches!(&body.stmts[0], Stmt::Local(_)));
    }

    #[test]
    fn managed_bare_block_parses_empty_payload() {
        // row 14: the POLICY lives in the checker — a bare block parses with
        // empty reason/category and the checker rejects it (HYB-E0003); the
        // parser rejects only non-literal payloads (E0002 envelope).
        let f = parse_src("fn main() { managed { x := 1 } }");
        let stmt = first_stmt(&f);
        let Stmt::Managed {
            reason, category, ..
        } = stmt
        else {
            panic!("managed")
        };
        assert!(reason.is_empty() && category.is_empty());
        // `()` and reason-only payloads parse too (the checker codes them)
        parse_src("fn main() { managed() { x := 1 } }");
        parse_src("fn main() { managed(\"only reason\") { x := 1 } }");
        // computed payload rejected at parse level (literals only)
        assert!(parse_file(
            "t.hy",
            "fn main() { managed(reason, \"mutex\") { x := 1 } }"
        )
        .is_err());
    }

    #[test]
    fn slices_both_optional_bounds() {
        let f = parse_src(
            "fn main() { a := words[a0:b0] b := words[1:] c := words[:2] d := buf[..n] }",
        );
        let body = &f.fns().next().unwrap().body.as_ref().unwrap().stmts;
        let cases = [
            (true, true),  // words[a0:b0]
            (true, false), // words[1:]
            (false, true), // words[:2]
            (false, true), // buf[..n]
        ];
        for (i, (has_lo, has_hi)) in cases.iter().enumerate() {
            let Stmt::Local(LocalDecl { values, .. }) = &body[i] else {
                panic!("local")
            };
            let Expr::Slice { lo, hi, .. } = &values[0] else {
                panic!("slice")
            };
            assert_eq!(lo.is_some(), *has_lo, "case {i} lo");
            assert_eq!(hi.is_some(), *has_hi, "case {i} hi");
        }
    }

    #[test]
    fn slice_bounds_are_expressions() {
        // corpus 14: payload[i*32 : i*32+32]
        let f = parse_src("fn main() { chunk := payload[i * 32: i * 32 + 32] }");
        let stmt = first_stmt(&f);
        let Stmt::Local(LocalDecl { values, .. }) = stmt else {
            panic!("local")
        };
        let Expr::Slice { lo, hi, .. } = &values[0] else {
            panic!("slice")
        };
        assert!(matches!(
            lo.as_deref(),
            Some(Expr::Binary { op: BinOp::Mul, .. })
        ));
        assert!(matches!(
            hi.as_deref(),
            Some(Expr::Binary { op: BinOp::Add, .. })
        ));
    }

    #[test]
    fn chan_directions() {
        // corpus 03: fn worker(id int, jobs <-chan string, done chan<- bool)
        let f = parse_src("fn worker(id int, jobs <-chan string, done chan<- bool) { }");
        let fn_decl = f.fns().next().unwrap();
        let ty = |i: usize| &fn_decl.params[i].ty;
        match ty(1) {
            TypeExpr::Chan {
                dir: ChanDir::Recv,
                elem,
                ..
            } => {
                assert!(matches!(&**elem, TypeExpr::Named { name, .. } if name == "string"));
            }
            _ => panic!("<-chan string"),
        }
        match ty(2) {
            TypeExpr::Chan {
                dir: ChanDir::Send,
                elem,
                ..
            } => {
                assert!(matches!(&**elem, TypeExpr::Named { name, .. } if name == "bool"));
            }
            _ => panic!("chan<- bool"),
        }
    }

    #[test]
    fn chan_map_elem() {
        // corpus 05: fn sink(results <-chan map[string]int, done chan<- bool)
        let f = parse_src("fn sink(results <-chan map[string]int, done chan<- bool) { }");
        let fn_decl = f.fns().next().unwrap();
        match &fn_decl.params[0].ty {
            TypeExpr::Chan {
                dir: ChanDir::Recv,
                elem,
                ..
            } => {
                assert!(matches!(&**elem, TypeExpr::Map { .. }));
            }
            _ => panic!("<-chan map"),
        }
    }

    #[test]
    fn struct_literal_with_elided_type() {
        // corpus 08: "frontend": {replicas: 2, tag: "prod"}
        let f = parse_src(
            "fn main() { e := env{services: map[string]service{\n\"frontend\": {replicas: 2, tag: \"prod\"},\n}} }",
        );
        let stmt = first_stmt(&f);
        let Stmt::Local(LocalDecl { values, .. }) = stmt else {
            panic!("local")
        };
        let Expr::StructLit { fields, .. } = &values[0] else {
            panic!("env lit")
        };
        let Expr::MapLit { entries, .. } = &fields[0].value else {
            panic!("map lit")
        };
        let Expr::StructLit {
            ty: None,
            fields: in_fields,
            ..
        } = &entries[0].1
        else {
            panic!("elided struct literal in map value")
        };
        assert_eq!(in_fields.len(), 2);
        assert_eq!(in_fields[0].name, "replicas");
    }

    #[test]
    fn addr_of_field_index_shape() {
        // corpus 08/09: n := &g.nodes[from] / svc := &e.services[existing]
        let f = parse_src("fn main() { svc := &e.services[existing] }");
        let stmt = first_stmt(&f);
        let Stmt::Local(LocalDecl { values, .. }) = stmt else {
            panic!("local")
        };
        let Expr::AddrOf { operand, .. } = &values[0] else {
            panic!("addr of")
        };
        let Expr::Index { base, index, .. } = &**operand else {
            panic!("index")
        };
        assert!(matches!(&**base, Expr::Field { name, .. } if name == "services"));
        assert!(matches!(&**index, Expr::Ident { name, .. } if name == "existing"));
    }

    #[test]
    fn multi_value_define() {
        // corpus 04: p, f := stats.processed, stats.failures
        let f = parse_src("fn main() { p, f := stats.processed, stats.failures }");
        let stmt = first_stmt(&f);
        let Stmt::Local(LocalDecl {
            kind: LocalKind::Define,
            names,
            values,
            ..
        }) = stmt
        else {
            panic!("define")
        };
        assert_eq!(names, &vec!["p".to_string(), "f".to_string()]);
        assert_eq!(values.len(), 2);
    }

    #[test]
    fn define_from_multi_result_call() {
        // corpus 10: code, body := httptest.get(mux, "/ping")?
        let f = parse_src("fn main() { code, body := httptest.get(mux, \"/ping\")? }");
        let stmt = first_stmt(&f);
        let Stmt::Local(LocalDecl { names, values, .. }) = stmt else {
            panic!("define")
        };
        assert_eq!(names.len(), 2);
        assert!(matches!(&values[0], Expr::Try { .. }));
    }

    #[test]
    fn if_with_init() {
        // corpus 07: if v, ok := c.entries[key]; ok { }
        let f = parse_src("fn main() { if v, ok := c.entries[key]; ok { return } }");
        let stmt = first_stmt(&f);
        let Stmt::If { init, cond, .. } = stmt else {
            panic!("if")
        };
        let init = init.as_ref().unwrap();
        let Stmt::Local(LocalDecl {
            kind: LocalKind::Define,
            names,
            values,
            ..
        }) = &**init
        else {
            panic!("init define")
        };
        assert_eq!(names, &vec!["v".to_string(), "ok".to_string()]);
        assert!(matches!(&values[0], Expr::Index { .. }));
        assert!(matches!(cond, Expr::Ident { name, .. } if name == "ok"));
    }

    #[test]
    fn if_bare_ident_cond_is_not_struct_lit() {
        // THE load-bearing ambiguity: `if ok {` — `{` starts the block
        let f = parse_src("fn main() { if ok { x := 1 } }");
        let stmt = first_stmt(&f);
        let Stmt::If { cond, then, .. } = stmt else {
            panic!("if")
        };
        assert!(matches!(cond, Expr::Ident { name, .. } if name == "ok"));
        assert_eq!(then.stmts.len(), 1);
    }

    #[test]
    fn for_forms() {
        let src = "fn main() {
            for {}
            for cond {}
            for i in 0..n {}
            for w in shard {}
            for _, url in seed_urls {}
            for i in 1..=8 {}
        }";
        let f = parse_src(src);
        let body = &f.fns().next().unwrap().body.as_ref().unwrap().stmts;
        assert!(matches!(
            &body[0],
            Stmt::For {
                kind: ForKind::Infinite,
                ..
            }
        ));
        assert!(matches!(
            &body[1],
            Stmt::For {
                kind: ForKind::While { .. },
                ..
            }
        ));
        assert!(
            matches!(&body[2], Stmt::For { kind: ForKind::Range { var, .. }, .. } if var == "i")
        );
        assert!(
            matches!(&body[3], Stmt::For { kind: ForKind::Iterate { vars, .. }, .. } if vars.len() == 1)
        );
        assert!(
            matches!(&body[4], Stmt::For { kind: ForKind::Iterate { vars, .. }, .. } if vars == &vec!["_".to_string(), "url".to_string()])
        );
        assert!(matches!(
            &body[5],
            Stmt::For {
                kind: ForKind::Range { .. },
                ..
            }
        ));
    }

    #[test]
    fn incdec_and_compound_assign_places() {
        let src = "fn main() {
            agg[w]++
            stats.processed++
            count += 1
            m2[\"total\"] = len(m2)
            rows = append(rows, row{key: \"k\", val: 1})
        }";
        let f = parse_src(src);
        let body = &f.fns().next().unwrap().body.as_ref().unwrap().stmts;
        assert!(matches!(
            &body[0],
            Stmt::Assign { op: AssignOp::Inc, targets, values, .. }
                if values.is_empty() && matches!(&targets[0].kind, PlaceKind::Index { .. })
        ));
        assert!(matches!(
            &body[1],
            Stmt::Assign { op: AssignOp::Inc, targets, .. }
                if matches!(&targets[0].kind, PlaceKind::Field { name, .. } if name == "processed")
        ));
        assert!(matches!(
            &body[2],
            Stmt::Assign {
                op: AssignOp::AddAssign,
                ..
            }
        ));
        assert!(matches!(
            &body[3],
            Stmt::Assign {
                op: AssignOp::Assign,
                ..
            }
        ));
        assert!(matches!(
            &body[4],
            Stmt::Assign { targets, values, .. }
                if matches!(&targets[0].kind, PlaceKind::Ident(n) if n == "rows")
                    && matches!(&values[0], Expr::Call { .. })
        ));
    }

    #[test]
    fn bare_receive_expression_statement() {
        // corpus 01: for _ in 0..n_shards { <-done }
        let f = parse_src("fn main() { for _ in 0..n_shards { <-done } }");
        let stmt = first_stmt(&f);
        let Stmt::For {
            kind: ForKind::Range { var, .. },
            body,
            ..
        } = stmt
        else {
            panic!("for")
        };
        assert_eq!(var, "_");
        assert!(matches!(
            &body.stmts[0],
            Stmt::Expr { expr: Expr::Receive { chan, .. }, .. }
                if matches!(&**chan, Expr::Ident { name, .. } if name == "done")
        ));
    }

    #[test]
    fn define_from_receive() {
        // corpus 15: shard_counts := <-results
        let f = parse_src("fn main() { shard_counts := <-results }");
        let stmt = first_stmt(&f);
        let Stmt::Local(LocalDecl { values, .. }) = stmt else {
            panic!("local")
        };
        assert!(matches!(&values[0], Expr::Receive { .. }));
    }

    #[test]
    fn task_spawn_with_move_arg() {
        // corpus 01: task tally(move chunk, counts, done)
        let f = parse_src("fn main() { task tally(move chunk, counts, done) }");
        let stmt = first_stmt(&f);
        let Stmt::Task { callee, args, .. } = stmt else {
            panic!("task")
        };
        assert!(matches!(callee, Expr::Ident { name, .. } if name == "tally"));
        assert_eq!(args.len(), 3);
        assert!(matches!(&args[0].value, Expr::Move { operand, .. }
            if matches!(&**operand, Expr::Ident { name, .. } if name == "chunk")));
    }

    #[test]
    fn method_and_assoc_fn_decls() {
        // spike-6 01: fn greeting.build(...) / fn (g *greeting) bump()
        let f = parse_src(
            "fn greeting.build(base string) greeting { return greeting{} }\nfn (g *greeting) bump() { }\nfn (g greeting) render() string { return \"\" }",
        );
        let fns: Vec<&FnDecl> = f.fns().collect();
        assert_eq!(fns.len(), 3);
        assert_eq!(fns[0].name, "greeting.build");
        assert!(fns[0].receiver.is_none());
        let r = fns[1].receiver.as_ref().unwrap();
        assert!(r.is_pointer);
        assert_eq!(r.name, "g");
        assert!(matches!(&r.ty, TypeExpr::Named { name, .. } if name == "greeting"));
        let r2 = fns[2].receiver.as_ref().unwrap();
        assert!(!r2.is_pointer);
    }

    #[test]
    fn named_results() {
        // spike-6 02: fn copy_stream(r io.reader, w io.writer) (total int, err error)
        //             fn main() (code int)
        let f = parse_src("fn copy_stream(r io.reader, w io.writer) (total int, err error) { }\nfn main() (code int) { }");
        let fns: Vec<&FnDecl> = f.fns().collect();
        assert_eq!(fns[0].results.len(), 2);
        assert_eq!(fns[0].results[0].name.as_deref(), Some("total"));
        assert_eq!(fns[0].results[1].name.as_deref(), Some("err"));
        assert_eq!(fns[1].results.len(), 1);
        assert_eq!(fns[1].results[0].name.as_deref(), Some("code"));
    }

    #[test]
    fn fn_literal_typed_and_untyped_params() {
        // corpus 10 / spike-6 06: http.handler(fn(w http.response_writer, r *http.request) { … })
        //                         http.handler(fn(w, r) { … })
        let f = parse_src("fn main() { mux.handle(\"/p\", http.handler(fn(w http.response_writer, r *http.request) { })) }");
        let stmt = first_stmt(&f);
        let Stmt::Expr {
            expr: Expr::Call { args, .. },
            ..
        } = stmt
        else {
            panic!("call")
        };
        // http.handler(fn...) — arg is the handler call
        let Expr::Call { args: hargs, .. } = &args[1].value else {
            panic!("handler call")
        };
        let Expr::FnLit { params, .. } = &hargs[0].value else {
            panic!("fn lit")
        };
        assert_eq!(params.len(), 2);
        assert!(
            matches!(&params[0].ty, TypeExpr::Named { name, .. } if name == "http.response_writer")
        );
        assert!(matches!(&params[1].ty, TypeExpr::Pointer { .. }));

        let f2 = parse_src("fn main() { mux.handle(\"/p\", http.handler(fn(w, r) { })) }");
        let stmt = first_stmt(&f2);
        let Stmt::Expr {
            expr: Expr::Call { args, .. },
            ..
        } = stmt
        else {
            panic!("call")
        };
        let Expr::Call { args: hargs, .. } = &args[1].value else {
            panic!("handler call")
        };
        let Expr::FnLit { params, .. } = &hargs[0].value else {
            panic!("fn lit")
        };
        assert_eq!(params.len(), 2);
        // inferred types: Named { name: "" } (contract-gap encoding)
        assert!(matches!(&params[0].ty, TypeExpr::Named { name, .. } if name.is_empty()));
    }

    #[test]
    fn generics_fold_into_names() {
        // spike-6 07 surface
        let f = parse_src(
            "struct lru_cache[k comparable, v any] {\n cap int\n}\nfn lru_cache.new[k comparable, v any](cap int) lru_cache[k, v] { return lru_cache[k, v]{cap: cap} }\nfn (c *lru_cache[k, v]) get(key k) option[v] { return none }",
        );
        let fns: Vec<&FnDecl> = f.fns().collect();
        assert_eq!(fns[0].name, "lru_cache.new");
        assert_eq!(fns[0].type_params.len(), 2);
        assert_eq!(
            fns[0].type_params[0].constraint.as_deref(),
            Some("comparable")
        );
        match &fns[0].results[0].ty {
            TypeExpr::Named { name, .. } => assert_eq!(name, "lru_cache[k, v]"),
            t => panic!("folded result type, got {:?}", t.render()),
        }
        let r = fns[1].receiver.as_ref().unwrap();
        assert!(r.is_pointer);
        assert!(matches!(&r.ty, TypeExpr::Named { name, .. } if name == "lru_cache[k, v]"));
        assert!(matches!(&fns[1].results[0].ty, TypeExpr::Option { .. }));

        // body: return lru_cache[k, v]{cap: cap}
        let body = fns[0].body.as_ref().unwrap();
        let Stmt::Return { values, .. } = &body.stmts[0] else {
            panic!("return")
        };
        match &values[0] {
            Expr::StructLit {
                ty: Some(TypeExpr::Named { name, .. }),
                fields,
                ..
            } => {
                assert_eq!(name, "lru_cache[k, v]");
                assert_eq!(fields.len(), 1);
            }
            e => panic!("generic struct lit, got {}", e.render()),
        }
    }

    #[test]
    fn generic_call_callee_folds_type_args() {
        // spike-6 07: c := lru_cache.new[string, int](2)
        let f = parse_src("fn main() { c := lru_cache.new[string, int](2) }");
        let stmt = first_stmt(&f);
        let Stmt::Local(LocalDecl { values, .. }) = stmt else {
            panic!("local")
        };
        match &values[0] {
            Expr::Call { callee, .. } => {
                assert!(
                    matches!(callee.as_ref(), Expr::Field { name, .. } if name == "new[string, int]"),
                    "callee: {}",
                    callee.render()
                );
            }
            e => panic!("call, got {}", e.render()),
        }
    }

    #[test]
    fn map_and_slice_literals() {
        let f = parse_src(
            "fn main() { counts := map[string]int{} got := []string{} xs := [10, 50, 250, 1000] }",
        );
        let body = &f.fns().next().unwrap().body.as_ref().unwrap().stmts;
        assert!(
            matches!(&body[0], Stmt::Local(LocalDecl { values, .. }) if matches!(&values[0], Expr::MapLit { entries, .. } if entries.is_empty()))
        );
        assert!(
            matches!(&body[1], Stmt::Local(LocalDecl { values, .. }) if matches!(&values[0], Expr::SliceLit { elem: Some(_), items, .. } if items.is_empty()))
        );
        assert!(
            matches!(&body[2], Stmt::Local(LocalDecl { values, .. }) if matches!(&values[0], Expr::SliceLit { elem: None, items, .. } if items.len() == 4))
        );
    }

    #[test]
    fn variadic_spread_argument() {
        // corpus 01: append([]string{}, words[a:b]...)
        let f = parse_src("fn main() { return append([]string{}, words[a:b]...) }");
        let stmt = first_stmt(&f);
        let Stmt::Return { values, .. } = stmt else {
            panic!("return")
        };
        let Expr::Call { args, .. } = &values[0] else {
            panic!("call")
        };
        assert!(args[1].spread);
        assert!(matches!(&args[1].value, Expr::Slice { .. }));
        assert!(!args[0].spread);
    }

    #[test]
    fn make_type_argument_encoding() {
        // corpus: make(chan bool, n_shards) / make([]byte, 64*1024)
        let f =
            parse_src("fn main() { done := make(chan bool, n) buf := make([]byte, 64 * 1024) }");
        let body = &f.fns().next().unwrap().body.as_ref().unwrap().stmts;
        match &body[0] {
            Stmt::Local(LocalDecl { values, .. }) => {
                let Expr::Call { callee, args, .. } = &values[0] else {
                    panic!("call")
                };
                assert!(matches!(callee.as_ref(), Expr::Ident { name, .. } if name == "make"));
                match &args[0].value {
                    Expr::StructLit {
                        ty:
                            Some(TypeExpr::Chan {
                                dir: ChanDir::Both, ..
                            }),
                        fields,
                        ..
                    } => {
                        assert!(fields.is_empty());
                    }
                    e => panic!("chan type-as-value, got {}", e.render()),
                }
            }
            _ => panic!("local"),
        }
        match &body[1] {
            Stmt::Local(LocalDecl { values, .. }) => {
                let Expr::Call { args, .. } = &values[0] else {
                    panic!("call")
                };
                assert!(
                    matches!(&args[0].value, Expr::SliceLit { elem: Some(_), items, .. } if items.is_empty())
                );
            }
            _ => panic!("local"),
        }
    }

    #[test]
    fn named_call_argument_name_is_dropped() {
        // spike-6 10: sha3.stream_checksum(1_000_000, seed=7)?
        // (contract gap: CallArg has no name field — value kept, name dropped)
        let f = parse_src("fn main() { checksum := sha3.stream_checksum(1_000_000, seed=7)? }");
        let stmt = first_stmt(&f);
        let Stmt::Local(LocalDecl { values, .. }) = stmt else {
            panic!("local")
        };
        let Expr::Try { operand, .. } = &values[0] else {
            panic!("try")
        };
        let Expr::Call { args, .. } = operand.as_ref() else {
            panic!("call")
        };
        assert_eq!(args.len(), 2);
        assert!(matches!(&args[1].value, Expr::Lit { kind: LitKind::Int(s), .. } if s == "7"));
    }

    #[test]
    fn type_alias_representable() {
        // spike-6 08: type node_id = int — a REAL alias since the emitter
        // batch (the zero-field-struct approximation is closed; example 08
        // needs `node_id(len(...))` to be an int conversion).
        let f = parse_src("type node_id = int\nfn main() { }");
        assert_eq!(f.decls.len(), 2);
        match &f.decls[0] {
            Decl::TypeAlias(TypeAliasDecl { name, ty, .. }) => {
                assert_eq!(name, "node_id");
                assert!(matches!(ty, TypeExpr::Named { name, .. } if name == "int"));
            }
            d => panic!("type alias, got {:?}", d),
        }
    }

    #[test]
    fn set_literal_approximation() {
        // spike-6 08: seen := set[node_id]{}
        let f = parse_src("fn main() { seen := set[node_id]{} }");
        let stmt = first_stmt(&f);
        let Stmt::Local(LocalDecl { values, .. }) = stmt else {
            panic!("local")
        };
        match &values[0] {
            Expr::StructLit {
                ty: Some(TypeExpr::Named { name, .. }),
                fields,
                ..
            } => {
                assert_eq!(name, "set[node_id]");
                assert!(fields.is_empty());
            }
            e => panic!("set lit approx, got {}", e.render()),
        }
    }

    #[test]
    fn select_default_braced_arm() {
        let f = parse_src("fn main() { select { default: { fmt.println(\"idle\") } } }");
        let Stmt::Select { arms, .. } = first_stmt(&f) else {
            panic!("select")
        };
        assert_eq!(arms.len(), 1);
        assert!(matches!(arms[0].kind, SelectArmKind::Default));
        assert_eq!(arms[0].body.stmts.len(), 1);
    }

    #[test]
    fn select_default_ends_unbraced_send_body() {
        let f = parse_src(
            "fn main() { select { ch <- 1: fmt.println(\"sent\") default: fmt.println(\"idle\") } }",
        );
        let Stmt::Select { arms, .. } = first_stmt(&f) else {
            panic!("select")
        };
        assert_eq!(arms.len(), 2);
        assert!(matches!(arms[0].kind, SelectArmKind::Send { .. }));
        assert!(matches!(arms[1].kind, SelectArmKind::Default));
        assert_eq!(arms[0].body.stmts.len(), 1);
        assert_eq!(arms[1].body.stmts.len(), 1);
    }

    #[test]
    fn select_default_arrow_before_receive() {
        let f = parse_src("fn main() { select { default => { fmt.println(\"idle\") } <-ch: {} } }");
        let Stmt::Select { arms, .. } = first_stmt(&f) else {
            panic!("select")
        };
        assert_eq!(arms.len(), 2);
        assert!(matches!(arms[0].kind, SelectArmKind::Default));
        assert!(matches!(arms[1].kind, SelectArmKind::Recv { .. }));
    }

    #[test]
    fn select_send_carries_the_move_marker() {
        // C5: a select send clause is a sendability check-site exactly like a
        // statement send, so the parser's `ch <- move v` marker must survive
        // the statement→arm conversion. Dropping it made the class-3
        // owned-move route unreachable inside select arms.
        let f = parse_src("fn main() { select { ch <- move m: {} } }");
        let Stmt::Select { arms, .. } = first_stmt(&f) else {
            panic!("select")
        };
        let SelectArmKind::Send { value, is_move, .. } = &arms[0].kind else {
            panic!("send arm")
        };
        assert!(*is_move, "the move marker must survive the select conversion");
        assert!(
            matches!(value, Expr::Ident { name, .. } if name == "m"),
            "the arm value is the plain moved ident, got {}",
            value.render()
        );
    }

    #[test]
    fn select_plain_send_is_not_a_move() {
        // Negative control for the marker: a send without `move` stays false,
        // so the arm cannot accidentally claim ownership transfer.
        let f = parse_src("fn main() { select { ch <- m: {} } }");
        let Stmt::Select { arms, .. } = first_stmt(&f) else {
            panic!("select")
        };
        let SelectArmKind::Send { is_move, .. } = &arms[0].kind else {
            panic!("send arm")
        };
        assert!(!*is_move);
    }

    #[test]
    fn select_default_identifier_is_not_an_arm_without_separator() {
        // LX3 does not reserve `default`: preserve the ordinary channel
        // name and the local assignment inside the unbraced send body.
        let f =
            parse_src("fn main() { select { default <- 1: default := 2 fmt.println(default) } }");
        let Stmt::Select { arms, .. } = first_stmt(&f) else {
            panic!("select")
        };
        assert_eq!(arms.len(), 1);
        assert!(
            matches!(&arms[0].kind, SelectArmKind::Send { chan: Expr::Ident { name, .. }, .. } if name == "default")
        );
        assert_eq!(arms[0].body.stmts.len(), 2);
        assert!(matches!(arms[0].body.stmts[0], Stmt::Local(_)));
    }

    #[test]
    fn select_default_requires_an_arm_separator() {
        assert!(parse_file("t.hy", "fn main() { select { default {} } }").is_err());
    }

    #[test]
    fn select_default_after_unbraced_receive_with_default_local() {
        let f = parse_src(
            "fn main() { select { v := <-ch: default := v fmt.println(default) default: {} } }",
        );
        let Stmt::Select { arms, .. } = first_stmt(&f) else {
            panic!("select")
        };
        assert_eq!(arms.len(), 2);
        assert!(matches!(arms[0].kind, SelectArmKind::Recv { .. }));
        assert_eq!(arms[0].body.stmts.len(), 2);
        assert!(matches!(arms[0].body.stmts[0], Stmt::Local(_)));
        assert!(matches!(arms[1].kind, SelectArmKind::Default));
        assert!(arms[1].body.stmts.is_empty());
    }

    #[test]
    fn select_with_recv_define_and_bare_recv_arms() {
        // spike-6 04 select shape
        let src = "fn main() {
            loop {
                select {
                    v, ok := <-sq:
                        if !ok { break loop }
                        total += v
                    <-deadline:
                        panic \"pipeline timed out\"
                }
            }
        }";
        let f = parse_src(src);
        let body = &f.fns().next().unwrap().body.as_ref().unwrap().stmts;
        let Stmt::For {
            kind: ForKind::Infinite,
            body: loop_body,
            ..
        } = &body[0]
        else {
            panic!("loop")
        };
        let Stmt::Select { arms, .. } = &loop_body.stmts[0] else {
            panic!("select")
        };
        assert_eq!(arms.len(), 2);
        match &arms[0].kind {
            SelectArmKind::Recv { names, chan } => {
                assert_eq!(names, &vec!["v".to_string(), "ok".to_string()]);
                assert!(matches!(chan, Expr::Ident { name, .. } if name == "sq"));
            }
            k => panic!("recv arm, got {:?}", k),
        }
        assert_eq!(arms[0].body.stmts.len(), 2); // if + total += v
        assert!(matches!(&arms[0].body.stmts[0], Stmt::If { .. }));
        assert!(matches!(&arms[0].body.stmts[1], Stmt::Assign { .. }));
        match &arms[1].kind {
            SelectArmKind::Recv { names, chan } => {
                assert!(names.is_empty());
                assert!(matches!(chan, Expr::Ident { name, .. } if name == "deadline"));
            }
            k => panic!("bare recv arm, got {:?}", k),
        }
        // panic "…" arm body → Call{Ident("panic")} expression statement
        match &arms[1].body.stmts[0] {
            Stmt::Expr {
                expr: Expr::Call { callee, .. },
                ..
            } => {
                assert!(matches!(callee.as_ref(), Expr::Ident { name, .. } if name == "panic"));
            }
            s => panic!("panic call, got {:?}", s),
        }
    }

    #[test]
    fn match_arms_with_colon_separator() {
        // spike-6 07: match c.get("b") { some(v): … none: … }
        let src = "fn main() {
            match c.get(\"b\") {
                some(v): fmt.println(\"b={v}\")
                none:     fmt.println(\"b evicted\")
            }
        }";
        let f = parse_src(src);
        let stmt = first_stmt(&f);
        let Stmt::Match { subject, arms, .. } = stmt else {
            panic!("match")
        };
        assert!(matches!(subject, Expr::Call { .. }));
        assert_eq!(arms.len(), 2);
        match &arms[0].pattern {
            MatchPattern::Path { segments, binding } => {
                assert_eq!(segments, &vec!["some".to_string()]);
                assert_eq!(binding.as_deref(), Some("v"));
            }
            p => panic!("some(v) pattern, got {:?}", p),
        }
        match &arms[1].pattern {
            MatchPattern::Path { segments, binding } => {
                assert_eq!(segments, &vec!["none".to_string()]);
                assert!(binding.is_none());
            }
            p => panic!("none pattern, got {:?}", p),
        }
        assert!(matches!(&arms[0].body.stmts[0], Stmt::Expr { .. }));
    }

    #[test]
    fn match_accepts_fat_arrow_separator() {
        let f = parse_src("fn main() { match x { some(v) => { a := 1 } _ => { } } }");
        let stmt = first_stmt(&f);
        let Stmt::Match { arms, .. } = stmt else {
            panic!("match")
        };
        assert_eq!(arms.len(), 2);
        assert!(matches!(arms[1].pattern, MatchPattern::Wildcard));
    }

    #[test]
    fn precedence() {
        // 1 + 2 * 3 == 7 && true  →  And( Eq( Add(1, Mul(2,3)), 7 ), true )
        let f = parse_src("fn main() { x := 1 + 2 * 3 == 7 && true }");
        let stmt = first_stmt(&f);
        let Stmt::Local(LocalDecl { values, .. }) = stmt else {
            panic!("local")
        };
        let Expr::Binary {
            op: BinOp::And,
            lhs,
            rhs,
            ..
        } = &values[0]
        else {
            panic!("and")
        };
        assert!(matches!(
            rhs.as_ref(),
            Expr::Lit {
                kind: LitKind::Bool(true),
                ..
            }
        ));
        let Expr::Binary {
            op: BinOp::Eq,
            lhs: eq_lhs,
            rhs: eq_rhs,
            ..
        } = lhs.as_ref()
        else {
            panic!("eq")
        };
        assert!(matches!(eq_rhs.as_ref(), Expr::Lit { kind: LitKind::Int(s), .. } if s == "7"));
        let Expr::Binary {
            op: BinOp::Add,
            rhs: add_rhs,
            ..
        } = eq_lhs.as_ref()
        else {
            panic!("add")
        };
        assert!(matches!(
            add_rhs.as_ref(),
            Expr::Binary { op: BinOp::Mul, .. }
        ));
    }

    #[test]
    fn bit_ops_parse_per_spec_ebnf() {
        // SPEC-LANGUAGE 1.4: AddOp = + - | ^, MulOp = * / % << >> &.
        // The benchmark community report (v0.1.0) hit `^` and the shifts
        // rejected at parse despite the EBNF; each op must parse.
        for (src, want) in [
            ("x := a | b", "BitOr"),
            ("x := a ^ b", "BitXor"),
            ("x := a & b", "BitAnd"),
            ("x := a << b", "Shl"),
            ("x := a >> b", "Shr"),
        ] {
            let f = parse_src(&format!("fn main() {{ {} }}", src));
            let stmt = first_stmt(&f);
            let Stmt::Local(LocalDecl { values, .. }) = stmt else {
                panic!("local for {}", src)
            };
            let Expr::Binary { op, .. } = &values[0] else {
                panic!("binary for {}", src)
            };
            assert_eq!(format!("{:?}", op), want, "src: {}", src);
        }
    }

    #[test]
    fn bit_ops_precedence_matches_go() {
        // `1 | 2 ^ 3 & 4` → BitXor( BitOr(1, 2), BitAnd(3, 4) ):
        // SPEC-LANGUAGE 1.4 puts `|` and `^` on the SAME AddOp level
        // (left-assoc, exactly like Go), while `&` is MulOp level.
        let f = parse_src("fn main() { x := 1 | 2 ^ 3 & 4 }");
        let stmt = first_stmt(&f);
        let Stmt::Local(LocalDecl { values, .. }) = stmt else {
            panic!("local")
        };
        let Expr::Binary {
            op: BinOp::BitXor,
            lhs,
            rhs,
            ..
        } = &values[0]
        else {
            panic!("bitxor at top, got {:?}", values[0])
        };
        assert!(
            matches!(
                lhs.as_ref(),
                Expr::Binary {
                    op: BinOp::BitOr,
                    ..
                }
            ),
            "left of ^ is 1|2, got {:?}",
            lhs
        );
        assert!(
            matches!(
                rhs.as_ref(),
                Expr::Binary {
                    op: BinOp::BitAnd,
                    ..
                }
            ),
            "right of ^ is 3&4, got {:?}",
            rhs
        );
    }

    #[test]
    fn compound_bit_assigns_parse() {
        // SPEC-LANGUAGE 1.4: AssignOp = = += -= |= &= ^= <<= >>=.
        for (src, want) in [
            ("x |= b", "BitOrAssign"),
            ("x &= b", "BitAndAssign"),
            ("x ^= b", "BitXorAssign"),
            ("x <<= b", "ShlAssign"),
            ("x >>= b", "ShrAssign"),
        ] {
            let f = parse_src(&format!("fn main() {{ {} }}", src));
            let stmt = first_stmt(&f);
            let Stmt::Assign { op, .. } = stmt else {
                panic!("assign for {}", src)
            };
            assert_eq!(format!("{:?}", op), want, "src: {}", src);
        }
    }

    #[test]
    fn mut_is_reserved_not_silently_emitted() {
        // `mut x := 3` used to parse (mut as a plain identifier) and emit the
        // word `mut` into Go — a broken build from accepted source. Now it is
        // a clean parse error naming the keyword (SPEC-LANGUAGE LX3, T22).
        let errs = match parse_file("t.hy", "fn main() { mut x := 3 }") {
            Ok(_) => panic!("mut must fail at parse, not emit broken Go"),
            Err(d) => d,
        };
        assert!(
            errs.iter().any(|d| d.message.contains("`mut`")),
            "error should name the keyword: {:?}",
            errs
        );
    }

    #[test]
    fn while_is_reserved_not_silently_emitted() {
        // `while x > 0 { … }` used to parse (while as a plain identifier +
        // cond expr + orphan block) and emit the word `while` into Go —
        // `go vet: undefined: while`, a broken build from accepted source
        // (issue #103). Hybrid's while-form is `for cond { }`. Now `while`
        // is a clean parse error naming the keyword, same as `mut`.
        let src = "fn main() { while x > 0 { x += 1 } }";
        let errs = match parse_file("t.hy", src) {
            Ok(_) => panic!("while must fail at parse, not emit broken Go"),
            Err(d) => d,
        };
        assert!(
            errs.iter().any(|d| d.message.contains("`while`")),
            "error should name the keyword: {:?}",
            errs
        );
    }

    #[test]
    fn go_keyword_traps_fail_at_parse_naming_keyword() {
        // issue #228: `switch`/`range`/`goto`/`fallthrough` parsed as
        // plain identifiers and were emitted verbatim into Go
        // (`x := switch` → go: `expected operand, found 'switch'`;
        // `fallthrough` → `fallthrough statement out of place`). Now they
        // fail at parse naming the word — the `while` (#103) move.
        for w in ["switch", "range", "goto", "fallthrough"] {
            let src = format!("fn main() {{ x := {} }}", w);
            let errs = match parse_file("t.hy", &src) {
                Ok(_) => panic!("`{}` must fail at parse, not emit broken Go", w),
                Err(d) => d,
            };
            assert!(
                errs.iter().any(|d| d.message.contains(&format!("`{}`", w))),
                "error should name `{}`: {:?}",
                w,
                errs
            );
        }
    }

    #[test]
    fn map_prefix_controls_still_parse() {
        // issue #228 controls: the legitimate `map` forms (type, literal,
        // generic map, map through trivia) must keep parsing — only a
        // *bare* `map` is rejected (as a HYB-E0002 lexical diagnostic).
        let ok = [
            "fn main() { m := map[string]int{} _ = m }",
            "fn main() { var x map[int]string _ = x }",
            "fn f(m map[bool]u8) { _ = m }",
            "fn main() { y := map /* c */ [string]int{} _ = y }",
        ];
        for src in ok {
            if let Err(diags) = parse_file("t.hy", src) {
                panic!("map-prefix control must parse: {} -> {:?}", src, diags);
            }
        }
        let errs = match parse_file("t.hy", "fn main() { x := map }") {
            Ok(_) => panic!("bare `map` must fail at parse"),
            Err(d) => d,
        };
        assert!(
            errs.iter().any(|d| d.message.contains("`map`")),
            "error should name `map`: {:?}",
            errs
        );
    }

    #[test]
    fn default_select_arm_parses() {
        // `default` was never in KEYWORDS, so at_kw("default") in
        // parse_select was dead code and a `default:` arm failed to parse
        // ("expected expression, found `:`") — issue #104. Reserving it
        // makes the existing Default arm branch live.
        let src = "\
fn main() {
    ch := make(chan int, 1)
    select {
        v := <-ch:
            fmt.println(v)
        default:
            fmt.println(\"d\")
    }
}
";
        if let Err(diags) = parse_file("t.hy", src) {
            panic!("default arm must parse: {:?}", diags);
        }
    }

    #[test]
    fn second_recv_define_select_arm_parses() {
        // The arm-body terminator `looks_like_select_arm_head` recognized
        // send arms (`ch <- v:`), bare recv arms (`<-ch:`), comma-ok
        // recv-define arms (`v, ok := <-ch:` — and even that one with an
        // off-by-one that made the final token check look for an Ident on
        // `:=`), but NOT the single-name recv-define arm (`v := <-ch:`).
        // Consequence: any select whose SECOND or later arm was a named
        // recv-define died with "expected expression, found `:`" — the
        // body loop swallowed the arm head as a local decl and choked on
        // the trailing colon. Corpus 24 only ever paired one named arm
        // with a bare arm, so the gap had zero coverage.
        let two_named = "\
fn main() {
    a := make(chan int, 1)
    b := make(chan int, 1)
    select {
        v := <-a:
            fmt.println(v)
        w := <-b:
            fmt.println(w)
    }
}
";
        if let Err(diags) = parse_file("t.hy", two_named) {
            panic!("two recv-define arms must parse: {:?}", diags);
        }
        let two_comma_ok = "\
fn main() {
    a := make(chan int, 1)
    b := make(chan int, 1)
    select {
        v, ok := <-a:
            fmt.println(v, ok)
        w, ok2 := <-b:
            fmt.println(w, ok2)
    }
}
";
        if let Err(diags) = parse_file("t.hy", two_comma_ok) {
            panic!("two comma-ok recv-define arms must parse: {:?}", diags);
        }
        let named_then_comma_ok = "\
fn main() {
    a := make(chan int, 1)
    b := make(chan int, 1)
    select {
        v := <-a:
            fmt.println(v)
        w, ok := <-b:
            fmt.println(w, ok)
    }
}
";
        if let Err(diags) = parse_file("t.hy", named_then_comma_ok) {
            panic!("named + comma-ok recv-define arms must parse: {:?}", diags);
        }
    }

    #[test]
    fn case_select_arm_is_reserved_not_silently_fused() {
        // Go-style `case <-ch:` used to parse: `case` was an unreserved
        // ident, the same-line `<-` fusion turned `case <-ch` into a Send
        // on ident `case`, and the emitter wrote `case case <- ch:`
        // (invalid Go) — issue #104. Now `case` is a keyword and the arm
        // fails cleanly at parse.
        let src = "\
fn main() {
    ch := make(chan int)
    select {
        case <-ch:
            return
    }
}
";
        let errs = match parse_file("t.hy", src) {
            Ok(_) => panic!("case arm must fail at parse, not emit broken Go"),
            Err(d) => d,
        };
        assert!(
            errs.iter().any(|d| d.message.contains("`case`")),

            "error should name the keyword: {:?}",
            errs
        );
    }

    #[test]
    fn interp_field_chain_and_calls() {
        // corpus 03: "processed={stats.processed} failures={stats.failures}"
        let f = parse_src(
            "fn main() { fmt.println(\"processed={stats.processed} failures={stats.failures}\") }",
        );
        let stmt = first_stmt(&f);
        let Stmt::Expr {
            expr: Expr::Call { args, .. },
            ..
        } = stmt
        else {
            panic!("call")
        };
        let Expr::Interp { parts, .. } = &args[0].value else {
            panic!("interp")
        };
        assert_eq!(parts.len(), 4);
        assert!(
            matches!(&parts[1], InterpPart::Expr(Expr::Field { name, .. }) if name == "processed")
        );
        assert!(matches!(&parts[2], InterpPart::Lit(s) if s == " failures="));
    }

    #[test]
    fn dump_ast_smoke() {
        let f = parse_src("fn main() { x := 1 }");
        let s = dump_ast(&f);
        assert!(s.contains("FnDecl"));
        assert!(s.contains("LocalDecl"));
    }

    #[test]
    fn parse_errors_are_clean() {
        let r = parse_file("t.hy", "fn main() { x := }");
        assert!(r.is_err());
        let d = r.unwrap_err();
        assert_eq!(d[0].code, "HYB-E0002");
        assert!(d[0].message.contains("parse error"));
        assert!(d[0].span.start.line >= 1);

        let r2 = parse_file("t.hy", "fn main() { if x { ");
        assert!(r2.is_err());
    }

    #[test]
    fn imports_and_consts_at_top_level() {
        let f = parse_src(
            "import \"rust/serde_json\"\nconst min_logins = 3\nconst payload = \"a\" + \"b\"\nfn main() { }",
        );
        assert_eq!(f.decls.len(), 4);
        assert!(matches!(&f.decls[0], Decl::Import { path, .. } if path == "rust/serde_json"));
        assert!(matches!(&f.decls[1], Decl::Const(ConstDecl { name, .. }) if name == "min_logins"));
        match &f.decls[2] {
            Decl::Const(ConstDecl {
                value: Expr::Binary { op: BinOp::Add, .. },
                ..
            }) => {}
            d => panic!("concat const, got {:?}", d),
        }
    }

    #[test]
    fn local_const_inside_fn() {
        // spike-6 03: fn-local `const n_workers = 4`
        let f = parse_src("fn main() { const n_workers = 4 x := n_workers }");
        let body = &f.fns().next().unwrap().body.as_ref().unwrap().stmts;
        assert!(matches!(
            &body[0],
            Stmt::Local(LocalDecl { kind: LocalKind::Const, names, .. })
                if names == &vec!["n_workers".to_string()]
        ));
    }

    #[test]
    fn var_with_type_and_initializer() {
        let f = parse_src("fn main() { var total int var got []int var hits atomic.int64 }");
        let body = &f.fns().next().unwrap().body.as_ref().unwrap().stmts;
        for s in body {
            assert!(
                matches!(s, Stmt::Local(LocalDecl { kind: LocalKind::Var, ty: Some(_), values, .. }) if values.is_empty()),
                "var decl shape"
            );
        }
        match &body[1] {
            Stmt::Local(LocalDecl {
                ty: Some(TypeExpr::Slice { .. }),
                ..
            }) => {}
            _ => panic!("[]int var"),
        }
        match &body[2] {
            Stmt::Local(LocalDecl {
                ty: Some(TypeExpr::Named { name, .. }),
                ..
            }) if true => {}
            _ => panic!("atomic.int64 var"),
        }
    }

    #[test]
    fn defer_call() {
        // spike-6 02: defer f.close()
        let f = parse_src("fn main() { defer f.close() }");
        let stmt = first_stmt(&f);
        let Stmt::Defer { call, .. } = stmt else {
            panic!("defer")
        };
        assert!(matches!(call, Expr::Call { .. }));
    }

    #[test]
    fn else_if_chain() {
        let f = parse_src("fn main() { if a { } else if b { } else { } }");
        let stmt = first_stmt(&f);
        let Stmt::If { otherwise, .. } = stmt else {
            panic!("if")
        };
        let otherwise = otherwise.as_ref().unwrap();
        assert!(matches!(otherwise.as_ref(), Stmt::If { .. }));
        let Stmt::If {
            otherwise: inner, ..
        } = otherwise.as_ref()
        else {
            unreachable!()
        };
        assert!(matches!(
            inner.as_ref().unwrap().as_ref(),
            Stmt::Block { .. }
        ));
    }

    #[test]
    fn try_postfix_and_call_chains() {
        // corpus 07: for u in doc.array("users")? { if u.int("logins")? == 0 { } }
        let f = parse_src(
            "fn main() { for u in doc.array(\"users\")? { if u.int(\"logins\")? == 0 { } } }",
        );
        let stmt = first_stmt(&f);
        let Stmt::For {
            kind: ForKind::Iterate { vars, expr },
            ..
        } = stmt
        else {
            panic!("for")
        };
        assert_eq!(vars, &vec!["u".to_string()]);
        assert!(matches!(expr, Expr::Try { .. }));
    }

    #[test]
    fn raw_string_argument() {
        // corpus 07: serde_json.parse(`{"users":[…]}`)?
        let f = parse_src("fn main() { doc := serde_json.parse(`{\"a\":1}`)? }");
        let stmt = first_stmt(&f);
        let Stmt::Local(LocalDecl { values, .. }) = stmt else {
            panic!("local")
        };
        let Expr::Try { operand, .. } = &values[0] else {
            panic!("try")
        };
        let Expr::Call { args, .. } = operand.as_ref() else {
            panic!("call")
        };
        assert!(
            matches!(&args[0].value, Expr::Lit { kind: LitKind::RawStr(s), .. } if s.contains("\"a\""))
        );
    }

    #[test]
    fn underscore_blank_assign() {
        // spike-6 07: _ = c.get("a")
        let f = parse_src("fn main() { _ = c.get(\"a\") }");
        let stmt = first_stmt(&f);
        assert!(matches!(
            stmt,
            Stmt::Assign { targets, op: AssignOp::Assign, .. }
                if matches!(&targets[0].kind, PlaceKind::Ident(n) if n == "_")
        ));
    }

    #[test]
    fn if_cond_field_chain_is_not_struct_lit() {
        // corpus 02: if rerr == io.eof { — rhs is a dotted name before `{`
        let f = parse_src("fn main() { if rerr == io.eof { return } }");
        let stmt = first_stmt(&f);
        let Stmt::If { cond, then, .. } = stmt else {
            panic!("if")
        };
        assert!(matches!(
            cond,
            Expr::Binary { op: BinOp::Eq, rhs, .. } if matches!(rhs.as_ref(), Expr::Field { .. })
        ));
        assert_eq!(then.stmts.len(), 1);
    }

    #[test]
    fn for_iterate_over_call_is_not_struct_lit() {
        // corpus 07: for u in doc.array("users")? { … } — covered above; also
        // for e in g.nodes[cur].edges { — index in the chain breaks the
        // type-name-chain rule, so `{` is the body.
        let f = parse_src("fn main() { for e in g.nodes[cur].edges { x := 1 } }");
        let stmt = first_stmt(&f);
        let Stmt::For {
            kind: ForKind::Iterate { expr, .. },
            body,
            ..
        } = stmt
        else {
            panic!("for")
        };
        assert!(matches!(expr, Expr::Field { .. }));
        assert_eq!(body.stmts.len(), 1);
    }

    #[test]
    fn range_bound_ident_is_not_struct_lit() {
        // corpus: for i in 0..n_shards { — hi is a bare ident before `{`
        let f = parse_src("fn main() { for i in 0..n_shards { x := 1 } }");
        let stmt = first_stmt(&f);
        let Stmt::For {
            kind: ForKind::Range { hi, .. },
            body,
            ..
        } = stmt
        else {
            panic!("for")
        };
        assert!(matches!(hi, Expr::Ident { name, .. } if name == "n_shards"));
        assert_eq!(body.stmts.len(), 1);
    }

    #[test]
    fn spikes_http_handler_capture_shape() {
        // corpus 10: closure body with hits++ and fmt.fprintln(w, "pong {hits}")
        let f = parse_src(
            "fn main() { mux.handle(\"/ping\", http.handler(fn(w http.response_writer, r *http.request) {\n hits++\n fmt.fprintln(w, \"pong {hits}\")\n })) }",
        );
        let _ = f; // shape smoke: parses cleanly with a 2-stmt closure body
    }

    #[test]
    fn multi_line_string_concat() {
        // corpus 01: const corpus = "…" +
        //                 "…"      (continuation line; no semicolons in Hybrid)
        let f = parse_src(
            "const corpus = \"the quick brown fox \" +\n    \"jumps over\"\nfn main() { }",
        );
        assert!(matches!(
            &f.decls[0],
            Decl::Const(ConstDecl {
                value: Expr::Binary { op: BinOp::Add, .. },
                ..
            })
        ));
    }

    #[test]
    fn stray_semicolon_is_rejected_fail_closed() {
        // LX9: no semicolons are written; a stray `;` (outside an if-init)
        // is a clean parse error, never a guess.
        let r = parse_file("t.hy", "const corpus = \"a\" + \"b\";\nfn main() { }");
        assert!(r.is_err());
    }

    #[test]
    fn multiple_errors_are_collected() {
        // two broken statements in one block → two diagnostics (cheap
        // recovery: sync to the next statement starter / `}` — the second
        // error only survives because `return` is a statement starter)
        let r = parse_file("t.hy", "fn main() {\n    x := )\n    return )\n}");
        assert!(r.is_err());
        let d = r.unwrap_err();
        assert!(d.len() >= 2, "expected >= 2 diagnostics, got {:#?}", d);
        for diag in &d {
            assert_eq!(diag.code, "HYB-E0002");
            assert!(diag.message.contains("parse error"));
        }
        assert_eq!(d[0].span.start.line, 2);
        assert_eq!(d[1].span.start.line, 3);
    }

    #[test]
    fn malformed_select_arm_is_clean_error_not_hang() {
        // 2026-10-14: a select arm starting `case x:` spun the arm loop
        // forever (parse_stmt succeeded consuming one ident, the arm match
        // rejected it, sync_stmt returned without progress). sync_stmt now
        // guarantees progress — this must be a clean E0002, never a hang.
        // (A regression here shows up as the test runner timing out.)
        let r = parse_file("t.hy", "fn main() {\n  select {\n  case x: {}\n  }\n}\n");
        assert!(r.is_err(), "malformed select arm must be a parse error");
        assert!(r.unwrap_err().iter().all(|d| d.code == "HYB-E0002"));
    }

    #[test]
    fn brace_misnest_is_clean_error_not_hang() {
        // 2026-10-14: the same mis-nest left a stray `}` that sync_decl saw
        // forever at the top level (zero-progress return). sync_decl now
        // guarantees progress — clean E0002, never a hang.
        let r = parse_file("t.hy", "fn main() {\n  select {\n  case x: {}\n  }\n}\n}\n");
        assert!(r.is_err(), "stray brace must be a parse error");
        assert!(r.unwrap_err().iter().all(|d| d.code == "HYB-E0002"));
    }

    #[test]
    fn spans_point_at_the_right_region() {
        // node spans are plausible: the `:=` of line 2 and the callee
        // `fmt.println` on line 3 land on their own lines.
        let f = parse_file(
            "t.hy",
            "fn main() {\n    count := 1\n    fmt.println(\"x\")\n}",
        )
        .unwrap();
        let body = &f.fns().next().unwrap().body.as_ref().unwrap();
        let Stmt::Local(l) = &body.stmts[0] else {
            panic!("local")
        };
        assert_eq!(l.span.start.line, 2);
        assert_eq!(l.span.start.col, 5); // `count`
        let Stmt::Expr {
            expr: Expr::Call { callee, .. },
            ..
        } = &body.stmts[1]
        else {
            panic!("call")
        };
        assert_eq!(callee.span().start.line, 3);
        assert_eq!(callee.span().start.col, 5); // `fmt`
    }
    #[test]
    fn newline_before_recv_does_not_fuse_into_send() {
        // issue #77 repro A: an expression statement followed by a line
        // that starts with a receive expression must parse as TWO
        // statements (`noop()`; `<-done`), not fuse into `noop() <- done`
        // (the fused form emitted Go that failed `go build`).
        let f = parse_src("fn main() {\n    noop()\n    <-done\n}");
        let body = f.fns().next().unwrap().body.as_ref().unwrap();
        assert_eq!(
            body.stmts.len(),
            2,
            "expected two statements, got {:?}",
            body.stmts
        );
        assert!(
            matches!(&body.stmts[0], Stmt::Expr { expr: Expr::Call { .. }, .. }),
            "first statement is the call"
        );
        assert!(
            matches!(&body.stmts[1], Stmt::Expr { expr: Expr::Receive { .. }, .. }),
            "second statement is a receive expression, not part of a send"
        );
    }

    #[test]
    fn line_leading_paren_starts_a_statement_not_a_call() {
        // `(` on a new line is a statement of its own, never an argument
        // list for the expression above. Before the newline guard,
        // `x := one` / `(2)` parsed as the single statement `x := one(2)`:
        // the `(2)` statement disappeared, `hyb check` reported 0
        // diagnostics and `hyb build` produced a working binary that meant
        // something else. There is no `;` terminator to force the break.
        let f = parse_src("fn main() {\n    x := one\n    (2)\n}");
        let body = f.fns().next().unwrap().body.as_ref().unwrap();
        assert_eq!(
            body.stmts.len(),
            2,
            "expected two statements, got {:?}",
            body.stmts
        );
        let Stmt::Local(l) = &body.stmts[0] else {
            panic!("first statement is the define")
        };
        assert!(
            matches!(&l.values[0], Expr::Ident { name, .. } if name == "one"),
            "the define's value must stay `one`, not become a call: {:?}",
            l.values[0]
        );
        assert!(
            matches!(&body.stmts[1], Stmt::Expr { expr: Expr::Paren { .. }, .. }),
            "the second statement is the parenthesized expression: {:?}",
            body.stmts[1]
        );
    }

    #[test]
    fn line_leading_paren_after_a_call_is_two_statements() {
        // the `<-` send-fusion precedent (#77), for `(`: `f()` then `(x)`
        // must not fuse into `f()(x)`.
        let f = parse_src("fn main() {\n    fmt.println(\"hi\")\n    (x)\n}");
        let body = f.fns().next().unwrap().body.as_ref().unwrap();
        assert_eq!(body.stmts.len(), 2, "got {:?}", body.stmts);
        let Stmt::Expr {
            expr: Expr::Call { args, .. },
            ..
        } = &body.stmts[0]
        else {
            panic!("first statement is the call")
        };
        assert_eq!(args.len(), 1, "the call keeps exactly its own argument");
        assert!(
            matches!(&body.stmts[1], Stmt::Expr { expr: Expr::Paren { .. }, .. }),
            "second statement is the parenthesized expression, not an argument list"
        );
    }

    #[test]
    fn same_line_and_multiline_call_args_still_parse_as_one_call() {
        // The guard is newline sensitivity, not a ban on `(`: a call whose
        // argument list opens on the callee's line keeps working, including
        // the idiomatic multiline spelling.
        let f = parse_src("fn main() {\n    foo(1)\n}");
        assert!(
            matches!(first_stmt(&f), Stmt::Expr { expr: Expr::Call { .. }, .. }),
            "same-line args must stay a call"
        );
        let f = parse_src("fn main() {\n    foo(\n        1,\n        2,\n    )\n}");
        let stmt = first_stmt(&f);
        let Stmt::Expr {
            expr: Expr::Call { args, .. },
            ..
        } = stmt
        else {
            panic!("multiline argument list is still one call, got {:?}", stmt)
        };
        assert_eq!(args.len(), 2);
    }

    #[test]
    fn paren_receiver_takes_a_same_line_member_chain() {
        // The guard only gates `(`; `(recv).member()` on one line must still
        // parse as a method call on a parenthesized receiver.
        let f = parse_src("fn main() {\n    (mux).handle(\"/ping\")\n}");
        let stmt = first_stmt(&f);
        let Stmt::Expr {
            expr: Expr::Call { callee, .. },
            ..
        } = stmt
        else {
            panic!("expected a call, got {:?}", stmt)
        };
        assert!(
            matches!(
                callee.as_ref(),
                Expr::Field { base, name, .. }
                    if name == "handle" && matches!(base.as_ref(), Expr::Paren { .. })
            ),
            "callee is `(mux).handle`, got {:?}",
            callee
        );
    }

    #[test]
    fn same_line_arrow_still_fuses_into_send() {
        // the one-line send spelling must keep parsing as a Send — the
        // #77 guard is newline sensitivity, not a ban on fusion.
        let f = parse_src("fn main() {\n    ch <- 1\n}");
        let stmt = first_stmt(&f);
        assert!(
            matches!(stmt, Stmt::Send { .. }),
            "same-line `ch <- 1` must stay a send, got {:?}",
            stmt
        );
    }

    #[test]
    fn send_value_may_continue_onto_the_next_line() {
        // the fusion guard compares the `<-` token's line to the END of
        // the expression list — a send whose VALUE wraps stays one Send
        // (`<-` and the channel are on line 1).
        let f = parse_src("fn main() {\n    ch <-\n        1\n}");
        let stmt = first_stmt(&f);
        assert!(
            matches!(stmt, Stmt::Send { .. }),
            "wrapped send value must stay one send, got {:?}",
            stmt
        );
    }

    #[test]
    fn array_composite_literals_parse_and_distinguish_from_slices() {
        let src = r#"
const n = 2
struct pair {
    a int
    b int
}
fn main() {
    a := [3]int{1, 2, 3}
    b := [...]int{1, 2, 3}
    c := [3]int{}
    d := [n + 1]int{1, 2, 3}
    e := [2][2]int{{1, 2}, {3, 4}}
    f := [2]pair{{a: 1, b: 2}, {a: 3, b: 4}}
    g := [][2]int{{1, 2}}
    h := [1, 2, 3]
    i := []int{1, 2, 3}
}
"#;
        let file = parse_src(src);
        let body = file.fns().next().unwrap().body.as_ref().unwrap();
        let expr_at = |idx: usize| -> &Expr {
            let Stmt::Local(l) = &body.stmts[idx] else {
                panic!("expected Local at {idx}");
            };
            &l.values[0]
        };

        // a := [3]int{1, 2, 3}
        let (len_a, elem_a, _) = expr_at(0).as_array_lit().expect("[3]int{...} is array lit");
        assert!(len_a.is_some());
        assert_eq!(elem_a.render(), "int");
        assert_eq!(expr_at(0).render(), "[3]int{...}");

        // b := [...]int{1, 2, 3}
        let (len_b, elem_b, _) = expr_at(1)
            .as_array_lit()
            .expect("[...]int{...} is array lit");
        assert!(len_b.is_none());
        assert_eq!(elem_b.render(), "int");
        assert_eq!(expr_at(1).render(), "[...]int{...}");

        // c := [3]int{}
        assert!(expr_at(2).as_array_lit().is_some());
        assert_eq!(expr_at(2).render(), "[3]int{...}");

        // d := [n + 1]int{1, 2, 3}
        let (len_d, _, _) = expr_at(3).as_array_lit().expect("[n + 1]int{...} is array lit");
        assert_eq!(len_d.unwrap().render(), "n + 1");

        // e := [2][2]int{{1, 2}, {3, 4}}
        let (_, elem_e, _) = expr_at(4)
            .as_array_lit()
            .expect("[2][2]int{...} is array lit");
        assert_eq!(elem_e.render(), "[2]int");

        // f := [2]pair{{a: 1, b: 2}, {a: 3, b: 4}}
        let (_, elem_f, _) = expr_at(5).as_array_lit().expect("[2]pair{...} is array lit");
        assert_eq!(elem_f.render(), "pair");

        // g := [][2]int{{1, 2}} — outer is a SLICE of arrays, not an array lit!
        assert!(expr_at(6).as_array_lit().is_none());

        // h := [1, 2, 3] and i := []int{1, 2, 3} — slice literals
        assert!(expr_at(7).as_array_lit().is_none());
        assert!(expr_at(8).as_array_lit().is_none());
    }

    #[test]
    fn float_trailing_dot_parses_as_statement_without_cross_line_folding_issue_659() {
        let src = r#"
package main

import "fmt"

fn main() {
    var scale f64 = 2.
    fmt.println(scale)
    fmt.println(scale * 3.0)
}
"#;
        let f = parse_src(src);
        let main_fn = f.decls.iter().find_map(|d| match d {
            Decl::Fn(f) if f.name == "main" => Some(f),
            _ => None,
        }).expect("main fn decl");
        let body = main_fn.body.as_ref().expect("main body");
        assert_eq!(body.stmts.len(), 3, "must have 3 statements, not 2 folded statements: {:?}", body.stmts);
        assert!(
            matches!(&body.stmts[0], Stmt::Local(LocalDecl { kind: LocalKind::Var, names, values, .. })
                if names == &["scale"] && matches!(values.first(), Some(Expr::Lit { kind: LitKind::Float(s), .. }) if s == "2.")),
            "first statement is var scale f64 = 2., got {:?}", body.stmts[0]
        );
        assert!(
            matches!(&body.stmts[1], Stmt::Expr { expr: Expr::Call { .. }, .. }),
            "second statement is fmt.println(scale)"
        );
        assert!(
            matches!(&body.stmts[2], Stmt::Expr { expr: Expr::Call { .. }, .. }),
            "third statement is fmt.println(scale * 3.0)"
        );
    }

    #[test]
    fn line_leading_dot_does_not_fold_into_previous_statement_issue_659() {
        // A newline-separated `.` must not fold into the previous expression statement
        let src = "fn main() {\n    x := 1\n    .foo\n}";
        let r = parse_file("t.hy", src);
        assert!(r.is_err(), "leading dot on new line must not fold into statement");
    }

}


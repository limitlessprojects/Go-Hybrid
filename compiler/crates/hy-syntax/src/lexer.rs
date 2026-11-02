//! Lexer for the Hybrid language (SPEC-LANGUAGE.md §1.1–1.4; the corpus,
//! spikes and examples are ground truth where the skeleton is silent).
//!
//! Design notes:
//!   * Tokens carry `Span`s (1-based line/col); the parser copies them onto
//!     AST nodes.
//!   * Comments (`//`, non-nested `/* */` per LX8) are skipped. Newlines are
//!     plain whitespace: statement boundaries come from grammar lookahead,
//!     not semicolons (LX9). Documented limitation: a statement-leading `-`
//!     would parse as a continuation of the previous expression — no
//!     corpus/spike/example file does this.
//!   * Interpolated strings use a MODE STACK (LX7): inside `"…"` a `{`
//!     pushes an expression frame (full sub-lexing, including NESTED
//!     strings, e.g. `"{m["fox"]}"` — corpus 19), `}` pops it. A string
//!     therefore lexes as
//!     StrChunk InterpOpen <expr tokens> InterpClose … StrEnd
//!     and the parser assembles `Expr::Interp` (or `Lit::Str` when the
//!     string has no interpolation). `{{` / `}}` escape literal braces.
//!     Raw strings (backticks) are verbatim: no escapes, no interpolation.
//!   * `in` is NOT lexed as a keyword: spike-6 04 (must-parse) uses `in` as
//!     an ordinary channel name (`fn squarer(in <-chan int, …)`,
//!     `for v in in`). It is a contextual keyword the for-parser looks for
//!     as `Ident("in")`.
//!   * Identifiers are `[a-z_][a-z0-9_]*` (LX2); anything containing an
//!     uppercase letter is a lexical HYB-E0001 (identifier-case) error.
//!   * Integer literals keep their base prefix; `_` digit separators are
//!     stripped at lex time (`1_000_000` → `1000000`).

use hy_diag::{Diagnostic, Pos, Span};

/// Parse/lex error code. `HYB-E0002` is the intended "parse-error" slot in
/// the E0 (parse) category — not yet registered in hy-diag's table; an
/// unregistered code passes through `Diagnostic::error` cleanly (no
/// misleading fix-its). The lead registers it when convenient.
pub const PARSE_ERR_CODE: &str = "HYB-E0002";

/// Registered lexical code for LX2 identifier-case violations.
const IDENT_CASE_CODE: &str = "HYB-E0001";

// ---------------------------------------------------------------------------
// Tokens
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Ident(String),
    /// Reserved words (LX3). `in` is notably absent — see module docs.
    Kw(&'static str),
    Int(String),
    Float(String),
    CharLit(char),
    /// One cooked (escape-resolved) literal chunk of a `"…"` string.
    StrChunk(String),
    /// Terminates a `"…"` string.
    StrEnd,
    /// `{` inside a string: starts an embedded expression.
    InterpOpen,
    /// `}` closing an embedded expression.
    InterpClose,
    /// Raw string (backticks), verbatim.
    RawStr(String),
    /// Operators and punctuation, spelled exactly (`:=`, `<-`, `..=`, …).
    Punct(&'static str),
    Eof,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub tok: Tok,
    pub span: Span,
}

/// Reserved words (LX3) + the reserved-for-future list (error to use today).
const KEYWORDS: &[&str] = &[
    "break",
    "chan",
    "const",
    "continue",
    "defer",
    "else",
    "enum",
    "false",
    "fn",
    "for",
    "if",
    "import",
    "let",
    "loop",
    "managed",
    "match",
    "move",
    "package",
    "pub",
    "return",
    "select",
    "struct",
    "task",
    "true",
    "type",
    "var",
    // reserved for future use (error to use today):
    "unsafe",
    "go",
    "func",
    "interface",
    "trait",
    "where",
    "use",
    "mod",
    "as",
    // `mut` is a Rust-ism SPEC-LANGUAGE LX3 deliberately does NOT take
    // (immutability is binding-level `let`, T22). It sat unreserved and
    // `mut x := 3` silently emitted the word `mut` into Go — a broken
    // build from accepted source. Reserved so it fails at parse.
    "mut",
    // `while` is the same trap one word over: Hybrid's while-form is
    // `for cond { }` (ForKind::While). Unreserved, `while x > 0 { … }`
    // parsed as ident `while` + expr + orphan block and emitted the word
    // `while` into Go (`undefined: while` from go vet). Reserved so it
    // fails at parse instead (issue #103).
    "while",
    // Select-arm words (issue #104 + pr-22/pr-108 seam): `case` is
    // RESERVED — Go-style `case <-ch:` used to silently parse as a Send
    // on ident `case` and emit `case case <- ch:` (invalid Go); now it
    // fails cleanly at parse. `default` is deliberately NOT reserved:
    // pr-22's contextual design (a select-arm head only, before `:`)
    // keeps channels named `default` legal — see SPEC-LANGUAGE LX14's
    // select paragraph.
    "case",
    // Go-keyword trap words (issue #228): Hybrid's surface never uses
    // them — its forms are `match` / `for .. in`, and labels don't
    // parse — so as identifiers they only ever leak into emitted Go
    // and break the build (`x := switch` → go: `expected operand, found
    // 'switch'`; `fallthrough` in an if → `fallthrough statement out of
    // place`). Reserved like `while` (#103/#107) and `case` (#104/#108)
    // so they fail at parse instead of at go build. (`map` cannot join
    // this list — `map[K]V` is a legitimate prefix form; it gets the
    // contextual rule in `lex_ident_or_kw` instead.)
    "switch",
    "range",
    "goto",
    "fallthrough",
];

/// Lex a whole source file. On success the token stream ends with a single
/// `Tok::Eof`. On failure returns every lexical diagnostic collected.
pub fn lex(path: &str, src: &str) -> Result<Vec<Token>, Vec<Diagnostic>> {
    let mut lx = Lexer::new(path, src);
    let mut toks = Vec::new();
    while let Some(t) = lx.produce() {
        let is_eof = matches!(t.tok, Tok::Eof);
        toks.push(t);
        if is_eof {
            break;
        }
    }
    if lx.diags.is_empty() {
        Ok(toks)
    } else {
        Err(lx.diags)
    }
}

// ---------------------------------------------------------------------------
// Lexer state machine
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum Frame {
    /// Inside the literal part of a `"…"` string. `emitted_any` suppresses
    /// empty chunk tokens after the first (the first chunk — even empty —
    /// is what tells the parser a string begins).
    Str { emitted_any: bool },
    /// Inside a `{…}` interpolation of a string. `depth` tracks nested
    /// braces (e.g. a struct literal inside an interpolation).
    Interp { depth: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StrEscape {
    Char(char),
    Byte(u8),
}

struct Lexer<'a> {
    path: String,
    chars: Vec<char>,
    i: usize,
    line: u32,
    col: u32,
    diags: Vec<Diagnostic>,
    frames: Vec<Frame>,
    /// One deferred token (a StrChunk is always followed by its StrEnd /
    /// InterpOpen in the same segment scan).
    pending: Option<Token>,
    eof_done: bool,
    _src: &'a str,
}

impl<'a> Lexer<'a> {
    fn new(path: &str, src: &'a str) -> Self {
        Lexer {
            path: path.to_string(),
            chars: src.chars().collect(),
            i: 0,
            line: 1,
            col: 1,
            diags: Vec::new(),
            frames: Vec::new(),
            pending: None,
            eof_done: false,
            _src: src,
        }
    }

    // -- low-level cursor ---------------------------------------------------

    fn at(&self, off: usize) -> Option<char> {
        self.chars.get(self.i + off).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.chars.get(self.i).copied()?;
        self.i += 1;
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    fn pos(&self) -> Pos {
        Pos::new(self.line, self.col)
    }

    fn tok(&self, start: Pos, tok: Tok) -> Token {
        Token {
            tok,
            span: Span::new(start, self.pos()),
        }
    }

    fn diag(&mut self, code: &str, start: Pos, msg: impl Into<String>) {
        self.diags.push(Diagnostic::error(
            code,
            self.path.clone(),
            Span::new(start, self.pos()),
            msg,
        ));
    }

    /// Non-mutating peek: the next non-trivia character after the cursor
    /// (whitespace and `//` / `/* … */` comments skipped), mirroring
    /// `skip_trivia` without consuming anything. Used by the contextual
    /// `map` + `[` rule.
    fn next_code_char_after_trivia(&self) -> Option<char> {
        let mut i = self.i;
        loop {
            match self.chars.get(i).copied() {
                Some(c) if c.is_whitespace() => i += 1,
                Some('/') if self.chars.get(i + 1).copied() == Some('/') => {
                    i += 2;
                    while let Some(c) = self.chars.get(i).copied() {
                        if c == '\n' {
                            break;
                        }
                        i += 1;
                    }
                }
                Some('/') if self.chars.get(i + 1).copied() == Some('*') => {
                    i += 2;
                    loop {
                        if i + 1 >= self.chars.len() {
                            return None;
                        }
                        if self.chars[i] == '*' && self.chars[i + 1] == '/' {
                            i += 2;
                            break;
                        }
                        i += 1;
                    }
                }
                other => return other,
            }
        }
    }

    // -- main driver ----------------------------------------------------------

    fn produce(&mut self) -> Option<Token> {
        if let Some(t) = self.pending.take() {
            return Some(t);
        }
        loop {
            if self.eof_done {
                return None;
            }
            match self.frames.last().cloned() {
                Some(Frame::Str { emitted_any }) => {
                    if let Some(t) = self.lex_str_segment(emitted_any) {
                        return Some(t);
                    }
                }
                Some(Frame::Interp { .. }) | None => {
                    if let Some(t) = self.lex_code() {
                        return Some(t);
                    }
                }
            }
        }
    }

    // -- code mode (top level and inside interpolations) ---------------------

    fn skip_trivia(&mut self) {
        loop {
            match self.at(0) {
                Some(c) if c.is_whitespace() => {
                    self.bump();
                }
                Some('/') if self.at(1) == Some('/') => {
                    while let Some(c) = self.at(0) {
                        if c == '\n' {
                            break;
                        }
                        self.bump();
                    }
                }
                Some('/') if self.at(1) == Some('*') => {
                    let start = self.pos();
                    self.bump();
                    self.bump();
                    let mut closed = false;
                    while let Some(c) = self.at(0) {
                        if c == '*' && self.at(1) == Some('/') {
                            self.bump();
                            self.bump();
                            closed = true;
                            break;
                        }
                        self.bump();
                    }
                    if !closed {
                        self.diag(PARSE_ERR_CODE, start, "unterminated block comment");
                    }
                }
                _ => return,
            }
        }
    }

    fn lex_code(&mut self) -> Option<Token> {
        self.skip_trivia();
        let start = self.pos();
        let Some(c) = self.at(0) else {
            // EOF: close any open interpolation frame with a diagnostic so
            // the parser sees a determinate end.
            if let Some(f) = self.frames.last().cloned() {
                if matches!(f, Frame::Interp { .. }) {
                    self.frames.pop();
                    self.diag(
                        PARSE_ERR_CODE,
                        start,
                        "unterminated interpolation: expected '}'",
                    );
                }
            }
            self.eof_done = true;
            return Some(self.tok(start, Tok::Eof));
        };

        // string starts (pushes a Str frame; the next produce() call scans it)
        if c == '"' {
            self.bump();
            self.frames.push(Frame::Str { emitted_any: false });
            return None; // re-dispatch
        }

        // interpolation / block braces
        if c == '{' {
            self.bump();
            if let Some(Frame::Interp { depth }) = self.frames.last_mut() {
                *depth += 1;
            }
            return Some(self.tok(start, Tok::Punct("{")));
        }
        if c == '}' {
            self.bump();
            match self.frames.last_mut() {
                Some(Frame::Interp { depth }) if *depth == 0 => {
                    self.frames.pop();
                    return Some(self.tok(start, Tok::InterpClose));
                }
                Some(Frame::Interp { depth }) => {
                    *depth -= 1;
                    return Some(self.tok(start, Tok::Punct("}")));
                }
                _ => return Some(self.tok(start, Tok::Punct("}"))),
            }
        }

        // identifiers / keywords
        if c == '_' || c.is_ascii_lowercase() || c.is_ascii_uppercase() {
            return Some(self.lex_ident_or_kw());
        }

        // numbers (incl. leading `.` floats)
        if c.is_ascii_digit() || (c == '.' && self.at(1).map(|d| d.is_ascii_digit()) == Some(true))
        {
            return Some(self.lex_number());
        }

        // char literals
        if c == '\'' {
            return Some(self.lex_char());
        }

        // raw strings
        if c == '`' {
            return Some(self.lex_raw_str());
        }

        Some(self.lex_punct())
    }

    fn lex_ident_or_kw(&mut self) -> Token {
        let start = self.pos();
        let mut s = String::new();
        while let Some(c) = self.at(0) {
            if c == '_' || c.is_ascii_alphanumeric() {
                s.push(c);
                self.bump();
            } else {
                break;
            }
        }
        if s.chars().any(|c| c.is_ascii_uppercase()) {
            // LX2: Hybrid identifiers are snake_case; foreign names are
            // normalized at binding time, so uppercase never appears here.
            self.diag(
                IDENT_CASE_CODE,
                start,
                format!(
                    "identifier `{}` is not snake_case (LX2) — Hybrid identifiers are lowercase; write it in snake_case",
                    s
                ),
            );
        }
        if s == "map" && self.next_code_char_after_trivia() != Some('[') {
            // `map` is the map type/literal prefix (`map[K]V`, `map{k: v}`),
            // never a free identifier: bare `map` in any position (expr,
            // binding, param, field, fn name, type) is emitted verbatim as
            // Go's `map` keyword, where it cannot parse (`go: expected '['`
            // / `expected '}', found 'map'`) — issue #228. The `[` must be
            // the next real token, so trivia (incl. comments) is skipped by
            // the peek, matching the parser's contextual checks.
            self.diag(
                PARSE_ERR_CODE,
                start,
                "`map` must be followed by `[` — it is Hybrid's map type/literal prefix, never a free name; a bare `map` would be emitted as Go's `map` keyword and fail `go build` (issue #228)",
            );
        }
        if let Some(kw) = KEYWORDS.iter().find(|k| **k == s) {
            self.tok(start, Tok::Kw(kw))
        } else {
            self.tok(start, Tok::Ident(s))
        }
    }

    fn lex_number(&mut self) -> Token {
        let start = self.pos();
        let mut s = String::new();

        // prefixed bases
        if self.at(0) == Some('0')
            && matches!(
                self.at(1),
                Some('x') | Some('X') | Some('o') | Some('O') | Some('b') | Some('B')
            )
        {
            let base = self.at(1).unwrap().to_ascii_lowercase();
            self.bump();
            self.bump();
            s.push('0');
            s.push(base);
            while let Some(c) = self.at(0) {
                if c == '_' || c.is_ascii_alphanumeric() {
                    if c != '_' {
                        s.push(c);
                    }
                    self.bump();
                } else {
                    break;
                }
            }
            if self.at(0) == Some('.') && self.at(1) != Some('.') {
                // A prefixed base literal (0x, 0b, 0o) cannot have a fractional part
                // (SPEC-LANGUAGE.md LX6). Consume the dot and subsequent characters so
                // it fails as a single malformed literal instead of leaking as an orphan
                // leading-dot float on the same line (issue #1168).
                s.push('.');
                self.bump();
                while let Some(c) = self.at(0) {
                    if c == '_' || c.is_ascii_alphanumeric() || c == '+' || c == '-' {
                        s.push(c);
                        self.bump();
                    } else {
                        break;
                    }
                }
                self.escape_diag(
                    start,
                    false,
                    format!(
                        "malformed 0{base} integer literal: prefixed base literals cannot have a fractional part (SPEC-LANGUAGE.md LX6)"
                    ),
                );
                return self.tok(start, Tok::Int(s));
            }
            let digits = &s[2..];
            if digits.is_empty() {
                self.escape_diag(
                    start,
                    false,
                    format!(
                        "malformed 0{base} integer literal: expected at least one digit after `0{base}`"
                    ),
                );
            } else if let Some(bad) = digits.chars().find(|&c| match base {
                'x' => !c.is_ascii_hexdigit(),
                'o' => !matches!(c, '0'..='7'),
                'b' => !matches!(c, '0' | '1'),
                _ => true,
            }) {
                self.escape_diag(
                    start,
                    false,
                    format!(
                        "malformed 0{base} integer literal: invalid digit `{bad}` for `0{base}` base"
                    ),
                );
            }
            return self.tok(start, Tok::Int(s));
        }

        // decimal / float
        let mut is_float = false;
        while let Some(c) = self.at(0) {
            if c == '_' || c.is_ascii_digit() {
                if c != '_' {
                    s.push(c);
                }
                self.bump();
            } else {
                break;
            }
        }
        // fractional part — SPEC LX6: `1.5`, `2.`, `.5`, `1e9`.
        // A dot after digits is part of a float unless:
        // 1. It is followed by `.` (ranges `0..n`, `1..=8`, ellipsis `...`)
        // 2. It is followed by an identifier/keyword start (e.g. `2.foo`),
        //    UNLESS that starts an exponent (`2.e3`, `2.e+3`, `2.e-3`).
        if self.at(0) == Some('.') {
            let is_range = self.at(1) == Some('.');
            let is_exp = matches!(self.at(1), Some('e') | Some('E')) && {
                let k = if matches!(self.at(2), Some('+') | Some('-')) { 3 } else { 2 };
                self.at(k).map(|d| d.is_ascii_digit()) == Some(true)
            };
            let is_ident_start = self.at(1).map(|c| c.is_ascii_alphabetic() || c == '_').unwrap_or(false);

            if !is_range && (!is_ident_start || is_exp) {
                is_float = true;
                s.push('.');
                self.bump();
                while let Some(c) = self.at(0) {
                    if c == '_' || c.is_ascii_digit() {
                        if c != '_' {
                            s.push(c);
                        }
                        self.bump();
                    } else {
                        break;
                    }
                }
            }
        }
        // exponent
        if matches!(self.at(0), Some('e') | Some('E')) {
            let mut k = 1;
            if matches!(self.at(1), Some('+') | Some('-')) {
                k = 2;
            }
            if self.at(k).map(|d| d.is_ascii_digit()) == Some(true) {
                is_float = true;
                // push the exponent marker (and sign) exactly once — the
                // bumped chars ARE the marker; no extra `e` push.
                for _ in 0..k {
                    if let Some(c) = self.bump() {
                        s.push(c);
                    }
                }
                while let Some(c) = self.at(0) {
                    if c == '_' || c.is_ascii_digit() {
                        if c != '_' {
                            s.push(c);
                        }
                        self.bump();
                    } else {
                        break;
                    }
                }
            }
        }
        if is_float {
            self.tok(start, Tok::Float(s))
        } else {
            if s.starts_with('0') && s.len() > 1 {
                if let Some(bad) = s.chars().find(|&c| !matches!(c, '0'..='7')) {
                    self.escape_diag(
                        start,
                        false,
                        format!(
                            "malformed octal integer literal `{s}`: invalid digit `{bad}` in leading-zero octal literal"
                        ),
                    );
                }
            }
            self.tok(start, Tok::Int(s))
        }
    }

    fn lex_char(&mut self) -> Token {
        let start = self.pos();
        self.bump(); // opening quote
        if self.at(0) == Some('\'') {
            self.bump();
            self.escape_diag(start, false, "empty char literal");
            return self.tok(start, Tok::CharLit('\0'));
        }
        let ch = match self.at(0) {
            Some('\\') => {
                let esc_start = self.pos();
                self.bump();
                let e = self.escape_char(false);
                if e.is_none() {
                    // diagnostic already recorded; recover to closing quote on this line
                    while let Some(c) = self.at(0) {
                        if c == '\n' {
                            break;
                        }
                        self.bump();
                        if c == '\'' {
                            break;
                        }
                    }
                    return self.tok(start, Tok::CharLit('\0'));
                }
                let _ = esc_start;
                e.unwrap()
            }
            Some(c) if c != '\n' => {
                self.bump();
                c
            }
            _ => {
                self.diag(PARSE_ERR_CODE, start, "unterminated char literal");
                return self.tok(start, Tok::CharLit('\0'));
            }
        };
        if self.at(0) == Some('\'') {
            self.bump();
        } else {
            self.diag(PARSE_ERR_CODE, start, "unterminated char literal");
            while let Some(c) = self.at(0) {
                if c == '\n' {
                    break;
                }
                self.bump();
                if c == '\'' {
                    break;
                }
            }
        }
        self.tok(start, Tok::CharLit(ch))
    }

    fn lex_raw_str(&mut self) -> Token {
        let start = self.pos();
        self.bump(); // opening backtick
        let mut s = String::new();
        loop {
            match self.at(0) {
                Some('`') => {
                    self.bump();
                    return self.tok(start, Tok::RawStr(s));
                }
                Some(c) => {
                    s.push(c);
                    self.bump();
                }
                None => {
                    self.diag(PARSE_ERR_CODE, start, "unterminated raw string literal");
                    return self.tok(start, Tok::RawStr(s));
                }
            }
        }
    }

    fn lex_punct(&mut self) -> Token {
        let start = self.pos();
        let rest: String = {
            let mut r = String::new();
            for k in 0..4 {
                match self.at(k) {
                    Some(c) => r.push(c),
                    None => break,
                }
            }
            r
        };
        // MULTI is scanned IN ORDER and matches by prefix, so the 3-char
        // compound forms must precede their 2-char prefixes (`<<=` before
        // `<<`, `>>=` before `>>`).
        const MULTI: &[&str] = &[
            "...", "..=", "=>", ":=", "==", "!=", "<=", ">=", "&&", "||", "<-", "<<=", ">>=",
            "<<", ">>", "++", "--", "+=", "-=", "*=", "/=", "%=", "|=", "&=", "^=", "..",
        ];
        for m in MULTI {
            if rest.starts_with(m) {
                for _ in 0..m.chars().count() {
                    self.bump();
                }
                return self.tok(start, Tok::Punct(m));
            }
        }
        const SINGLE: &[char] = &[
            '+', '-', '*', '/', '%', '!', '<', '>', '=', '&', '|', '^', '.', ',', ';', ':', '?',
            '(', ')', '{', '}', '[', ']',
        ];
        if SINGLE.contains(&rest.chars().next().unwrap()) {
            let c = self.bump().unwrap();
            return self.tok(start, Tok::Punct(static_punct(c)));
        }
        let c = self.bump().unwrap_or('\u{fffd}');
        self.diag(
            PARSE_ERR_CODE,
            start,
            format!("unexpected character `{}`", c),
        );
        self.tok(start, Tok::Punct("?"))
    }

    // -- string mode -----------------------------------------------------------

    /// Flush buffered byte-valued escapes (`\xNN`, `\NNN`) into `cooked`.
    /// Valid UTF-8 sequences decode directly to their Unicode scalar values
    /// (preserving multi-byte UTF-8 sequences like `"\xc3\xa9"` -> `"é"` per
    /// LX7/T3); any non-UTF-8 remainder bytes fall back byte-for-byte.
    fn flush_str_bytes(pending_bytes: &mut Vec<u8>, cooked: &mut String) {
        if pending_bytes.is_empty() {
            return;
        }
        for chunk in pending_bytes.utf8_chunks() {
            cooked.push_str(chunk.valid());
            for &b in chunk.invalid() {
                cooked.push(char::from(b));
            }
        }
        pending_bytes.clear();
    }

    /// Scan one segment of a `"…"` string: a literal chunk (terminated by
    /// `"` or EOF) or an interpolation opener (terminated by `{`). Returns
    /// `None` never — every path emits a token (chunks carry their StrEnd /
    /// InterpOpen successor in `pending`).
    fn lex_str_segment(&mut self, emitted_any: bool) -> Option<Token> {
        let start = self.pos();
        let mut cooked = String::new();
        let mut pending_bytes: Vec<u8> = Vec::new();
        loop {
            let Some(c) = self.at(0) else {
                Self::flush_str_bytes(&mut pending_bytes, &mut cooked);
                self.frames.pop();
                self.diag(PARSE_ERR_CODE, start, "unterminated string literal");
                if cooked.is_empty() && emitted_any {
                    return Some(self.tok(start, Tok::StrEnd));
                }
                self.pending = Some(self.tok(self.pos(), Tok::StrEnd));
                return Some(self.tok(start, Tok::StrChunk(cooked)));
            };
            match c {
                '"' => {
                    Self::flush_str_bytes(&mut pending_bytes, &mut cooked);
                    self.bump();
                    self.frames.pop();
                    if cooked.is_empty() && emitted_any {
                        return Some(self.tok(start, Tok::StrEnd));
                    }
                    self.pending = Some(self.tok(self.pos(), Tok::StrEnd));
                    return Some(self.tok(start, Tok::StrChunk(cooked)));
                }
                '{' => {
                    Self::flush_str_bytes(&mut pending_bytes, &mut cooked);
                    if self.at(1) == Some('{') {
                        self.bump();
                        self.bump();
                        cooked.push('{');
                    } else {
                        self.bump();
                        // mark the Str frame as having emitted, push the interp frame
                        if let Some(Frame::Str { emitted_any }) = self.frames.last_mut() {
                            *emitted_any = true;
                        }
                        self.frames.push(Frame::Interp { depth: 0 });
                        if cooked.is_empty() && emitted_any {
                            return Some(self.tok(start, Tok::InterpOpen));
                        }
                        self.pending = Some(self.tok(self.pos(), Tok::InterpOpen));
                        return Some(self.tok(start, Tok::StrChunk(cooked)));
                    }
                }
                '}' => {
                    Self::flush_str_bytes(&mut pending_bytes, &mut cooked);
                    // `}}` escapes a literal brace; a lone `}` is literal text
                    self.bump();
                    if self.at(0) == Some('}') {
                        self.bump();
                    }
                    cooked.push('}');
                }
                '\\' => {
                    self.bump();
                    match self.escape_raw(true) {
                        Some(StrEscape::Byte(b)) => pending_bytes.push(b),
                        Some(StrEscape::Char(e)) => {
                            Self::flush_str_bytes(&mut pending_bytes, &mut cooked);
                            cooked.push(e);
                        }
                        None => {
                            Self::flush_str_bytes(&mut pending_bytes, &mut cooked);
                            // unknown escape: keep the char verbatim (diag recorded)
                            if let Some(c2) = self.bump() {
                                cooked.push(c2);
                            }
                        }
                    }
                }
                _ => {
                    Self::flush_str_bytes(&mut pending_bytes, &mut cooked);
                    cooked.push(c);
                    self.bump();
                }
            }
        }
    }

    /// One escape sequence after `\` for rune literals (`'…'`), where `\xNN`
    /// and `\NNN` denote a code point in `0..=255`.
    fn escape_char(&mut self, str_mode: bool) -> Option<char> {
        match self.escape_raw(str_mode)? {
            StrEscape::Char(c) => Some(c),
            StrEscape::Byte(b) => Some(char::from(b)),
        }
    }

    /// One escape sequence after `\`. `str_mode` selects the Go string escape
    /// set (`\uNNNN`); char literals use the rune set (`\u{…}`, LX6).
    /// Byte-valued escapes (`\xNN`, `\NNN`) return `StrEscape::Byte(u8)` so
    /// interpreted strings can assemble multi-byte UTF-8 sequences before
    /// decoding, while rune literals convert the byte to its scalar value.
    ///
    /// Fail-closed contract (GUARANTEES.md row 4's spirit, extended to the
    /// lexer): every malformed escape this function can SEE must record a
    /// diagnostic before returning `None` — both callers assume "diag
    /// recorded" on `None`. The one silent `None` left is end-of-input
    /// mid-escape: the literal loop then runs to EOF and the caller emits
    /// its own `unterminated string/char literal` diagnostic.
    fn escape_raw(&mut self, str_mode: bool) -> Option<StrEscape> {
        let start = self.pos();
        let c = self.bump()?;
        let out = match c {
            'n' => StrEscape::Char('\n'),
            't' => StrEscape::Char('\t'),
            'r' => StrEscape::Char('\r'),
            '\\' => StrEscape::Char('\\'),
            '"' => StrEscape::Char('"'),
            '\'' => StrEscape::Char('\''),
            'a' => StrEscape::Char('\u{7}'),
            'b' => StrEscape::Char('\u{8}'),
            'f' => StrEscape::Char('\u{c}'),
            'v' => StrEscape::Char('\u{b}'),
            'x' => {
                let mut v: u32 = 0;
                for _ in 0..2 {
                    let h = self.bump()?;
                    let Some(d) = h.to_digit(16) else {
                        self.escape_diag(
                            start,
                            str_mode,
                            format!(
                                "malformed \\x escape: `{h}` is not a hex digit (\\x takes exactly two hex digits, e.g. \\x41)"
                            ),
                        );
                        return None;
                    };
                    v = v * 16 + d;
                }
                StrEscape::Byte(v as u8)
            }
            'u' => {
                if str_mode {
                    let mut v: u32 = 0;
                    for _ in 0..4 {
                        let h = self.bump()?;
                        let Some(d) = h.to_digit(16) else {
                            self.escape_diag(
                                start,
                                str_mode,
                                format!(
                                    "malformed \\u escape: `{h}` is not a hex digit (\\u takes exactly four hex digits, e.g. \\u00e9; the \\u{{…}} rune form (LX6) is char-literal syntax only)"
                                ),
                            );
                            return None;
                        };
                        v = v * 16 + d;
                    }
                    StrEscape::Char(self.scalar_or_diag(v, start, str_mode, "\\u")?)
                } else {
                    // rune style: \u{1F600}
                    if self.bump() != Some('{') {
                        self.escape_diag(
                            start,
                            str_mode,
                            "malformed \\u{…} escape: expected `{` after \\u in the rune form",
                        );
                        return None;
                    }
                    let mut v: u32 = 0;
                    let mut digits: usize = 0;
                    loop {
                        match self.at(0) {
                            Some(h) if h.is_ascii_hexdigit() => {
                                let d = h.to_digit(16).unwrap_or(0);
                                v = v.saturating_mul(16).saturating_add(d);
                                digits += 1;
                                self.bump();
                            }
                            Some('}') => {
                                self.bump();
                                if digits == 0 {
                                    self.escape_diag(
                                        start,
                                        str_mode,
                                        "malformed \\u{…} escape: expected hex digits then `}`",
                                    );
                                    return None;
                                }
                                break;
                            }
                            _ => {
                                self.escape_diag(
                                    start,
                                    str_mode,
                                    "malformed \\u{…} escape: expected hex digits then `}`",
                                );
                                return None;
                            }
                        }
                    }
                    StrEscape::Char(self.scalar_or_diag(v, start, str_mode, "\\u{…}")?)
                }
            }
            'U' => {
                let mut v: u32 = 0;
                for _ in 0..8 {
                    let h = self.bump()?;
                    let Some(d) = h.to_digit(16) else {
                        self.escape_diag(
                            start,
                            str_mode,
                            format!(
                                "malformed \\U escape: `{h}` is not a hex digit (\\U takes exactly eight hex digits, e.g. \\U0001F600)"
                            ),
                        );
                        return None;
                    };
                    v = v * 16 + d;
                }
                StrEscape::Char(self.scalar_or_diag(v, start, str_mode, "\\U")?)
            }
            '0'..='7' => {
                // octal \NNN — at most three digits (c plus up to two more),
                // representing a byte value 0..=0o377 (0..=255).
                let mut v: u32 = c.to_digit(8)?;
                for _ in 0..2 {
                    let Some(d) = self.at(0).and_then(|ch| ch.to_digit(8)) else {
                        break;
                    };
                    v = v * 8 + d;
                    self.bump();
                }
                if v > 0xFF {
                    self.escape_diag(
                        start,
                        str_mode,
                        format!(
                            "invalid octal escape: \\{v:o} ({v}) exceeds maximum byte value 255 (\\377)"
                        ),
                    );
                    return None;
                }
                StrEscape::Byte(v as u8)
            }
            _ => {
                self.diag(
                    PARSE_ERR_CODE,
                    start,
                    format!("unknown escape sequence `\\{}`", c),
                );
                return None;
            }
        };
        Some(out)
    }

    /// `char::from_u32(v)` or a named fail-closed diagnostic (surrogate
    /// D800–DFFF or above U+10FFFF). `\u`, `\u{…}` and `\U` share it.
    fn scalar_or_diag(
        &mut self,
        v: u32,
        start: Pos,
        str_mode: bool,
        escape_name: &str,
    ) -> Option<char> {
        match char::from_u32(v) {
            Some(ch) => Some(ch),
            None => {
                self.escape_diag(
                    start,
                    str_mode,
                    format!(
                        "invalid {escape_name} escape: U+{v:X} is not a Unicode scalar value (D800–DFFF surrogates and values above U+10FFFF are invalid)"
                    ),
                );
                None
            }
        }
    }

    /// Record a malformed-escape diagnostic tagged with the lexical rule in
    /// force (LX6 rune literals / LX7 interpreted strings) so the corpus
    /// harness can join on it.
    fn escape_diag(&mut self, start: Pos, str_mode: bool, msg: impl Into<String>) {
        let rule = if str_mode { "LX7" } else { "LX6" };
        self.diags.push(
            Diagnostic::error(PARSE_ERR_CODE, self.path.clone(), Span::new(start, self.pos()), msg)
                .with_rule(rule),
        );
    }
}

fn static_punct(c: char) -> &'static str {
    match c {
        '+' => "+",
        '-' => "-",
        '*' => "*",
        '/' => "/",
        '%' => "%",
        '!' => "!",
        '<' => "<",
        '>' => ">",
        '=' => "=",
        '&' => "&",
        '|' => "|",
        '^' => "^",
        '.' => ".",
        ',' => ",",
        ';' => ";",
        ':' => ":",
        '?' => "?",
        '(' => "(",
        ')' => ")",
        '{' => "{",
        '}' => "}",
        '[' => "[",
        ']' => "]",
        _ => "?",
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(src: &str) -> Vec<Tok> {
        lex("test.hy", src)
            .expect("lex ok")
            .into_iter()
            .map(|t| t.tok)
            .collect()
    }

    #[test]
    fn plain_string_no_interp() {
        let t = toks(r#"let x = "hello\nworld""#);
        assert_eq!(
            t,
            vec![
                Tok::Kw("let"),
                Tok::Ident("x".into()),
                Tok::Punct("="),
                Tok::StrChunk("hello\nworld".into()),
                Tok::StrEnd,
                Tok::Eof,
            ]
        );
    }

    #[test]
    fn interp_with_nested_quotes() {
        // corpus 19: fmt.println("fox count: {m["fox"]}")
        let t = toks(r#"fmt.println("fox count: {m["fox"]}")"#);
        assert_eq!(
            t,
            vec![
                Tok::Ident("fmt".into()),
                Tok::Punct("."),
                Tok::Ident("println".into()),
                Tok::Punct("("),
                Tok::StrChunk("fox count: ".into()),
                Tok::InterpOpen,
                Tok::Ident("m".into()),
                Tok::Punct("["),
                Tok::StrChunk("fox".into()),
                Tok::StrEnd,
                Tok::Punct("]"),
                Tok::InterpClose,
                Tok::StrEnd,
                Tok::Punct(")"),
                Tok::Eof,
            ]
        );
    }

    #[test]
    fn brace_escapes_and_multiple_interps() {
        let t = toks(r#""brace: {{}} k{seq}-{i}""#);
        assert_eq!(
            t,
            vec![
                Tok::StrChunk("brace: {} k".into()),
                Tok::InterpOpen,
                Tok::Ident("seq".into()),
                Tok::InterpClose,
                Tok::StrChunk("-".into()),
                Tok::InterpOpen,
                Tok::Ident("i".into()),
                Tok::InterpClose,
                Tok::StrEnd,
                Tok::Eof,
            ]
        );
    }

    #[test]
    fn nested_string_with_own_interp_inside_interp() {
        let t = toks(r#""outer {fmt.println("inner {x}")} tail""#);
        assert_eq!(
            t,
            vec![
                Tok::StrChunk("outer ".into()),
                Tok::InterpOpen,
                Tok::Ident("fmt".into()),
                Tok::Punct("."),
                Tok::Ident("println".into()),
                Tok::Punct("("),
                Tok::StrChunk("inner ".into()),
                Tok::InterpOpen,
                Tok::Ident("x".into()),
                Tok::InterpClose,
                Tok::StrEnd,
                Tok::Punct(")"),
                Tok::InterpClose,
                Tok::StrChunk(" tail".into()),
                Tok::StrEnd,
                Tok::Eof,
            ]
        );
    }

    #[test]
    fn raw_strings_verbatim() {
        let t = toks(r#"let pat = `^[^@\s]+$`"#);
        assert_eq!(
            t,
            vec![
                Tok::Kw("let"),
                Tok::Ident("pat".into()),
                Tok::Punct("="),
                Tok::RawStr(r"^[^@\s]+$".into()),
                Tok::Eof,
            ]
        );
    }

    // -- malformed escapes fail closed (issue: silent value corruption) ------
    // Regression: `\x`/`\u`/`\U` with a non-hex digit, the rune-style
    // `\u{…}` inside a STRING, and surrogate/oversized scalars used to
    // return `None` from escape_char WITHOUT recording the diagnostic both
    // callers assume ("diag recorded"), so `"\u{41}"` was ACCEPTED with a
    // silently corrupted value ("41}"). Every malformed escape the lexer
    // can see must now be named and tagged LX6/LX7.

    fn lex_errs(src: &str) -> Vec<Diagnostic> {
        match lex("test.hy", src) {
            Ok(_) => panic!("expected lex failure for: {src}"),
            Err(d) => d,
        }
    }

    #[test]
    fn malformed_string_u_brace_escape_is_diagnosed() {
        // The LX6 rune form used in an interpreted string (LX7 = Go escape set).
        let errs = lex_errs(r#"fn main() { s := "\u{41}" }"#);
        assert!(errs.iter().any(|d| d.message.contains("malformed \\u escape")
            && d.tier0_rule.as_deref() == Some("LX7")), "{errs:?}");
    }

    #[test]
    fn malformed_string_x_escape_is_diagnosed() {
        let errs = lex_errs(r#"fn main() { s := "\xG1" }"#);
        assert!(errs.iter().any(|d| d.message.contains("malformed \\x escape")
            && d.tier0_rule.as_deref() == Some("LX7")), "{errs:?}");
    }

    #[test]
    fn malformed_string_short_u_escape_is_diagnosed() {
        let errs = lex_errs(r#"fn main() { s := "\u12G4" }"#);
        assert!(errs.iter().any(|d| d.message.contains("malformed \\u escape")
            && d.tier0_rule.as_deref() == Some("LX7")), "{errs:?}");
    }

    #[test]
    fn surrogate_string_u_escape_is_diagnosed() {
        let errs = lex_errs(r#"fn main() { s := "\ud800" }"#);
        assert!(errs.iter().any(|d| d.message.contains("not a Unicode scalar value")
            && d.tier0_rule.as_deref() == Some("LX7")), "{errs:?}");
    }

    #[test]
    fn malformed_rune_u_without_brace_is_diagnosed() {
        let errs = lex_errs(r#"fn main() { c := '\u12' }"#);
        assert!(errs.iter().any(|d| d.message.contains("expected `{` after \\u")
            && d.tier0_rule.as_deref() == Some("LX6")), "{errs:?}");
    }

    #[test]
    fn surrogate_rune_u_brace_escape_is_diagnosed() {
        let errs = lex_errs(r#"fn main() { c := '\u{D800}' }"#);
        assert!(errs.iter().any(|d| d.message.contains("not a Unicode scalar value")
            && d.tier0_rule.as_deref() == Some("LX6")), "{errs:?}");
    }

    #[test]
    fn valid_escapes_still_lex_clean() {
        // Over-rejection guard: the full VALID escape surface stays accepted.
        let t = toks(r#"fn main() { s := "\u00e9\x41\U0001F600\101" c := '\u{1F600}' d := '\x41' }"#);
        assert!(t.contains(&Tok::StrChunk("\u{e9}A\u{1F600}A".into())));
        assert!(t.contains(&Tok::CharLit('\u{1F600}')));
        assert!(t.contains(&Tok::CharLit('A')));
    }

    #[test]
    fn numbers_and_ranges() {
        let t = toks("for i in 0..n { x := 1_000_000 y := 0x1F f := 1.5e3 }");
        assert!(t.contains(&Tok::Int("1000000".into())));
        // hex digits are kept as written (only the base prefix normalizes)
        assert!(t.contains(&Tok::Int("0x1F".into())));
        assert!(t.contains(&Tok::Float("1.5e3".into())));
        assert!(t.contains(&Tok::Punct("..")));
    }

    #[test]
    fn float_trailing_dot_and_ranges_issue_659() {
        // LX6: Float literals include `2.`, `.5`, `1.5`, `1e9`.
        let t = toks("var scale f64 = 2.\nx := 2. + 1.0\ny := .5\nz := 2.e3\nw := 2.e+3\nfor i in 0..n {}\nfor j in 1..=8 {}");
        assert!(t.contains(&Tok::Float("2.".into())));
        assert!(t.contains(&Tok::Float(".5".into())));
        assert!(t.contains(&Tok::Float("2.e3".into())));
        assert!(t.contains(&Tok::Float("2.e+3".into())));
        assert!(t.contains(&Tok::Punct("..")));
        assert!(t.contains(&Tok::Punct("..=")));

        // Range operators keep their leading integer intact
        let r1 = toks("0..n");
        assert_eq!(r1[0], Tok::Int("0".into()));
        assert_eq!(r1[1], Tok::Punct(".."));

        let r2 = toks("1..=8");
        assert_eq!(r2[0], Tok::Int("1".into()));
        assert_eq!(r2[1], Tok::Punct("..="));
    }

    #[test]
    fn prefixed_base_literal_fraction_issue_1168() {
        // Issue #1168: prefixed base literals (0x, 0b, 0o) followed by a fractional
        // dot must not split into Tok::Int and orphan leading-dot Tok::Float.
        for (src, base) in [
            ("x := 0x1.8", "0x"),
            ("x := 0x1.f", "0x"),
            ("x := 0x1.8p-2", "0x"),
            ("x := 0x1.", "0x"),
            ("x := 0o7.5", "0o"),
            ("x := 0b1.0", "0b"),
        ] {
            let errs = lex_errs(src);
            assert!(
                errs.iter().any(|d| d.message.contains(&format!("malformed {base}"))
                    && d.message.contains("fractional part")
                    && d.tier0_rule.as_deref() == Some("LX6")),
                "expected malformed {base} diagnostic for `{src}`, got: {errs:?}"
            );
        }

        // Range syntax on prefixed base literals must remain valid (not treated as fraction)
        let r_hex = toks("0x1..0x10");
        assert_eq!(r_hex[0], Tok::Int("0x1".into()));
        assert_eq!(r_hex[1], Tok::Punct(".."));
        assert_eq!(r_hex[2], Tok::Int("0x10".into()));

        let r_hex_inc = toks("0x1..=0x10");
        assert_eq!(r_hex_inc[0], Tok::Int("0x1".into()));
        assert_eq!(r_hex_inc[1], Tok::Punct("..="));
        assert_eq!(r_hex_inc[2], Tok::Int("0x10".into()));
    }


    #[test]
    fn operators_lex_correctly() {
        let t = toks("a <- b c <-chan d e := f g ..= h i++ j-- k += 1 l ... m");
        assert!(t.contains(&Tok::Punct("<-")));
        assert!(t.contains(&Tok::Punct(":=")));
        assert!(t.contains(&Tok::Punct("..=")));
        assert!(t.contains(&Tok::Punct("++")));
        assert!(t.contains(&Tok::Punct("--")));
        assert!(t.contains(&Tok::Punct("+=")));
        assert!(t.contains(&Tok::Punct("...")));
    }

    #[test]
    fn comments_skipped() {
        let t = toks("a // line\n /* block /* not nested */ b");
        assert_eq!(
            t,
            vec![Tok::Ident("a".into()), Tok::Ident("b".into()), Tok::Eof,]
        );
    }

    #[test]
    fn in_is_contextual_not_keyword() {
        // spike-6 04: `fn squarer(in <-chan int, …)` / `for v in in`
        let t = toks("fn squarer(in <-chan int) { for v in in { } }");
        assert!(!t.contains(&Tok::Kw("in")));
        assert!(
            t.iter()
                .filter(|t| matches!(t, Tok::Ident(s) if s == "in"))
                .count()
                >= 3
        );
        assert!(t.contains(&Tok::Kw("chan")));
    }

    #[test]
    fn uppercase_identifier_is_lexical_error() {
        let r = lex("t.hy", "WorkerID := 3");
        assert!(r.is_err());
        let diags = r.unwrap_err();
        assert_eq!(diags[0].code, "HYB-E0001");
    }

    #[test]
    fn spans_are_one_based_line_col() {
        let t = lex("t.hy", "fn main() {\n    x := 1\n}").unwrap();
        // find the `:=`
        let d = t.iter().find(|t| t.tok == Tok::Punct(":=")).unwrap();
        assert_eq!(d.span.start.line, 2);
        assert_eq!(d.span.start.col, 7);
    }

    #[test]
    fn unterminated_string_is_error() {
        assert!(lex("t.hy", r#"x := "oops"#).is_err());
    }

    #[test]
    fn go_keyword_traps_are_reserved() {
        // issue #228: these four leak into emitted Go as keywords and
        // break the build, so they lex as Kw (parser turns that into a
        // clean parse error naming the word — same shape as `while`).
        for w in ["switch", "range", "goto", "fallthrough"] {
            let r = lex("t.hy", &format!("fn main() {{ x := {} }}", w));
            let t = r.expect("lex ok");
            assert!(
                t.iter().any(|t| matches!(&t.tok, Tok::Kw(k) if *k == w)),
                "`{}` must lex as a keyword",
                w
            );
        }
    }

    #[test]
    fn map_needs_bracket_everywhere() {
        // issue #228 B5–B6 + the declaration positions: bare `map` as an
        // expr, binding, param, fn name, or freestanding type is emitted
        // as Go's `map` keyword and cannot parse.
        for src in [
            "fn main() { x := map }",
            "fn main() { map := 3 }",
            "fn f(map int) {}",
            "fn map() {}",
            "fn main() { var x map }",
            "struct s { map int }",
        ] {
            let r = lex("t.hy", src);
            assert!(r.is_err(), "bare `map` must be a lexical error: {}", src);
            let diags = r.unwrap_err();
            assert!(
                diags.iter().any(|d| d.message.contains("`map`")),
                "diagnostic must name `map`: {:?}",
                diags
            );
        }
    }

    #[test]
    fn map_prefix_forms_stay_lexed() {
        // controls: map types, map literals, and `[` reached through
        // trivia must keep working (they are the legitimate `map` uses).
        let ok = [
            "m := map[string]int{}",
            "var x map[int]string",
            "fn f(m map[bool]u8) {}",
            "y := map /* c */ [string]int{}",
            "z := map\n    [string]int{}",
        ];
        for src in ok {
            assert!(
                lex("t.hy", src).is_ok(),
                "legitimate map-prefix form must still lex: {}",
                src
            );
        }
        let t = toks("m := map[string]int{}");
        assert!(t.iter().any(|t| matches!(t, Tok::Ident(s) if s == "map")));
    }
}

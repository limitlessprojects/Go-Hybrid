//! The Hybrid AST — the cross-component contract between `hy-syntax`
//! (produces), `hy-check` (consumes), and the MVP-0 emitter (consumes).
//!
//! Scope: the surface exercised by tests/checker-corpus/*.hy (130 programs),
//! examples/ (ten target programs), and spikes/spike6-ownership/work/*.hy
//! (ten programs). Anything outside that surface is allowed to be
//! unimplemented by the parser — it must produce a clean `unsupported`
//! parse error, never a wrong AST.
//!
//! Design notes:
//!   * Spans are carried on every node (diagnostics point at real sites).
//!   * `Place` (assignable lvalue) is separate from `Expr`: `agg[w]++`,
//!     `stats.processed++`, `svc.tag = "x"` are places; the checker treats
//!     a place access as a read/write/use of every base binding.
//!   * `move` and `?` are expression forms (`Expr::Move`, `Expr::Try`) so
//!     they compose in argument/send positions (`results <- move agg`,
//!     `doc.array("users")?`), exactly as the corpus spells them.
//!   * Multi-value: `a, b := f()` / `return a, b` / `if v, ok := m[k]; ok`
//!     are list forms (T23: no tuples). A call used as a single value is a
//!     type error (HYB-E2002), checked later — not an AST concern.
//!   * String interpolation: `Interp { parts }` — lexer/parser must support
//!     nested quotes inside interpolations (`"{m["fox"]}"`, corpus 19).

use hy_diag::Span;

// ---------------------------------------------------------------------------
// File + declarations
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct File {
    /// Source path as given on the command line (diagnostics carry it).
    pub path: String,
    pub decls: Vec<Decl>,
}

impl File {
    pub fn fns(&self) -> impl Iterator<Item = &FnDecl> {
        self.decls.iter().filter_map(|d| match d {
            Decl::Fn(f) => Some(f),
            _ => None,
        })
    }
}

#[derive(Debug, Clone)]
pub enum Decl {
    /// `import "sync"` / `import "rust/serde_json"`
    Import {
        path: String,
        span: Span,
    },
    /// `const n_workers = 4` (file-level or fn-local)
    Const(ConstDecl),
    Struct(StructDecl),
    Enum(EnumDecl),
    /// `type node_id = int` (example 08's arena-id discipline).
    TypeAlias(TypeAliasDecl),
    Fn(FnDecl),
}

#[derive(Debug, Clone)]
pub struct ConstDecl {
    pub name: String,
    /// Optional type ascription (`const n int = 4`).
    pub ty: Option<TypeExpr>,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct StructDecl {
    pub name: String,
    /// Generic type parameters: `struct lru_cache[k comparable, v any]`.
    pub type_params: Vec<TypeParam>,
    pub fields: Vec<FieldDecl>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct FieldDecl {
    pub name: String,
    pub ty: TypeExpr,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct EnumDecl {
    pub name: String,
    pub variants: Vec<EnumVariant>,
    pub span: Span,
}

/// `type node_id = int` — a transparent alias (Go `type X = Y`).
#[derive(Debug, Clone)]
pub struct TypeAliasDecl {
    pub name: String,
    pub ty: TypeExpr,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct EnumVariant {
    pub name: String,
    /// `payload: Option<TypeExpr>` — `some(T)` etc.
    pub payload: Option<TypeExpr>,
}

#[derive(Debug, Clone)]
pub struct FnDecl {
    /// `pub fn` — exported at package emission (example 09's adoption
    /// surface: M4's export mapping ValidateEmail). Contract extension
    /// (lead-owned, logged): the parser previously had no visibility flag.
    pub is_pub: bool,
    pub name: String,
    /// Method receiver: `fn (g *graph) connect(...)`.
    pub receiver: Option<Receiver>,
    /// Generic type parameters: `fn id[t any](x t) t` (examples 07 surface).
    pub type_params: Vec<TypeParam>,
    pub params: Vec<Param>,
    /// 0, 1, or 2 results (multi-value, never a tuple). Named results
    /// (`fn main() (code int)`, example 02) carry their name.
    pub results: Vec<FnResult>,
    /// `None` = declaration without body (not expected in corpus files).
    pub body: Option<Block>,
    pub span: Span,
}

/// A function result: type + optional name (`(code int)`).
#[derive(Debug, Clone)]
pub struct FnResult {
    pub name: Option<String>,
    pub ty: TypeExpr,
}

/// A generic type parameter: `[k comparable, v any]`.
#[derive(Debug, Clone)]
pub struct TypeParam {
    pub name: String,
    /// Constraint rendered as written (`comparable`, `any`, ...).
    pub constraint: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Receiver {
    pub name: String,
    pub ty: TypeExpr,
    pub is_pointer: bool,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: TypeExpr,
    pub span: Span,
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum TypeExpr {
    /// `int`, `bool`, `string`, `error` — or qualified foreign:
    /// `serde_json.value`, `sync.mutex`, `atomic.int64`, `http.request`.
    /// (Dotted = foreign/Go-side package path; the checker resolves these.)
    Named { name: String, span: Span },
    /// `*T` — pointer (params like `stats *pool_stats`).
    Pointer { to: Box<TypeExpr>, span: Span },
    /// `[]T`
    Slice { elem: Box<TypeExpr>, span: Span },
    /// `[N]T` (fixed array; `len` may be an expression or None for `[...]`)
    Array {
        len: Option<Box<Expr>>,
        elem: Box<TypeExpr>,
        span: Span,
    },
    /// `map[K]V`
    Map {
        key: Box<TypeExpr>,
        value: Box<TypeExpr>,
        span: Span,
    },
    /// `chan T` / `<-chan T` / `chan<- T`
    Chan {
        dir: ChanDir,
        elem: Box<TypeExpr>,
        span: Span,
    },
    /// `option[T]`
    Option { elem: Box<TypeExpr>, span: Span },
    /// Function type for fn-literal params: `fn(w http.response_writer) {...}`
    /// — used only inside `FnLit`; a standalone named fn type is not in the
    /// corpus surface.
    Fn {
        params: Vec<Param>,
        results: Vec<TypeExpr>,
        span: Span,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChanDir {
    Both,
    Recv, // <-chan T
    Send, // chan<- T
}

impl TypeExpr {
    pub fn span(&self) -> Span {
        match self {
            TypeExpr::Named { span, .. }
            | TypeExpr::Pointer { span, .. }
            | TypeExpr::Slice { span, .. }
            | TypeExpr::Array { span, .. }
            | TypeExpr::Map { span, .. }
            | TypeExpr::Chan { span, .. }
            | TypeExpr::Option { span, .. }
            | TypeExpr::Fn { span, .. } => *span,
        }
    }

    /// Human-readable rendering for diagnostics (`map[string]int`).
    pub fn render(&self) -> String {
        match self {
            TypeExpr::Named { name, .. } => name.clone(),
            TypeExpr::Pointer { to, .. } => format!("*{}", to.render()),
            TypeExpr::Slice { elem, .. } => format!("[]{}", elem.render()),
            TypeExpr::Array { len, elem, .. } => match len {
                Some(e) => format!("[{}]{}", e.render(), elem.render()),
                None => format!("[...]{}", elem.render()),
            },
            TypeExpr::Map { key, value, .. } => {
                format!("map[{}]{}", key.render(), value.render())
            }
            TypeExpr::Chan { dir, elem, .. } => match dir {
                ChanDir::Both => format!("chan {}", elem.render()),
                ChanDir::Recv => format!("<-chan {}", elem.render()),
                ChanDir::Send => format!("chan<- {}", elem.render()),
            },
            TypeExpr::Option { elem, .. } => format!("option[{}]", elem.render()),
            TypeExpr::Fn { .. } => "fn".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Statements
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    /// Span of the whole block (for managed-block diagnostics etc.).
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum Stmt {
    /// All local binding forms:
    ///   `size := (len(words)+n-1)/n`      (Define, names=1, values=1)
    ///   `let langs = ["Go","Rust"]`       (Let — immutable binding)
    ///   `var above int`                    (Var, ty=Some, values empty)
    ///   `var hits atomic.int64`            (Var, ty=Some)
    ///   `var got []int`                    (Var, ty=Some)
    ///   `p, f := a, b`                     (Define, names=2)
    ///   `code, body := httptest.get(...)?` (Define, values=1 with Try)
    ///   `const n_workers = 4`              (Const — fn-local, spike-6 03)
    Local(LocalDecl),
    /// Assignment/inc-dec through places:
    ///   `agg[w]++` / `count += 1` / `m2["total"] = len(m2)` /
    ///   `rows = append(rows, row{...})` / `stats.processed++`
    Assign {
        targets: Vec<Place>,
        op: AssignOp,
        values: Vec<Expr>,
        span: Span,
    },
    /// Expression statement: calls, bare receives (`<-done`).
    Expr {
        expr: Expr,
        span: Span,
    },
    /// Channel send: `results <- move agg`, `jobs <- url`, `done <- true`.
    /// `value: None` = bare send `<-ch`? (not in corpus; receive covers it).
    Send {
        chan: Expr,
        value: Expr,
        is_move: bool,
        span: Span,
    },
    /// `if ok { } else { }` / `if v, ok := m[k]; ok { }` / `else if`
    If {
        init: Option<Box<Stmt>>,
        cond: Expr,
        then: Block,
        otherwise: Option<Box<Stmt>>,
        span: Span,
    },
    /// All loop forms (see `ForKind`).
    For {
        kind: ForKind,
        body: Block,
        span: Span,
    },
    /// `return` / `return a, b` / `return &reg.counters[i]`
    Return {
        values: Vec<Expr>,
        span: Span,
    },
    /// `task tally(move chunk, counts, done)` — THE spawn check-site.
    Task {
        callee: Expr,
        args: Vec<CallArg>,
        span: Span,
    },
    /// `managed("reason", "category", tickets...) { ... }` — the escape
    /// hatch (O6, hardened by row 14 / discussion #4). The payload is
    /// REQUIRED and fail-closed: a bare block is HYB-E0003, an unknown
    /// category is HYB-E0004. Checker: sites inside are permitted + linted;
    /// values born inside are managed-tainted. The reason/category/tickets
    /// flow into --report-managed and the emitted Go's marker comment.
    Managed {
        reason: String,
        category: String,
        tickets: Vec<String>,
        body: Block,
        span: Span,
    },
    /// `match x { pattern => body }` (spec §2.5; corpus-stretch)
    Match {
        subject: Expr,
        arms: Vec<MatchArm>,
        span: Span,
    },
    /// `defer f.close()?`
    Defer {
        call: Expr,
        span: Span,
    },
    /// `break` / `break loop` / `break <label>` (example 04: breaking the
    /// LOOP from inside a select — the label is representable).
    Break {
        span: Span,
        label: Option<String>,
    },
    Continue {
        span: Span,
        label: Option<String>,
    },
    /// Bare nested block `{ ... }`
    Block {
        block: Block,
        span: Span,
    },
    /// `select { ... }` (examples 04 surface; corpus-stretch)
    Select {
        arms: Vec<SelectArm>,
        span: Span,
    },
}

#[derive(Debug, Clone)]
pub struct LocalDecl {
    pub kind: LocalKind,
    pub names: Vec<String>,
    /// Type ascription for `var`/`let`/`const`.
    pub ty: Option<TypeExpr>,
    /// 0 (no initializer), 1, or N (multi-value define from one call).
    pub values: Vec<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalKind {
    /// `:=`
    Define,
    /// `let x = e` (immutable; spike-6 also writes `let x := e` — the parser
    /// normalizes BOTH to Define + immutable flag, per D14)
    Let,
    /// `var x T` / `var x T = e`
    Var,
    /// fn-local `const x = e`
    Const,
}

impl LocalKind {
    pub fn is_immutable(&self) -> bool {
        matches!(self, LocalKind::Let | LocalKind::Const)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Assign,      // =
    AddAssign,   // +=
    SubAssign,   // -=
    MulAssign,   // *=
    DivAssign,   // /=
    ModAssign,   // %=
    Inc,         // ++ (values empty)
    Dec,         // -- (values empty)
    BitOrAssign,   // |=
    BitAndAssign,  // &=
    BitXorAssign,  // ^=
    ShlAssign,     // <<=
    ShrAssign,     // >>=
}

/// Assignable lvalue.
#[derive(Debug, Clone)]
pub struct Place {
    pub kind: PlaceKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum PlaceKind {
    Ident(String),
    /// `agg[w]`, `e.services[fresh]`
    Index {
        base: Box<Place>,
        index: Expr,
    },
    /// `stats.processed`, `svc.tag`, `stats.mu`
    Field {
        base: Box<Place>,
        name: String,
    },
    /// `&stats`, `&hits`, `&reg.counters[i]` — address-of in place/arg
    /// position (the parser may also yield Expr::Unary{Addr} there).
    AddrOf {
        base: Box<Place>,
    },
}

impl Place {
    /// The root binding name this place touches (`agg[w]` → "agg").
    pub fn root(&self) -> &str {
        match &self.kind {
            PlaceKind::Ident(n) => n,
            PlaceKind::Index { base, .. } => base.root(),
            PlaceKind::Field { base, .. } => base.root(),
            PlaceKind::AddrOf { base } => base.root(),
        }
    }

    /// True if the place is exactly the plain binding `name`.
    pub fn is_ident(&self, name: &str) -> bool {
        matches!(&self.kind, PlaceKind::Ident(n) if n == name)
    }

    pub fn render(&self) -> String {
        match &self.kind {
            PlaceKind::Ident(n) => n.clone(),
            PlaceKind::Index { base, .. } => format!("{}[...]", base.render()),
            PlaceKind::Field { base, name } => format!("{}.{}", base.render(), name),
            PlaceKind::AddrOf { base } => format!("&{}", base.render()),
        }
    }
}

#[derive(Debug, Clone)]
pub enum ForKind {
    /// `for { }` / `loop { }`
    Infinite,
    /// `for len(got) < len(inputs) { }`
    While { cond: Expr },
    /// `for i in 0..n_shards { }` (exclusive) / `for i in 1..=8 { }`
    /// (inclusive — the `..=` flag is representable, example 04).
    Range {
        var: String,
        lo: Expr,
        hi: Expr,
        inclusive: bool,
    },
    /// `for w in shard { }` / `for i, l in langs { }` / `for w, n in m { }`
    /// / `for _, url in seed_urls { }` — vars[0] is the value or index.
    Iterate { vars: Vec<String>, expr: Expr },
}

// ---------------------------------------------------------------------------
// Expressions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum Expr {
    Ident {
        name: String,
        span: Span,
    },
    Lit {
        kind: LitKind,
        span: Span,
    },
    /// Interpolated string: `"count={count} done"` — parts are literal
    /// chunks and embedded expressions (which may contain nested strings:
    /// `"{m["fox"]}"`).
    Interp {
        parts: Vec<InterpPart>,
        span: Span,
    },
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
        span: Span,
    },
    Unary {
        op: UnOp,
        operand: Box<Expr>,
        span: Span,
    },
    /// `&stats`, `&reg.counters[i]`, `&hits` — address-of. Synonymous with
    /// `Unary { Addr }`; both spellings are legal AST (parser picks one —
    /// the checker handles both).
    AddrOf {
        operand: Box<Expr>,
        span: Span,
    },
    /// `move agg` — ownership transfer marker (O5). Legal in spawn/send
    /// argument position and (later) assignment/return.
    Move {
        operand: Box<Expr>,
        span: Span,
    },
    /// Postfix `?` — error propagation (E-rules): `doc.array("users")?`.
    Try {
        operand: Box<Expr>,
        span: Span,
    },
    /// `<-results` (expression form; as a statement it is Expr{Receive}).
    Receive {
        chan: Box<Expr>,
        span: Span,
    },
    /// `agg[w]`, `thresholds[3]`
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
        span: Span,
    },
    /// `words[a:b]`, `payload[i*32 : i*32+32]` (lo/hi optional)
    Slice {
        base: Box<Expr>,
        lo: Option<Box<Expr>>,
        hi: Option<Box<Expr>>,
        span: Span,
    },
    /// `stats.processed`, `doc.array`, `fmt.println`, `serde_json.parse`
    Field {
        base: Box<Expr>,
        name: String,
        span: Span,
    },
    /// `f(x)`, `fmt.println(...)`, `doc.set("k", true)`, `min(a, b)`
    Call {
        callee: Box<Expr>,
        args: Vec<CallArg>,
        span: Span,
    },
    /// `pool_stats{}`, `sample{probe: id, above: above}`,
    /// `{replicas: 2, tag: "prod"}` (type-elided, inside a typed context)
    StructLit {
        ty: Option<TypeExpr>,
        fields: Vec<FieldInit>,
        span: Span,
    },
    /// `[]string{}` (elem Some, no items) / `["a","b"]` / `[c1, c2]` (elem None)
    SliceLit {
        elem: Option<Box<TypeExpr>>,
        items: Vec<Expr>,
        span: Span,
    },
    /// `map[string]int{}` / `map[string]service{"frontend": {...}, ...}`
    MapLit {
        key: Box<TypeExpr>,
        value: Box<TypeExpr>,
        entries: Vec<(Expr, Expr)>,
        span: Span,
    },
    /// `fn(w http.response_writer, r *http.request) { ... }` — closure
    /// literal (corpus 10: http.handler capture — the A5 perimeter shape)
    FnLit {
        params: Vec<Param>,
        /// Declared results (`fn() int {…}`) — SPEC EBNF: `FuncLit =
        /// "fn" [Signature] Block`. Empty for the untyped form.
        results: Vec<FnResult>,
        body: Block,
        span: Span,
    },
    /// Parenthesized expression (kept for faithful spans).
    Paren {
        inner: Box<Expr>,
        span: Span,
    },
}

#[derive(Debug, Clone)]
pub enum LitKind {
    Int(String),
    Float(String),
    Bool(bool),
    /// Cooked string (quotes stripped, escapes resolved by the lexer).
    Str(String),
    /// Raw string (backticks; content verbatim).
    RawStr(String),
    Char(char),
    /// `nil` (legal only for error + foreign perimeter — HYB-E4001 otherwise)
    Nil,
}

#[derive(Debug, Clone)]
pub enum InterpPart {
    Lit(String),
    Expr(Expr),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,
    // Bit-level ops (SPEC-LANGUAGE 1.4: AddOp `|` `^`, MulOp `&` `<<` `>>`).
    BitOr,
    BitXor,
    BitAnd,
    Shl,
    Shr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Not,
    Neg,
    /// `&x` — also spelled `Expr::AddrOf` (both are legal AST).
    Addr,
}

impl UnOp {
    pub fn render(&self) -> &'static str {
        match self {
            UnOp::Not => "!",
            UnOp::Neg => "-",
            UnOp::Addr => "&",
        }
    }
}

#[derive(Debug, Clone)]
pub struct CallArg {
    /// `seed=7` — a NAMED argument (example 10's surface). Go has no named
    /// args: the emitter places named args by the binding descriptor's
    /// param order (rust-call surface only; positional stays positional).
    /// Contract extension (lead-owned, logged per interfaces/ discipline):
    /// the parser previously DROPPED the name.
    pub name: Option<String>,
    pub value: Expr,
    /// `words[a:b]...` — variadic spread.
    pub spread: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct FieldInit {
    pub name: String,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct MatchArm {
    /// Pattern (spec §2.5): ident, variant path, literal, or `_`.
    pub pattern: MatchPattern,
    /// Optional guard `if cond`.
    pub guard: Option<Expr>,
    pub body: Block,
}

#[derive(Debug, Clone)]
pub enum MatchPattern {
    Wildcard,
    /// `shape.circle` / `ok` / `some(x)` (with binding)
    Path {
        segments: Vec<String>,
        binding: Option<String>,
    },
    Lit(LitKind),
}

#[derive(Debug, Clone)]
pub struct SelectArm {
    /// `ch <- v` (send) or `v := <-ch` (recv) or `default`
    pub kind: SelectArmKind,
    pub body: Block,
}

#[derive(Debug, Clone)]
pub enum SelectArmKind {
    /// C5: a select send clause (`ch <- v:`) is a sendability check-site
    /// exactly like a statement send. `is_move` carries the parser's
    /// `ch <- move v` marker for the very same reason `Stmt::Send` carries
    /// it: the owned-move route (O4 class 3) must be available here, and the
    /// freeze (O5.2) must be recorded for the moved binding.
    Send {
        chan: Expr,
        value: Expr,
        is_move: bool,
    },
    Recv {
        names: Vec<String>,
        chan: Expr,
    },
    Default,
}

// ---------------------------------------------------------------------------
// Walk helpers (the checker's traversal foundation)
// ---------------------------------------------------------------------------

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Expr::Ident { span, .. }
            | Expr::Lit { span, .. }
            | Expr::Interp { span, .. }
            | Expr::Binary { span, .. }
            | Expr::Unary { span, .. }
            | Expr::AddrOf { span, .. }
            | Expr::Move { span, .. }
            | Expr::Try { span, .. }
            | Expr::Receive { span, .. }
            | Expr::Index { span, .. }
            | Expr::Slice { span, .. }
            | Expr::Field { span, .. }
            | Expr::Call { span, .. }
            | Expr::StructLit { span, .. }
            | Expr::SliceLit { span, .. }
            | Expr::MapLit { span, .. }
            | Expr::FnLit { span, .. }
            | Expr::Paren { span, .. } => *span,
        }
    }

    /// The root identifier of an access chain, if any
    /// (`stats.mu.lock` → "stats"; `agg` → "agg"; literals → None).
    pub fn root_ident(&self) -> Option<&str> {
        match self {
            Expr::Ident { name, .. } => Some(name),
            Expr::Field { base, .. } => base.root_ident(),
            Expr::Index { base, .. } => base.root_ident(),
            Expr::Slice { base, .. } => base.root_ident(),
            Expr::AddrOf { operand, .. } => operand.root_ident(),
            Expr::Unary {
                op: UnOp::Addr,
                operand,
                ..
            } => operand.root_ident(),
            Expr::Paren { inner, .. } => inner.root_ident(),
            Expr::Try { operand, .. } => operand.root_ident(),
            _ => None,
        }
    }

    /// If this `SliceLit` represents a typed array composite literal
    /// (`[N]T{…}` or `[...]T{…}`), returns `(len, inner_elem, array_ty)`.
    /// Distinguished from a slice-of-arrays literal `[][N]T{…}` by the
    /// array `TypeExpr` sharing the literal's opening `[` source position.
    pub fn as_array_lit(&self) -> Option<(Option<&Expr>, &TypeExpr, &TypeExpr)> {
        if let Expr::SliceLit {
            elem: Some(t),
            span,
            ..
        } = self
        {
            if let TypeExpr::Array {
                len,
                elem,
                span: ty_span,
            } = t.as_ref()
            {
                if ty_span.start == span.start {
                    return Some((len.as_deref(), elem.as_ref(), t.as_ref()));
                }
            }
        }
        None
    }

    /// Render for diagnostics (short, lossy, never panics).
    pub fn render(&self) -> String {
        match self {
            Expr::Ident { name, .. } => name.clone(),
            Expr::Lit { kind, .. } => match kind {
                LitKind::Int(s) | LitKind::Float(s) => s.clone(),
                LitKind::Bool(b) => b.to_string(),
                LitKind::Str(s) | LitKind::RawStr(s) => format!("{:?}", s),
                LitKind::Char(c) => format!("{:?}", c),
                LitKind::Nil => "nil".to_string(),
            },
            Expr::Interp { .. } => "\"...{...}\"".to_string(),
            Expr::Binary { op, lhs, rhs, .. } => {
                format!("{} {} {}", lhs.render(), op.render(), rhs.render())
            }
            Expr::Unary { op, operand, .. } => format!("{}{}", op.render(), operand.render()),
            Expr::AddrOf { operand, .. } => format!("&{}", operand.render()),
            Expr::Move { operand, .. } => format!("move {}", operand.render()),
            Expr::Try { operand, .. } => format!("{}?", operand.render()),
            Expr::Receive { chan, .. } => format!("<-{}", chan.render()),
            Expr::Index { base, .. } => format!("{}[...]", base.render()),
            Expr::Slice { base, .. } => format!("{}[...:...]", base.render()),
            Expr::Field { base, name, .. } => format!("{}.{}", base.render(), name),
            Expr::Call { callee, .. } => format!("{}(...)", callee.render()),
            Expr::StructLit { ty, .. } => match ty {
                Some(t) => format!("{}{{...}}", t.render()),
                None => "{...}".to_string(),
            },
            Expr::SliceLit { .. } => {
                if let Some((_, _, arr_ty)) = self.as_array_lit() {
                    format!("{}{{...}}", arr_ty.render())
                } else {
                    "[...]".to_string()
                }
            }
            Expr::MapLit { key, value, .. } => {
                format!("map[{}]{}{{...}}", key.render(), value.render())
            }
            Expr::FnLit { .. } => "fn(...) {...}".to_string(),
            Expr::Paren { inner, .. } => format!("({})", inner.render()),
        }
    }
}

impl BinOp {
    pub fn render(&self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Mod => "%",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Gt => ">",
            BinOp::Le => "<=",
            BinOp::Ge => ">=",
            BinOp::And => "&&",
            BinOp::Or => "||",
            BinOp::BitOr => "|",
            BinOp::BitXor => "^",
            BinOp::BitAnd => "&",
            BinOp::Shl => "<<",
            BinOp::Shr => ">>",
        }
    }
}

impl Stmt {
    pub fn span(&self) -> Span {
        match self {
            Stmt::Local(l) => l.span,
            Stmt::Assign { span, .. }
            | Stmt::Expr { span, .. }
            | Stmt::Send { span, .. }
            | Stmt::If { span, .. }
            | Stmt::For { span, .. }
            | Stmt::Return { span, .. }
            | Stmt::Task { span, .. }
            | Stmt::Managed { span, .. }
            | Stmt::Match { span, .. }
            | Stmt::Defer { span, .. }
            | Stmt::Break { span, .. }
            | Stmt::Continue { span, .. }
            | Stmt::Block { span, .. }
            | Stmt::Select { span, .. } => *span,
        }
    }
}

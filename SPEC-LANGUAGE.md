# SPEC-LANGUAGE.md — The Hybrid Language Specification

**The pitch this spec serves** (research/positioning.md, human-supplied):
> "Keep Go exactly as it is. Add Rust where you need it."

Type-safety features are table stakes ordered by the r/golang wishlist — (1) enums +
exhaustiveness, (2) null safety, (3) sum types + pattern matching, (4) immutability,
(5) Option/Result. The differentiator is invisible Rust interop. Go's compile speed
and simplicity are never traded away. Syntax is Go-shaped and **explicitly not Rust
syntax** (§0.2 item 7).

---

## 0. Front matter

### 0.1 Status

| field | value |
|---|---|
| Document | `SPEC-LANGUAGE.md` — normative language specification, **FIRST DRAFT** |
| Date | 2026-10-12, Phase 1 (G0 passed the same day; all six spikes green, both tiers) |
| Supersedes | nothing — first draft. The informal priors are `examples/*/program.hy` (ten sketches) and `spikes/spike6-ownership/work/*.hy` (the Tier-0-corpus rewrite of those ten) |
| Superseded by | revision 2, after the adversarial checker corpus (`tests/checker-corpus/`, being written in parallel) and the MVP-0 walking skeleton land; reviewed at G1 |
| Authority chain | SPEC.md §8 mandates this file; PLAN.md "Design Decisions in Force" bind it; QUESTIONS.md Q1 (resolved 2026-10-12) binds §9 normatively; `interfaces/abi-types.md` binds §12; `research/spike-report.md` is the Phase-0 design this spec must stay consistent with |
| Verification level | **nothing here is compiler-verified yet — no compiler exists.** Rules are normative intent; each acceptance/rejection behavior will be pinned by the checker corpus, the reference interpreter (SPEC §14.4), and compile-fail tests (SPEC §10) |

### 0.2 How to read this document

1. **Rule format.** Every normative rule carries a chapter-scoped id (`LX4`, `T7`,
   `O4` …), states the rule in one or two sentences, and is followed by a one-line
   example. Load-bearing rules add **Why** (one sentence) and **Reversal** (what it
   costs to undo).
2. **Decision tags.**
   - `[decided: <reason>]` — the source documents are silent; resolved here using
     SPEC.md §9 rule 3's ordering: *lower risk to MVP → memory safety → Go
     compatibility → lower runtime overhead → simplicity* (in that priority).
   - `[TBD]` — genuinely undecided. Precision is never faked: a `[TBD]` names the
     artifact that will settle it (see §13.2).
   - **PROVISIONAL** — a ruling that is *reversible by design*: the alternatives
     and their rejection reasons are retained in this document so a reviewer can
     reverse cheaply (QUESTIONS.md Q1 requirement). Currently exactly one
     PROVISIONAL ruling exists: §9.4 (A1 sendability).
3. **Examples are tests.** Extraction is marker-driven (SPEC.md §14.4): a code
   block containing `// expect: …` lines is an executable test-claim the
   Phase-1 CI extractor runs and diffs against stdout; a block containing
   `// reject:` lines is a compile-fail claim (the stated diagnostic code must
   be emitted); a block containing `// panics:` lines is a runnable
   panic-contract claim. Blocks marked `// fragment:` — and any block with
   none of the three markers — are illustrations, not tests. A code block's
   `// expect:` output is part of the rule it demonstrates.
4. **Syntax style.** Examples follow the `.hy` house style established by the ten
   spike-6 programs: Go declaration order and expression syntax, `:=` short
   declarations, `struct`/`match`/`task`/`move`/`managed` keywords, snake_case
   identifiers, `{expr}` string interpolation, no semicolons. The corpus is law:
   the ten `work/*.hy` files become the checker's first positive tests, so this
   spec codifies their style rather than inventing a new one. The three
   deliberate divergences from Go surface syntax are recorded in §1.1 (LX4).
5. **Precedence.** SPEC.md → QUESTIONS.md rulings → PLAN.md Design Decisions in
   Force → spike RESULTS.md → this document's `[decided]` tags. This file
   knowingly contradicts none of them; a contradiction found later is a bug in
   *this file*, and gets logged as such.
6. **Honesty rules (SPEC §14.7 apply to this document).** Spike-6's 10/10
   zero-annotation result is *hand-traced, believed-level evidence* (RESULTS.md
   "Verified / believed / unknown"). Where a rule rests on it, the rule says so.
   Where a number is quoted (2.42 ns crossing, 0-alloc error path), it is a
   measured Phase-0 spike number, not a language guarantee; language-level
   budgets live in SPEC.md §6.
7. **The not-Rust list.** Hybrid syntax must feel like Go. The following Rust
   shapes are normatively absent and must stay absent unless a logged decision
   reverses it: `impl` blocks, `::` path separators, `'lifetime` annotations,
   `<T>` angle-bracket generics, `&mut` borrows, `=>` match arms, `Option<T>` /
   `Result<T, E>` CamelCase spellings, `mut` parameters, `as` casts
   (conversions are Go-style `T(x)`), macros of any kind (also a compile-speed
   rule: no unbounded expansion before type checking).

### 0.3 The provisional-decision banner

> ⚠️ **PROVISIONAL DECISIONS IN FORCE.** The Tier-0 sendability ruling (§9.4,
> resolving Spike-6 ambiguity **A1**) is **strict (c)+(b)** — safe-by-type *plus*
> freeze-after-send — adopted under the human's Q1 ruling of 2026-10-12 and
> marked **PROVISIONAL** with the three rejected alternatives and their rejection
> reasons retained in §9.4's table, so the decision can be reversed cheaply and
> deliberately. Two tripwires guard it (§9.7): if more than ~25% of the
> adversarial corpus needs `managed`, or reviewers repeatedly hit the same
> rejection on idiomatic Go patterns, G0 reopens with that table in hand. The
> number 25% is the lead's concretization of the human's "~X%" and is recorded
> as tunable. Until the adversarial corpus exists, this document treats Spike
> 6's acceptance set as *necessary but not sufficient* evidence (Q1 addition 2:
> the ten programs were chosen by us; selection bias is built in).

### 0.4 Table of contents

- §1 Lexical structure and grammar (EBNF skeleton)
- §2 Types (priority order: enums → null safety → sum types → immutability → Option)
- §3 Zero values and `nil`
- §4 Generics
- §5 Interfaces and traits
- §6 Error handling and the `try` operator (`?`)
- §7 `defer`, `panic`, `recover` — and the boundary rule
- §8 Concurrency: tasks, channels, `select`
- §9 Ownership, borrowing, and the safe/unsafe boundary (Tier 0 — the A1–A8 chapter)
- §10 Modules, visibility, imports, versioning
- §11 Integer overflow, bounds checking, numeric conversions
- §12 Memory layout guarantees for boundary-crossing types
- §13 Spec-examples-are-tests, open decisions, pointers

---

## 1. Lexical structure and grammar (EBNF skeleton)

This chapter is a **skeleton**: productions are given at the level needed to
write the ten target programs and the checker corpus, with `[TBD]` where a
detail is genuinely open. The parser (Phase 1) implements exactly this and
fails closed on anything else — unspecified syntax is rejected, not guessed.

### 1.1 Lexical rules

**LX1 — Source encoding.** Source files are UTF-8, one `package` per file, files
grouped by directory into one Hybrid package (Go's layout rule, kept so the
emitted Go tree mirrors the Hybrid tree).

**LX2 — Identifiers.** `[a-z_][a-z0-9_]*` — lowercase snake_case only.
`CapsIdent` and `camelCase` are lexical errors in Hybrid-declared names
(`HYB-E0001`); foreign Go/Rust names are *normalized* at binding time (M4), so
uppercase never appears in Hybrid source.

```hy
// fragment:
worker_id  := 3        // ok
WorkerID   := 3        // reject: HYB-E0001 — Hybrid identifiers are snake_case; write `worker_id`
```

**Why:** one convention, `pub` (LX5) is the sole visibility marker, and the
grep-ability rules in §9.7 get a uniform namespace. **Reversal:** mechanical
(rewrite pass); cheap while pre-1.0, and the corpus pins whichever form is
chosen at MVP-0. [decided: corpus style + single convention beats Go's
caps-as-visibility, which conflicts with `pub`]

**LX3 — Keywords (reserved).**

```
break  chan  const  continue  defer  else  enum  false  fn  for  if
import  in  let  loop  managed  match  move  package  pub  return
select  struct  task  true  type  var
```

Reserved for future use (error to use today): `unsafe`, `go`, `func`,
`interface`, `trait`, `where`, `use`, `mod`, `as`, `mut` (a Rust-ism this
language deliberately does not take; immutability is binding-level `let`,
T22). Predeclared identifiers (not
keywords, shadowable with a lint): `nil`, `error`, `option`, `set`, `some`,
`none`, `int`, `uint`, `byte`, `rune`, `string`, `bool`,
`f32`, `f64`, `u8`…`u64`, `i8`…`i64`, `len`, `cap`, `append`, `delete`,
`make`, `copy`, `close`, `panic`, `recover`.

**LX4 — The three house divergences from Go surface syntax**, recorded so
nobody "fixes" them by accident (all three appear throughout the spike-6
corpus):

| # | Hybrid | Go | note |
|---|---|---|---|
| 1 | `fn` | `func` | shorter keyword; identical declaration shape (name, params, results). Also the closure-literal introducer (`fn(w, r) { … }`) |
| 2 | `pub` | exported-by-Capitalization | visibility is a keyword, not a casing convention (LX2 forbids caps anyway) |
| 3 | `match` with `pat:` arms | `switch` with `case` | sum types need destructuring arms; `switch` remains available for value comparison TBD §1.5 |

Everything else — `:=`, `var`, `const`, `type`, `struct`, `if`/`else`,
`for`, `select`, `defer`, `panic`, `import`, package layout, operator set
and precedence — is Go's, carried over deliberately.

**LX5 — Visibility.** `pub` marks a top-level declaration as exported from its
package; everything else is package-private. Go-export mapping in §10.

**LX6 — Literals.** Integer: decimal, `0x`/`0o`/`0b` prefixed, `_` digit
separators (`1_000_000`). Float: `1.5`, `2.`, `.5`, `1e9`. Rune: `'a'`,
`'\n'`, `'\u{1F600}'` (type `rune` = i32 code point). Boolean: `true`,
`false`. Untyped constants follow Go's rules (default type on first use).

**LX7 — String literals.** Interpreted `"…"`: Go escape set **plus**
interpolation `{expr}` (T4); `{{` and `}}` escape literal braces. Raw
`` `…` ``: no escapes, **no interpolation**. Strings are always UTF-8 (T3).
Malformed escape sequences — a non-hex digit in `\x`/`\u`/`\U`, the
`\u{…}` rune form inside a string, a surrogate or a value above U+10FFFF —
are `HYB-E0002` parse errors (rule `LX7`; the rune-literal forms carry
`LX6`): the escape set is enforced fail-closed, never silently skipped
with the remaining characters leaking into the value (corpus 61).

```hy
name := "Hybrid"
fmt.println("hello, {name}!")     // expect: hello, Hybrid!
fmt.println("brace: {{}}")        // expect: brace: {}
pat  := `^[^@\s]+$`               // raw: backslashes literal, no interpolation
```

**LX8 — Comments.** `//` line comments and non-nested `/* … */` block
comments, Go semantics.

**LX9 — Semicolons.** None are written. Statements are line-terminated;
Go's insertion rules apply at the lexer level for the emitted Go, and the
Hybrid parser is newline-sensitive where two statements would otherwise
merge. Exact rule `[TBD]` — pinned by the parser's golden tests, not invented
here.

**LX10 — The new constructs** (SPEC §8's list, mapped to their grammar
homes): `enum` + `match` (§1.2, §2.5), `option[T]` (§2.6), `?` (§1.4, §6),
`move` (§1.4, §9.5), `managed` (§1.3, §9.6). `Result` has **no** grammar
production: the error model is Go-shaped `(T, error)` + `?` (E10 records the
rejection of a first-class `result[T, E]`).

### 1.2 Grammar — declarations

EBNF: `=` defines, `{x}` repetition, `[x]` optional, `a | b` alternative,
terminals in `"quotes"`, `.` ends a production.

```ebnf
SourceFile   = PackageClause { ImportDecl } { TopLevelDecl } .
PackageClause = "package" PackageName .

ImportDecl   = "import" ( ImportSpec | "(" { ImportSpec } ")" ) .
ImportSpec   = ImportPath .                          // "net/http" | "rust/serde_json"  (M10)

TopLevelDecl = ConstDecl | TypeDecl | StructDecl | EnumDecl
             | InterfaceDecl | FnDecl .

ConstDecl    = "pub"? "const" Identifier [ Type ] "=" Expression .
TypeDecl     = "pub"? "type" Identifier "=" Type .   // alias only in MVP (T25)

StructDecl   = "pub"? "struct" Identifier [ TypeParams ] "{" { FieldDecl } "}" .
FieldDecl    = Identifier Type .

EnumDecl     = "pub"? "enum" Identifier [ TypeParams ] "{" { EnumVariant } "}" .
EnumVariant  = Identifier [ "=" IntLiteral ] [ "(" TypeList ")" ] .   // T7–T10

InterfaceDecl= "pub"? "interface" Identifier [ TypeParams ] "{" { MethodSpec } "}" .
MethodSpec   = Identifier Signature .

FnDecl       = "pub"? "fn" ( AssocFn | Method ) Signature [ Block ] .
AssocFn      = TypeName "." Identifier .             // fn lru_cache.new[k any](…) …
Method       = "(" [ "*" ] Identifier TypeName ")" Identifier .   // fn (c *cache) get(…)

Signature    = [ TypeParams ] "(" [ ParamList ] ")" [ Result ] .
Result       = Type | "(" [ ParamList ] ")" .        // (string, error) etc.
ParamList    = Parameter { "," Parameter } [ "," ] .
Parameter    = [ Identifier ] Type .                 // typed: (r io.reader) | inferred in closures (LX13)
TypeParams   = "[" TypeParam { "," TypeParam } "]" .
TypeParam    = Identifier Constraint .               // [k comparable, v any]  (G1)
```

Undefined terminals above (`PackageName`, `ImportPath`, `IdentList`,
`Constraint`, `TypeList`, `Literal`, `AssignOp` operands) carry their
conventional meanings; the lexer/parser's full grammar lands with the parser
and its golden tests — this skeleton's job is to fix the *shape*, not every
production.

**LX11 — Two function forms.** Associated functions (`fn Type.name(…)`) have
no receiver; methods (`fn (r Type) name(…)`) take a receiver. `*T` receivers
are unique-write loans, `T` receivers are value copies (O3, O9). This is
Go's method model with `fn` spelling; the corpus uses both forms
(`fn greeting.build(…)` / `fn (g *greeting) bump()`).

**LX12 — Results may be named** (`(total int, err error)`), Go semantics:
named results are pre-initialized to their zero values (§3) and a bare
`return` returns them.

### 1.3 Grammar — statements

```ebnf
Block         = "{" { Statement } "}" .

Statement     = Declaration | SimpleStmt | IfStmt | ForStmt | LoopStmt
              | MatchStmt | SelectStmt | TaskStmt | DeferStmt | PanicStmt
              | ReturnStmt | BreakStmt | ContinueStmt | Block .

Declaration   = ConstDecl | VarDecl | LetDecl .
VarDecl       = "var" Identifier Type [ "=" Expression ] .
LetDecl       = "let" Identifier "=" Expression .    // immutable binding (T22)

SimpleStmt    = ShortVarDecl | Assignment | IncDecStmt | ExpressionStmt .
ShortVarDecl  = ExpressionList ":=" ExpressionList . // v, ok := m[k]
Assignment    = ExpressionList AssignOp ExpressionList .
AssignOp      = "=" | "+=" | "-=" | "|=" | "&=" | "^=" | "<<=" | ">>=" .
IncDecStmt    = Expression ( "++" | "--" ) .
ExpressionStmt= Expression .

IfStmt        = "if" [ SimpleStmt ";" ] Expression Block
              [ "else" ( IfStmt | Block ) ] .

ForStmt       = "for" ( RangeClause | Expression ) Block .       // cond or range
RangeClause   = [ IdentList ] "in" RangeExpr .                   // for i, v in xs
RangeExpr     = Expression ".." [ "=" ] Expression .             // 0..n  /  1..=8
LoopStmt      = "loop" Block .                                   // infinite loop (no bare `for {}`)

MatchStmt     = "match" Expression "{" { MatchArm } "}" .
MatchArm      = Pattern ":" ArmBody .
ArmBody       = Block | Statement .
Pattern       = VariantPattern | Literal | Identifier | "_" .
VariantPattern= [ TypeName "." ] Identifier [ "(" IdentList ")" ] .

SelectStmt    = "select" "{" { CommClause } "}" .
CommClause    = ( RecvClause | SendClause | "default" ) ":" ArmBody .
RecvClause    = [ ExpressionList ( ":=" | "=" ) ] RecvExpr .
SendClause    = Expression "<-" Expression .
RecvExpr     = "<-" Expression .

TaskStmt      = "task" Expression .                // a call: task worker(i, jobs, results)
DeferStmt     = "defer" Expression .               // a call; deferred CALL, not a loan (P1)
PanicStmt     = "panic" Expression .
ReturnStmt    = "return" [ ExpressionList ] .
BreakStmt     = "break" [ "loop" ] .               // `break loop` exits nearest `loop`
ContinueStmt  = "continue" .
```

**LX13 — Range clauses.** `for x in e` iterates slices, arrays, strings
(runes), maps (keys), channels (until close), and integer ranges
`for i in 0..n` / `for i in 1..=8` (inclusive). `for i, v in e` yields
index/element for slices/arrays/strings, key/value for maps; `for v in ch`
yields received values. Two-variable channel receive is the comma-ok
clause inside `select` (`v, ok := <-sq:`).

Integer range bounds are compared by their *emitted Go integer type*, not by
the spelling the author used. The SPEC spellings (`u8`, `i16`, `u32`, …) and
the Go builtins `byte` and `rune` denote the same types Go gives them
(`uint8`, `int16`, `uint32`, `int32`, …), so a range whose two bounds are
spelled differently but resolve to one Go type is legal: `for i in a..b` with
`a u8` and `b uint8` is accepted. Spellings that Go keeps distinct stay
distinct — `int`/`int64` and `uint`/`uintptr` never collapse, and the
`usize`/`isize` names are undefined by SPEC-LANGUAGE (§402) and are left
untouched. Untyped-bound defaulting and the constant-overflow checks are
unchanged. (Issue #1165.)
An inclusive range `lo..=hi` runs the body once for every integer from `lo`
through `hi`. The range variable is re-bound from that sequence on each
iteration, so a write to it inside the body cannot change how many times the
loop runs: `for i in 0..=n` with an `i += 1` in the body is the skip-next
idiom (the next iteration still gets `i + 1`), not a way to end the loop
early, and the sequence can never run past `hi` or wrap at the type maximum.
The half-open form `lo..hi` keeps its C-style lowering, where the body sees
and may steer the same variable the loop tests. (Issues #322, #2153.)

**LX13a — the iterable is TYPE-checked (HYB-E2009/E3005).** LX13's iterable
set is a closed contract: since issue #411, the checker rejects `for x in e`
when `e` resolves to a concrete non-iterable shape (an int, bool, struct, a
pointer to a struct/slice/string — anything outside the set above; a pointer
to an ARRAY stays rangeable, mirroring Go's range rule) with `HYB-E2009`; an
unresolvable operand fails closed under `HYB-E3005` (O4.9), exactly as the
send (#77) and receive (#409) sides treat an unknown operand. The
integer-range form `for i in 0..n` is a separate statement kind and carries
no iterable check.

**LX14 — `break`/`continue` scope.** They bind to the innermost `for`,
`loop`, or `select`; `break loop` exits the nearest enclosing `loop`
statement. General labels (Go's `L:`) are `[TBD]` — the corpus needs none
(04 escapes `select` inside `loop` with `break loop`).

**LX15 — Closure literals.** `fn [Signature] Block`; parameter types may be
omitted when uniquely inferable from context (06: `fn(w, r) { … }` passed to
`http.handler`). Closures capture by reference into the enclosing GC frame
(Go closure semantics); captures are GC values, never loans (O12).

**LX16 — `managed` blocks (payload REQUIRED since row 14 — discussion #4).**

```ebnf
ManagedStmt  = "managed" [ "(" StrLit { "," StrLit } ")" ] Block .
```

Lexically scoped suspension of the Tier-0 sendability checks; full rule O6.
Greppable by keyword; counted by `--report-managed` (O6.3, normative).

The payload (row 14, fail-closed): `managed("reason", "category", tickets...)`.
String LITERALS only (computed payloads would make the audit un-greppable).
arg 1 = the reason (non-empty); arg 2 = the category — a CLOSED set
(`mutex`, `atomic`, `actor`, `finalizer`, `interop`, `other`); args 3.. =
ticket references. A bare `managed { … }` PARSES but the checker rejects it
(HYB-E0003 — both missing parts named); an unknown category rejects with
HYB-E0004. The category+reason+tickets flow into `--report-managed` and the
emitted Go's marker comment (`{ // managed [mutex] reason ... }`).

### 1.4 Grammar — expressions

```ebnf
Expression   = UnaryExpr { BinaryOp UnaryExpr } [ "?" ] .
BinaryOp     = "||" | "&&" | RelOp | AddOp | MulOp .
RelOp        = "==" | "!=" | "<" | "<=" | ">" | ">=" .
AddOp        = "+" | "-" | "|" | "^" .
MulOp        = "*" | "/" | "%" | "<<" | ">>" | "&" .

UnaryExpr    = [ "!" | "-" | "^" | "move" ] UnaryExpr | PrimaryExpr .
PrimaryExpr  = Operand { Postfix } .
Postfix      = "." Identifier | "(" [ ArgList ] ")" | "[" Expression "]"
             | "[" [ Expression ] ":" [ Expression ] "]" .
Operand      = Literal | Identifier | "(" Expression ")" | FuncLit | "nil" .
FuncLit      = "fn" [ Signature ] Block .
ArgList      = Expression { "," Expression } [ "," ] | Expression "=" Expression { "," … } .
```

**LX17 — `?` (the try operator).** Postfix on an expression whose type is a
multi-value result ending in `error`; it yields the non-error results and
propagates the error (E3). Highest-binding postfix; `f()?` is one operand.
Chains (`f()?.g()?`) are `[TBD]` post-MVP — the corpus uses none.

**LX18 — `move`.** Prefix operator on a name: `move counter`. It declares an
ownership transfer of its operand and makes the operand moved-from (O5).
Legal in spawn arguments, channel sends, assignments, and returns. Applying
`move` to a literal, primitive, or other unaliased fresh value is legal but
redundant (no-op semantically, linted).

**LX19 — Slicing.** `s[a:b]` (T7); three-index `s[a:b:c]` is `[TBD]`.
`buf[..n]` omits the low bound (0); `c.order[1:]` omits the high (`len`).

### 1.5 Skeleton holes — deliberately open

- `switch` statement (plain value switch): `[TBD]` — `match` covers enums and
  option; whether `switch` is ported for Go-subset programs or `match` grows
  literal patterns (already in the grammar) is decided at revision 2, pinned
  by the differential corpus (SPEC §10 shared-subset tests).
- Struct embedding: `[TBD]` post-MVP (no example needs it).
- Labels beyond `break loop`: `[TBD]` (LX14).
- `as`-style import renaming: `[TBD]` (M7 collision policy).
- Operator precedence beyond Go's set: none added.
- Generic constraint declarations (custom `interface`-as-constraint):
  `[TBD]` post-MVP (§4 G3).

## 2. Types

Chapter order follows research/positioning.md's r/golang wishlist priority
(enums+exhaustiveness first, null safety second [§3], sum types+pattern
matching third [folded into enums], immutability fourth, Option/Result fifth)
— the ordering is a roadmap statement, not a difficulty ranking. All types
are name-lowercase, Go-style postfix declarations (`x int`, `m map[k]v`).

### 2.1 Primitives

**T1 — Primitive set.**

| type | zero value | notes |
|---|---|---|
| `bool` | `false` | |
| `u8 u16 u32 u64` / `i8 i16 i32 i64` | `0` | fixed-width; the boundary-crossing set (§12) |
| `int` / `uint` | `0` | word-sized, Go semantics; 64-bit on MVP targets; normalize to `i64`/`u64` in descriptors (B9) |
| `f32` / `f64` | `0.0` | IEEE-754; NaN compares false to itself (Go semantics, X8) |
| `byte` / `rune` | `0` | aliases: `byte` = `u8`; `rune` = `i32` code point |
| `string` | `""` (empty, valid) | UTF-8, immutable (T3) |

Primitives are value types: assignment copies them; they are always sendable
(O4 class 1). There is no `usize`/`isize` at the language level; Rust-side
`usize` maps to `u64` in descriptors (B9).

**T2 — No implicit numeric coercion.** Untyped constants adopt a type on
first use (Go rules); thereafter every conversion is explicit `T(x)`
(§11). One-line example under X4.

### 2.2 Strings — UTF-8 semantics

**T3 — Strings are immutable UTF-8 byte sequences.** `len(s)` is the byte
length; `s[i]` is the `u8` at byte index `i` (out-of-range panics, X3);
`for i, c in s` iterates byte index and `rune`; `for c in s` iterates runes.
Invalid UTF-8 in a string (only possible via byte-slice conversion) iterates
as `U+FFFD` per Go's rules. Comparison is bytewise; no normalization is
performed (Go semantics). A `rune` is an `i32` code point (T1, §2.1),
so `fmt.println(c)` and `"{c}"` display its number (T4's Go `%v` shape);
`string(c)` displays the character.

```hy
s := "héllo"
fmt.println(len(s))        // expect: 6   (é is 2 bytes)
fmt.println(s[0])          // expect: 104 (byte 'h')
for c in s { fmt.println(c) }   // expect: 104 233 108 108 111   (5 lines; é is one rune, U+00E9)
for c in s { fmt.println(string(c)) }   // expect: h é l l o   (5 lines)
```

**T4 — Interpolation.** `"{expr}"` inside an interpreted string formats the
expression with default display formatting (Go `%v` shape: `104` for ints,
`[2 2 4 6 8 10 12 18]` for slices, `true`/`false` for bools, quoted strings
render unquoted as in `fmt.println`). Verb-controlled formatting
(`{x:.3}`-style) is `[TBD]` post-MVP — use `fmt.printf` meanwhile, which
keeps Go's verb set (`fmt.printf("count=%d ok=%v\n", n, ok)`).

**T5 — Strings are immutable-by-type, hence freely shareable.** A `string`
crossing a task spawn or channel send needs no `move` (O4 class 4). Slicing a
string (`s[a:b]`) is `[TBD]`-post-MVP if the Go-subset corpus needs it; the
corpus converts explicitly via `[]byte`/`string` conversions instead.

### 2.3 Arrays, slices, maps, sets

**T6 — Arrays `[N]T`.** Fixed length, value semantics (assignment copies the
whole array — Go). Composite literal `[3]int{1, 2, 3}`. Element type may be
any type. Arrays cross the boundary only if `T` does and the total is ≤ 4
words (B1, B2).

**T7 — Slices `[]T`.** Reference to a backing array with Go semantics:
`make([]T, n)`, `append` (with `items...` variadic spread, Go), `s[a:b]`,
`len`, `cap`, `copy`, and the corpus-pinned `.pop()` (removes and returns
the last element; panics on empty — example 08). Zero value is the
**empty slice** — observationally identical to Go's nil slice (`len` 0,
`append` works, `for` iterates zero times); the nil/empty distinction is not
observable in Hybrid (N4). Slices are GC values; reads of elements are
copies (O9); a slice passed to a call is a call-scoped view (O3).

**T8 — Maps `map[K]V`.** GC hash maps, Go semantics for iteration order
(unspecified), `delete`, comma-ok read (`v, ok := m[k]`), `len`. Zero value
is the **empty map** and first-write auto-allocates (`var m map[string]int`
then `m["a"] = 1` works — a deliberate divergence from Go's nil-map panic,
recorded in N5). **Map-slot borrows** `n := &m[k]` are legal loans (O8) —
a Hybrid extension Go lacks, lowered to read-modify-write.

**T9 — Sets `set[T]` (builtin generic).** Over comparable `T`; literal
`set[T]{}`, `insert(v) bool` (false if present), `contains(v) bool`, `len`,
`for v in s` iteration, `delete(s, v)`. Zero value: empty set.

```hy
seen := set[int]{}
fmt.println(seen.insert(1))   // expect: true
fmt.println(seen.insert(1))   // expect: false
```

**Why:** example 08's corpus uses exactly this surface; a set is the
idiomatic race-free "visited" structure and cheaper to check than
`map[T]bool`. **Reversal:** drop to library type post-MVP if it earns no
second corpus use. [decided: corpus-driven — spike6 `08-graph-ownership.hy`
pins the API]

### 2.4 Structs

**T10 — Structs.** Field lists with zero values (§3); keyed composite
literals (`job{input: v, cost_ms: 3}`); unkeyed literals allowed for
1-field-compatible porting but linted (`go vet`'s rule, adopted); no
embedding in MVP; no tags in MVP (`[TBD]`, needed eventually for JSON
interop in example 05's Go-side surface).

**T11 — Methods.** Declared outside the struct body (LX11). Value receivers
copy; `*T` receivers take a unique-write loan on the receiver for the call
(O3). Method sets follow Go: a type's method set determines interface
satisfaction (I2).
Selecting a `*T` method, including a bound method value or a `task` callee,
requires a pointer or an addressable value; a map read, call result, or
struct literal cannot supply the implicit address-of.

### 2.5 Enums and sum types — the #1 r/golang request

**T12 — Enum declaration.** A `enum` is a closed sum type: unit variants,
optionally constant-carrying (`= IntLiteral`), and data-carrying variants
with positional payload types. This *replaces* `iota` and delivers the
tagged unions the wishlist ranks #1/#3.

```hy
// fragment:
enum shape {
    circle(radius f64)
    rectangle(w f64, h f64)
    point
}

enum mode {          // unit-only enum with explicit constants (the iota story)
    read  = 0
    write = 1
}
```

**T13 — Exhaustiveness is compile-checked.** A `match` on an enum value
must cover every variant, or end with a `_` arm (which is then required to
be the last arm). A missing variant is `HYB-E5001` with the missing names in
the diagnostic (§9.6's registry). This is the single most-requested Go feature
(positioning §4.1) and it is non-negotiable in the MVP.

```hy
fn describe(s shape) string {
    var out string
    match s {
        circle(r):       out = "circle r={r}"
        rectangle(w, h): out = "rect {w}x{h}"
        point:           out = "point"
    }
    return out
}
// fragment — the rejection this feature exists to produce:
fn broken(s shape) string {
    var out string
    match s {
        circle(r): out = "circle"
    }
    // reject: HYB-E5001 — match on `shape` is not exhaustive:
    //         `rectangle`, `point` are not covered (add arms, or `_:` last)
    return out
}
```

**T14 — `match` is a statement in MVP.** Arms are blocks or single
statements (LX13); there is no match-expression. **Why:** Go's `switch` is a
statement; expression-`match` invites Rust-style chaining that fights the
compile-speed and readability rules. **Reversal:** add expression form
post-MVP; tagged open (D9, pinned by checker-corpus ergonomics). [decided:
Go feel + smaller checker surface]

**T15 — Patterns (MVP set).** Variant patterns with positional bindings
(`circle(r)`, `rectangle(w, h)`), enum-qualified form (`shape.circle(r)`),
literals, single bindings, and `_`. Nested patterns (`some(circle(r))`),
guards (`circle(r) if r > 10:`), and or-patterns are `[TBD]` post-MVP — the
grammar reserves nothing for them, so adding them is additive.

**T16 — Enum equality.** `==`/`!=` are defined only for **unit-only**
enums (all variants payload-free). Data-carrying enums compare via `match`.
**Why:** payload equality is deep-equality policy (reference vs structural)
that MVP does not need and must not guess. **Reversal:** add per-variant
comparable-payload rule post-MVP. [decided: simplicity; tagged open D10]

**T17 — Numeric conversion for constant enums.** If **every** variant of a
unit-only enum carries an explicit integer constant, `int(e)` is defined
(and `mode(n)` reverse-conversion is `[TBD]`). Without full constants there
is no numeric view of an enum — they are tags, not ints. This is the
"iota-with-a-type" bridge for calling int-based Go APIs.

```hy
m := mode.read
fmt.println(int(m))     // expect: 0
```

**T18 — Enum lowering.** The emitter lowers an enum to a Go tagged struct
(kind field + payload fields) plus generated constructors; `match` lowers to
a kind dispatch. Exhaustiveness is checked in **Hybrid**, before emission —
Go never sees an open switch. Layout of boundary-crossing tagged structs:
forbidden in MVP (rule zero, B1). `[decided: emitter-level detail; exact Go
shape pinned at MVP-0 by example 07/08 codegen tests]`

### 2.6 `option[T]` — the fifth wishlist item, delivered as a builtin

**T19 — `option[T]`** is a builtin generic enum with two variants: `none`
and `some(T)`. It is the *only* null-safety story for data (§3): absence is
a value, not a pointer state.

```hy
fn find_user(id int) option[string] {
    if id == 1 { return some("ada") }
    return none
}

match find_user(1) {
    some(name): fmt.println("found {name}")   // expect: found ada
    none:       fmt.println("missing")
}
```

**T20 — Option API (MVP surface).** Constructors `some(v)` / `none`;
`o.unwrap()` (panics with `HY-PANIC/unwrap-none` if `none` — the direct
`m[k]`-style escape hatch); `o.or(default)` returns the payload or the
default; `o.is_some() bool`. No `==` on option in MVP (T16's rule applied
to the builtin). `?` does **not** apply to option — the try operator is
errors-only (E3), so the two absence mechanisms never blur.

**T21 — Where option appears.** Map/slice lookups via comma-ok *or* option
(comma-ok stays for Go-subset porting; new APIs prefer option — example 07
uses both styles deliberately). Function results that can be absent. Struct
fields that may hold nothing (the nil-field killer). Option is a value
wrapper: `option[T]` is sendable iff `T` is (O4).

### 2.7 Immutability — the fourth wishlist item, scoped honestly

**T22 — MVP immutability is binding-level.** `let x = e` creates an
immutable binding (no rebinding/reassignment; interior mutation through an
alias is still possible — `let` is shallow); `const` creates a compile-time
constant (deeply immutable, and the only *deep* immutability in MVP).
**Deep `const` slices/maps** ("Slices/Maps as const", the exact wishlist
ask) are `[TBD]` post-MVP: they interact with sendability class 4 (O4) and
deserve their own rule once the adversarial corpus shows what programs
actually need. **Why scoped:** deep immutability for reference types is a
type-system feature, not a keyword; faking it would create a false safety
claim. [decided: honesty over wishlist completeness; reversal = design the
type qualifier when corpus demands]

### 2.8 Tuples — decided: no

**T23 — There is no tuple type.** Multi-value returns exist (LX12) but are
not first-class values: they cannot be stored, passed, or compared — only
destructured at the call site or returned.

```hy
x, y := minmax(3, 9)          // ok:   destructuring declaration
p := minmax(3, 9)             // reject: HYB-E2002 — multi-value is not a tuple;
                              //        bind two names or declare a struct
```

**Why:** Go has none; structs-with-two-fields cost one line; a tuple type
adds inference ambiguity (and compile-time cost) for near-zero Go feel.
**Reversal:** add `(T, U)` post-MVP if the differential corpus shows Go
programs whose faithful port needs it. [decided: Go compat + simplicity]

### 2.9 Pointers, borrows, and receivers

**T24 — Hybrid has no general pointer type.** The `*` appears in exactly
three places: (1) method receivers `fn (c *cache) get(…)` — a unique-write
loan notation (O3); (2) foreign Go types (`*os.File`, `*http.request`) which
keep Go semantics including Go's nil (N2); (3) map-slot borrows `&m[k]`
(O8). There is no `&x` for general values, no pointer arithmetic, no
double indirection, and no user-declared pointer types.

```hy
n := &g.nodes[from]      // ok: map-slot loan (O8) — the only general-value `&`
p := &n                  // reject: HYB-E3104 — no address-of on a loan
```

**Why:** Tier 0's promise is that loans are function-local and never spelled
in signatures (O2); general pointers would immediately break both.
**Reversal:** none within Tier 0; a pointer type belongs to `unsafe`
(§9.9), which is post-MVP.

### 2.10 Type aliases and named types

**T25 — `type name = T`** declares an alias (identical type). **Named struct
types** (`struct node { … }`) are nominal. `[TBD]`: Go's `type name T`
derived-type form (methods on a named primitive) — deferred until the
differential corpus ports code needing it. [decided: corpus silence]

### 2.11 `result[T, E]` — considered, deferred

**T26 — No first-class `result` type in MVP.** The error model is Go-shaped
`(T, error)` + `?` (E1–E10), which *is* the Result-shaped dataflow with Go's
spelling; a distinct `result[T, E]` type would split one concept into two
spellings and double the `?`-checking surface. `result` may return post-MVP
as an ordinary library enum (`enum result[T, E] { ok(T), err(E) }`) if
positioning evidence (the r/rust audience) demands it. [decided: SPEC §9
rule 3 — lower risk to MVP first; tagged open D6]

## 3. Zero values and `nil` — the null-safety design

The wishlist's #2 ask, in the asker's words: "I'd love to see the compiler
tell me reliably if I forgot a null check and keep the same speed." The
design below delivers that by removing the *nullable-by-default* state from
every Hybrid-declared type, and by making the two places `nil` must survive
(Go's error convention, and the Go interop perimeter) explicit, checkable,
and documented.

### 3.1 The rules

**N1 — `nil` exists in exactly two scoped places.**

1. **`error`**: `nil` is the zero value of `error` and means *no error*
   (Go's convention, kept because the `(T, error)` ABI is load-bearing —
   E1, E8, and example 09's adoption story).
2. **Foreign Go values** (types imported from Go packages): they keep Go's
   semantics including Go's nil pointers, nil interfaces, and the nil checks
   that guard them (N2).

Everywhere else, `nil` is a compile error:

```hy
var m map[string]int
m = nil                        // reject: HYB-E4001 — map has no nil; zero value is
                               //          the empty map; absence is option[map]
var s []int = nil              // reject: HYB-E4001 — slice has no nil; use []int{} or option[[]int]
```

**Why:** this is the entire feature — a nil-check the compiler can "tell you
reliably" exists only where nil can actually appear. **Reversal:** none;
adding nullable-by-default back would delete the wishlist #2 feature.

**N2 — The Go-compat perimeter is Go's world.** Values whose type comes
from a Go package may be nil, are compared with `== nil`, and dereference
of a foreign nil pointer panics with Go's runtime message. The compiler
emits a *lint* (`hy-lint/foreign-nil-deref`) when it can prove a foreign
nil dereference, but never a hard error.

```hy
// fragment:
f := os.open(path)?        // f: *os.file — foreign, may be nil in general
if f == nil { … }          // legal check on a foreign type
```

**Why:** "Keep Go exactly as it is" — Go APIs passing nil pointers into
Hybrid must keep working unchanged (memory-safety ordering in SPEC §9 rule 3
puts Go compatibility above removing a *panic* class Go already owns).
**Reversal:** a hard check could be layered on later behind a lint level;
tagged open D3.

**N3 — Hybrid-declared types all have non-nullable zero values.**

| type | zero value |
|---|---|
| primitives (T1) | `0`, `0.0`, `false` |
| `string` | `""` — a valid empty string |
| `option[T]` | `none` |
| enum | first variant **iff** it is payload-free; otherwise the type has no zero value and `var e E` is a compile error (`HYB-E4002`) |
| struct | field-wise zero values (hence: no zero value if any field's type has none) |
| slice / map / set | empty (`len` 0; first write allocates — N5) |
| chan | the **zero channel**: receive and send block forever, `close` panics (Go's nil-channel semantics exactly) |

```hy
struct greeting {
    name  string
    count int        // count starts at 0 — example 01
}
g := greeting{name: "Hybrid"}
fmt.println(g.count)         // expect: 0
```

**Why:** zero-value-constructibility is a Go idiom ("the zero value is
useful") that Hybrid keeps; the zero channel is the one deliberately
awkward case, kept Go-exact because channels are synchronization primitives,
not data, and `select` with a timeout already gives the escape.
**Reversal:** the enum zero-value rule could be relaxed to "no zero value
for any data-carrying enum's declaring struct" — no; current rule is the
loosest that stays honest. [decided: Go idiom + exhaustiveness coherence]

**N4 — nil-slice/empty-slice unification.** On Hybrid's side the nil/empty
slice distinction is unobservable: a Go nil slice entering Hybrid (N6) is the
empty slice; `len`, `append`, `range`, `==` (slices are not comparable
except to nothing — Go rule kept) all behave identically.

**N5 — Map zero value auto-allocates (divergence from Go, recorded).**
`var m map[k]v; m[k] = v` works (first write allocates), where Go panics on
a nil map. **Why:** removes an entire nil-shaped panic class from
Hybrid-declared code at the cost of one emitted guard. **Reversal:** make
zero maps read-only (Go-exact) if the differential corpus shows the guard
costs anything measurable. [decided: null-safety coherence; tagged open D11]

### 3.2 The interop story, concretely

**N6 — Go callers' nil values mapping in** (examples 02 and 09 are the
pinned cases):

| Go value at the perimeter | Hybrid view |
|---|---|
| `nil` error | `nil` (no error) — 1:1, verified by Spike 5's identity tests |
| nil `*T` / nil interface (foreign) | foreign nil — `== nil` allowed (N2) |
| nil slice | empty slice (N4) |
| nil map | reads behave as empty; **writes panic** — the perimeter keeps Go semantics for Go-owned storage (contrast N5, which is about Hybrid-declared maps) |
| nil chan | the zero channel (N3) |

**N7 — Hybrid values mapping out to Go** (the adoption story, example 09):

| Hybrid value | Go view after emission |
|---|---|
| `(string, error)` result | `func F(email string) (string, error)` — `nil` error maps to Go `nil` |
| `option[T]` in an exported signature | generated Go struct `type option_t struct { some bool; value t }` with constructors — `[TBD]` exact naming at MVP-0, pinned by extending example 09 with an option-returning export |
| enum | generated tagged struct (T18) |
| `pub fn` name `validate_email` | Go `ValidateEmail` (M4's export mapping) |

**N8 — `errors.new` never returns nil; `?` never produces nil payloads.**
A function whose type says `(T, error)` returns either a real `T` or a real
`error`, never a zeroed `T` *and* a non-nil error pair that the caller must
defensively handle — the `?` desugaring (E3) guarantees the pairing at every
propagation site. `[decided: closes the "forgotten nil check" class at the
source]`

### 3.3 What was rejected

- **Nullable-by-default (Go's model):** rejected — that *is* the bug class
  the wishlist complains about; keeping it would make Hybrid "a worse Go"
  (Q1's phrase applies structurally here too).
- **Dart-style `?T` suffix:** rejected — two spellings for absence (`?T` and
  `option[T]`) with different type rules; `option[T]` + match already
  delivers sound, exhaustive absence-handling with one concept. `[decided:
  one absence mechanism; reversal = add `?T` sugar post-MVP only if corpus
  shows option ergonomics failing]`
- **`nil` for maps/slices/chans:** rejected (N1) — zero values replace it.

---

## 4. Generics

**G1 — Hybrid generics are Go generics.** Declaration syntax, constraints,
and semantics follow Go's type-parameter design as implemented by the pinned
toolchain (go1.26.9): `[k comparable, v any]` type parameters on structs,
enums, interfaces, and functions; instantiation `lru_cache[string, int]`.

```hy
// fragment:
struct lru_cache[k comparable, v any] {
    cap     int
    entries map[k]v
    order   []k
}

fn lru_cache.new[k comparable, v any](cap int) lru_cache[k, v] {
    return lru_cache[k, v]{cap: cap, entries: map[k]v{}, order: []k{}}
}
```

(Example 07 is the pinned program; its expected output `b evicted` / `a=1`
is the acceptance test.)

**G2 — Lowering: Go type parameters, NOT monomorphization.** Hybrid emits Go
source (Option B in force), so generic Hybrid declarations lower to generic
Go declarations and instantiation lowers to instantiation. gc's
implementation (shape-based dictionaries) is inherited wholesale.
**Monomorphization is rejected for MVP.** **Why:** compile speed is the
never-break rule (positioning §1/§4 — "they care more about compile speed
than features"); monomorphizing would multiply emitted code and rebuild
times for zero behavioral benefit. **Reversal:** a post-MVP optimization
may monomorphize *marked hot* instantiations behind a flag, benchmarked
against the §6 budget; tagged open D7, pinned by example 07's macro-bench
in Phase 4. [decided: Option B constraint + compile-speed rule]

**G3 — Constraints (MVP): `comparable` and `any` only.** Custom constraint
interfaces (`interface { fn hash() u64 }` as a constraint) are `[TBD]`
post-MVP (G4 in §1.5); until then, method-bearing requirements are
expressed with interface parameters and type assertions are not needed —
or the code is written concretely.

**G4 — Boundary calls require concrete instantiation.** A generic function
may not be called across the Rust boundary with free type parameters: every
boundary call site must be monomorphic at descriptor level (the descriptor
schema — interfaces/abi-types.md §5 — is per-signature and unparameterized).
The checker enforces this; per-instantiation descriptors are generated on
demand and deduplicated.

```hy
fn cached_hash[k comparable](k k) u64 { … }          // ok: Go-side generic
// calling a rust/ crate fn with a generic type parameter is a compile error:
// reject: HYB-E7002 — boundary calls need concrete types; instantiate first
```

**Why:** descriptors are versioned hashes of concrete signatures; leaving
them generic would make the runtime handshake unbounded. **Reversal:** none
planned. [decided: ABI constraint, interfaces/abi-types.md]

**G5 — Inference.** Call-site type inference follows Go's rules
(inferable from argument types; explicit instantiation when ambiguous).
Type parameters may not appear in receiver-only positions (Go's rule,
kept).

---

## 5. Interfaces and traits

**I1 — Hybrid interfaces are Go interfaces.** Declared with `interface`,
satisfied **structurally and implicitly** (no `impl` blocks — the not-Rust
list), method sets and dynamic dispatch follow Go, and after emission a
Hybrid interface *is* a Go interface value (itable dispatch, Go's cost
profile — SPEC §6 "same as Go to Go").

```hy
// fragment:
interface counter {
    next() u64
}

fn (s *seq) next() u64 { s.n += 1; return s.n }   // seq satisfies counter implicitly

fn total(c counter, n int) u64 {                  // dynamic dispatch through the interface
    var sum u64
    for i in 0..n { sum += c.next() }
    return sum
}
```

**I2 — Method sets (Go's rules, verbatim).** Value-receiver methods are in
both the value and pointer method sets; pointer-receiver methods only in the
pointer set. A `*T`-receiver call takes the unique-write loan on the
receiver (O3), which the loan checker sees — so an interface value holding a
`*T` keeps a *reference*, not a loan: interface values are GC values
(dispatch targets are heap-allocated or addressable, Go's escape rules
apply).

**I3 — The foreign-Go interface is the common case.** Interfaces imported
from Go packages (`io.reader`, `http.handler`) are used directly as types;
Hybrid closures convert to them (example 06's `http.handler(fn(w, r) { … })`),
and Hybrid types satisfy them with the same structural rule. This is the
load-bearing Go interop: no wrapper layer, no adapter traits.

**I4 — Rust traits map through the descriptor, not the type system.** A
Rust crate's types appear in Hybrid as opaque handle types (B6) whose
methods are whatever the binding generator extracted (rustdoc JSON →
descriptor): inherent impls plus trait impls that are dyn-compatible
("object safe"). Trait *names* do not appear; the methods arrive on the
handle:

```hy
// fragment:
import "rust/regex"
re := regex.compile(pattern)?    // re: own-handle; regex::Regex behind it
re.is_match(subject)             // a trait-or-inherent method — same call shape
```

**Why:** handle indirection erases the Rust type, so only dyn-dispatchable
surface can cross faithfully; pretending otherwise would fake static
dispatch we cannot deliver. **Reversal:** newtype bridge crates (Hybrid type
exported to Rust with generated trait impls) is `[TBD]` post-MVP, gated on
a real program needing it. [decided: ABI honesty, abi-types.md rule zero]

**I5 — No Hybrid-implements-Rust-trait in MVP; no Rust-trait bounds in
Hybrid signatures.** The two directions that would require cross-language
coherence checking are out; both are stretch items per SPEC §7 (unified
scheduler tier and below in the cut order).

**I6 — Interface embedding**: `[TBD]` post-MVP with struct embedding (§1.5).

## 6. Error handling and the `try` operator (`?`)

This chapter is the language-level face of the Spike-5 error ABI: **two-word
HyResult (RAX:RDX) + sentinel table + wrap-on-demand**, verified on both tiers
(ok path 2.58 ns / 0 allocs; error path exactly 1 alloc; `errors.Is` identity
survives; Rust `source()` chains do not). The language surface is Go-shaped:
`(T, error)` returns plus a postfix `?` that propagates.

### 6.1 The error value

**E1 — `error` is a builtin interface-shaped type, always the LAST result.**
Functions signal failure by returning `error` as the final result:
`(string, error)`, `(total int, err error)`, or bare `error`. The Go
convention is codified (not merely idiomatic) so the emitter can map results
1:1 onto Go signatures and the HyResult ABI (E8).

**E2 — `nil` means no error** (N1.1). Constructing errors:
`errors.new("invalid email address")`; wrapping: `errors.wrap(err, "opening
config")` (the wrap-on-demand surface — one allocation, Spike 5 E3);
identity: `errors.is(err, sentinel)`. `errors.as` is `[TBD]` post-MVP
(needs the type-descriptor machinery of Phase 4); examples 02/09 pin only
`new`/`is`/implicit wrap.

**E3 — The try operator `?` (postfix, calls only).** `v := f(args)?` where
`f`'s type is `(T, error) …` binds `v` to `T` and, on error, returns the
error from the enclosing function immediately. With multiple non-error
results, `x, y := f()?` binds both. The desugaring (the checker expands this
before loan checking, so `?` is never a checker special case):

```hy
// fragment:
f := os.open(path)?
// desugars to:
f, hy_err := os.open(path)
if hy_err != nil { return _, hy_err }   // other named results return their zero values
```

**E3b — the expression form** (Batch G, pinned by example 05): `?` may also
appear nested inside a larger expression (`append(names, u.string("k")?)`,
`if u.int("k")? >= 3`, `for u in doc.array("a")?`). Each such site hoists to
the binding form before the statement: `t, hy_err := u.string("k")` +
propagation, then `t` substitutes at the use site. Unique temps keep nesting
sound; hoists happen in evaluation order and never inside a closure literal
(the closure hoists its own). Where the propagation cannot land (no error
result, not main, not an E11 handler), it is a compile error — never a
silent swallow.

**E4 — Where `?` may appear.** In any function whose results end in `error`
(and in `main`, where it triggers the exit contract E6). `?` on an
expression that is not an error-returning call is a type error; `?` on
`option` is a type error (T20) — absence and failure stay distinct.
`?` inside a closure binds to the closure's own results, not the enclosing
function's (lexical, like `return`).

**E5 — No hidden allocations.** The ok path of a `?`-propagated boundary
call performs **zero allocations**; a propagated error performs at most one
(the on-demand wrap) — both are Spike-5-measured facts the emitter must
preserve (SPEC §6 budget: 0 / ≤1). Regression tests count allocations
explicitly.

### 6.2 A8 — the top-level `?` exit contract

**E6 — `?` at the top level of `main` terminates the process.** This
resolves Spike-6 ambiguity **A8** and is pinned byte-for-byte by example 02's
expected output (`cat: open <file>: no such file or directory`, exit 1).
The rule: when a `?` fires in `main`'s body (not inside a nested closure
with its own `error` result), the runtime, in order:

1. runs `main`'s deferred calls (LIFO — P1; deferred resources are closed),
2. writes `"<prog>: {err}"` to **stderr** (one line, `{err}` in default
   display formatting),
3. exits with status **1**.

`<prog>` is `basename(os.args[0])`, or `"hybrid"` if args are empty.
`main` may not declare an `error` result (its `?` sites are the exit
contract); a declared non-error result (`fn main() (code int)`) is bypassed
by the `?` exit — the explicit `return 1` path in 02 coexists with it.

```hy
// fragment:
fn main() (code int) {
    args := os.args()
    path := args[1]
    f := os.open(path)?      // missing file → stderr "cat: open <path>: no such
    defer f.close()          // such file or directory", exit 1 — and f's defer
    …                        // would run first if a LATER `?` fired
}
```

**Why defers run:** Go's `panic` runs defers; an error exit that silently
leaked file handles would be a Go-feel regression, and the emission is a
caught-sentinel in the generated main wrapper. **Reversal:** drop to
`os.exit`-style (no defers) if MVP-0 shows the wrapper complicating the
emitter; pinned by example 02's exit-path test. [decided: Go-shaped cleanup
semantics; tagged open D12]

### 6.3 The boundary ABI (normative summary; detail in interfaces/abi-types.md)

**E7 — Errors cross as HyResult, never as Go interface pointers.** A
boundary function returning `(T, error)` at the language level returns a
**two-word HyResult (RAX:RDX)** at the ABI level: the value word plus an
error word that is either 0 (ok) or an index into the **sentinel table**
(the crate's registered error kinds). The Go-side mapping layer turns a
non-zero sentinel into a Hybrid `error` **on demand**, wrapping it (one
allocation, E5) with the crate's message text.

**E8 — Identity contract (Spike 5 E4, restated normatively).** `errors.is`
identity survives a crossing: a sentinel maps to a stable Go-side error
value per (crate, kind), so `errors.is(err, regex.err_syntax)` works across
the boundary. Rust-side `source()` chains do **not** survive (documented
loss); `errors.wrap` at the Hybrid call site recovers the text. This loss is
recorded in the spike-report's honest ledger and is not re-litigated here.

**E9 — Panics are not errors.** `?` never catches a panic; panic semantics
are §7 and are never converted to error values at the boundary (P4).

**E10 — What was rejected.** Exceptions (unwinding as control flow) —
rejected: unwinding across the Rust boundary is UB (SPEC prime directive 6)
and Go's ecosystem convention is return-values. `try`/`catch` blocks —
rejected: `?` + `if err != nil` cover the same ground with less syntax.
First-class `result[T, E]` — rejected for MVP, T21. [decided: Spike-5 ABI +
Go shape; each recorded above]

**E11 — The handler-conversion propagation contract.** A closure literal
converting to a *conversion target* (§5's `http.handler`-shaped types) has
no `error` result for E3's return to land in — the target's Go signature
decides. A target may therefore declare a **propagation contract**: when `?`
fires inside such a closure, the declared statement runs and the closure
returns. `http.handler`'s contract (pinned by example 06):
`http.Error(w, err, http.StatusBadRequest)` + return — the idiomatic Go
handler answer, not a silent swallow and not a panic. Everything else from
E4 still holds (`?` on non-error calls is a type error; E5's allocation
budget applies). Scope: exactly one target carries a contract at MVP;
adding one is a Decision Log entry. [decided: Batch E, 2026-10-13]

Two mechanical consequences of the same batch (M5's curated-surface
mechanics): **import paths map** root → real Go path (`http` → `net/http`,
`httptest` → `net/http/httptest` — the code keeps the root spelling, which
IS Go's package name); and the **`httptest.get/post(mux, path)` smoke
client** is a composite lowering to the in-process recorder dance
(`http.NewRequest` → `httptest.NewRecorder` → `ServeHTTP`), binding
`(code, body)` with `?` carrying the NewRequest error; `body` is the
response's body LINE (the single trailing newline is transport, trimmed) —
pinned byte-for-byte by example 06's expected output.

---

## 7. `defer`, `panic`, `recover` — and THE BOUNDARY RULE

**P1 — `defer` is Go's.** `defer f.close()` schedules a call (arguments
evaluated at defer time; LIFO at function exit; runs on `return`, on `?`
propagation [E3's desugaring returns through the normal path], and on the
main-level `?` exit [E6]). A deferred **call** is not a loan: no borrow
outlives the frame (spike-6 program 02's traced property, now a rule).

```hy
// fragment:
f := os.open(path)?
defer f.close()        // deferred CALL — the only legal defer operand shape in MVP
```

`defer` of a non-call expression is a compile error (`HYB-E2001`).
Multiple-value-defer (`defer f.close(fd)` with args captured now) follows
Go; `defer` inside `select` arms follows Go's block semantics.

**P2 — `panic` is Go's.** `panic expr` (statement form, LX grammar)
unwinds the Hybrid call stack exactly as a Go panic unwinds the emitted Go
stack: defers run, then the process dies with Go's exit status (2) and Go's
panic output. `panic` never converts to an `error` at any layer.

**P3 — `recover` is Go's.** Usable only in deferred functions; returns the
panic value or `nil`; stops the unwinding of *that* panic. MVP keeps it for
server-shape programs (a handler goroutine must not kill the process —
example 06's perimeter). Cross-language interaction: none — see P4.

**P4 — THE BOUNDARY RULE (never-cut).** **No unwinding crosses the
Hybrid↔Rust boundary — ever.** Concretely:

1. A Rust `panic!` that reaches the boundary **aborts the process**
   (SIGABRT at the Rust edge; child-process-verified, Spike 5 E5). It does
   not unwind into Hybrid, does not become an error, and cannot be
   `recover`ed.
2. The staticlib crate graph is built `panic=abort`; exactly **one** Rust
   staticlib per binary (PLAN Decision Log; linking hazard rule).
3. There is no reverse path in MVP: **no Go/Hybrid callbacks into Rust
   code** (function pointers are forbidden at the boundary, B1), so a
   Hybrid panic can never find itself above a Rust frame.
4. `catch_unwind`-at-the-edge (panic→error conversion) is **rejected** for
   MVP; the reversal clause lives in PLAN.md's Decision Log: revisit only
   at G1 if abort proves unacceptable in practice.

```hy
// fragment — the boundary panic contract as an executable claim:
// a rust/ function that panics kills the process with SIGABRT; the test
// runner asserts exit-by-signal, not an error value:
//   expect: process aborted (SIGABRT); stderr contains the Rust panic message
```

**Why abort rather than convert:** converting panics to errors would make
`?` a disguised exception handler across an ABI where unwinding is UB, and
would silently eat the "the process is in an unknown state" signal that
panics exist to send. **Reversal:** priced and logged (PLAN.md 2026-10-12).

**P5 — `?`-exit vs `panic`-exit are different contracts.** `?` at main:
defers run, message `<prog>: {err}` on stderr, exit 1 (E6). `panic`:
defers run, Go panic output, exit 2 (P2). Programs must not rely on panic
formatting (it is Go's, and Go does not guarantee it).

## 8. Concurrency: tasks, channels, `select`

Hybrid tasks **are** goroutines (Option B: emitted Go; the two-tier blocking
design of Spike 4 lives *below* the language surface — invisible interop,
per positioning). The race story is Tier 0's sendability rules, summarized
here and specified in §9.

### 8.1 Tasks

**C1 — `task` spawns a concurrent function call on a new task
(goroutine).**

```hy
task worker(i, jobs, results)      // spawn site [S1] — sendability-checked (O4)
```

Arguments follow Go's call-by-value rules **as modified by the sendability
checker**: every argument is a check-site (C5). The spawn has no handle of
its own (there is no `WaitGroup`/join in the MVP language surface — example
03 joins by counting on a channel; `sync` is `[TBD]` for the curated Go
surface, D4). `task` of a closure literal is `[TBD]` post-MVP (grammar
accepts it; checker semantics for closure captures at spawn are designed
with the adversarial corpus — D2). [decided: corpus shape; nothing in the
ten programs spawns a closure]

**C2 — Spawn budget.** Task spawn must stay within 2× of Go goroutine spawn
(SPEC §6; measured against the pinned toolchain at Phase 3, not asserted
here). The bridge's blocking-pool and thread-budget machinery (Spike 4) is
runtime-owned and invisible to `task` semantics.

### 8.2 Channels

**C3 — Channels are Go's.** `make(chan T)`, `make(chan T, n)` (buffered —
the buffer IS the backpressure mechanism, example 04's size-2 channels),
directional types `chan<- T` / `<-chan T` (type-checked producer/consumer
roles), send `ch <- v`, receive `<-ch` / `v, ok := <-ch`, `close(ch)`
(receive-after-close yields the zero value with `ok == false`; send-on-close
panics), `for v in ch` (receives until close), `len`/`cap` on buffered
channels. Zero value: the zero channel (N3). Channels are
**safe-by-type**: sharing a channel across tasks is exactly what channels
are for (O4 class 2) — a channel passed to a spawn needs no `move`.

The producer/consumer roles are checked on the **operand**, not only on a bare
identifier: a channel reached through a composite expression is resolved to the
channel it denotes, so `<-chs[0]`, `<-chs[i:]`, `<-append(chs, ch)[0]` and
`<-outer[0][0]` are all judged by the direction of the channel they produce.
A receive from a send-only channel is rejected wherever it appears
(HYB-E2008); an operand whose direction cannot be resolved from its shape
stays fail-closed (HYB-E3005). The send side resolves the same projections,
so the two directions agree.

```hy
// fragment:
jobs := make(chan job, 16)
for i in 0..n_workers {
    task worker(i, jobs, results)     // channels cross the spawn freely
}
jobs <- job{input: v, cost_ms: (v % 3)}   // fresh literal: sendable (O4 class 5)
close(jobs)
```

### 8.3 `select`

**C4 — `select` is Go's.** One or more comm clauses plus optional
`default`; a ready clause is chosen uniformly at random among ready ones;
with no ready clause and no `default`, the task blocks. Timeout idiom uses
`time.after`, which returns a receive-only channel (example 04, the pinned
program):

```hy
deadline := time.after(time.seconds(2))
loop {
    select {
        v, ok := <-sq:
            if !ok { break loop }       // pipeline drained
            total += v
        <-deadline:
            panic "pipeline timed out"  // never fires in the happy path
    }
}
fmt.println("total={total}")            // expect: total=204
```

`break`/`continue` inside `select` bind to the enclosing `loop`/`for`
(LX14; `break loop` shown above). Send clauses (`ch <- v:`) are
sendability check-sites exactly like statement sends (C5).

`default` is contextual at a select arm head (`default:`), including after
an unbraced preceding arm. It is not an additional LX3 reserved word:
ordinary identifier uses, such as a channel named `default`, remain legal.
Corpus 24 pins the nonblocking fallback; parser tests pin this distinction.

### 8.4 Sendability check-sites — the complete list

**C5 — Exactly two Hybrid-level check-sites gate cross-task sharing of
Layer-1 (GC) values:**

1. **spawn arguments** — every argument of a `task` statement;
2. **channel sends** — the operand of `ch <- v` (both statement and
   `select` send clauses).

No other construct moves a value to another task in MVP Hybrid; therefore
no other construct is checked. The rules applied at these sites are O4–O6
(§9). Receiving (`<-ch`) is never a check-site — the receiver owns what it
receives.

**C5a — Receive operands are TYPE-checked, not sendability-checked
(HYB-E2008/E3005).** C5's "never a check-site" is about ownership: the
receiver owns what it receives, so no sendability rule fires. The operand's
*type* is a different contract — the accepted-subset rule that every
accepted program emits buildable Go. Since issue #77, a send to a
non-channel is rejected (`HYB-E3007`; unresolvable targets fail closed
under `HYB-E3005`, O4.9). The receive direction is symmetric: `<-expr`
where `expr` resolves to a concrete non-channel is `HYB-E2008`
(`x := 5; <-x` used to pass with 0 diagnostics and emit Go that fails
`go build` with `cannot receive from non-channel`); an unresolvable
operand fails closed under `HYB-E3005`, exactly as the send side does.
Fires at every syntactic position a receive can occupy — statement,
binding, nested in an expression, `select` receive clause, and closure
bodies (the A5 perimeter).

**C6 — Blocking Rust calls are ordinary calls.** A `rust/` crate function
flagged `blocking` in its descriptor crosses via Spike 4's two-tier path
(entersyscallblock syswin lane behind sampler-gated admission + bounded
blocking pool for overflow/high fan-in). The language surface is unchanged:
`sha3.stream_checksum(1_000_000, seed=7)?` is a normal call (example 10,
whose hot loop makes ONE crossing for a million iterations). The thread
budget is configuration (Q2 ruling, PLAN.md Decision Log 2026-10-12):
ceiling = `max_threads` config, default `min(512, 0.5 × pids.max)` when
readable; probes may only LOWER it; effective budget + source logged at
startup; behavior at the budget is queue/backpressure, documented — never a
silent crash.

**C7 — No `fork`.** No process-spawn primitive; `os/exec` interop only
(runtime-hazards rule 7). Environment via `os`; Rust init runs only from
generated Go `init()`.

**C8 — Atomics are `[TBD]`.** `sync/atomic` Go interop joins the curated
surface post-MVP; when it lands, atomic-typed values become O4 class 2
(sendable safe-by-type). No Hybrid-level `atomic` keyword is planned.

### 8.5 A5 — the Go-API escape perimeter (decided: fourth check-site class)

**C9 — Values passed to foreign Go APIs ARE check-sites (fourth class).**
A closure that writes to a captured mutable binding and is passed as an
argument to a foreign (package-qualified) Go call must take
`managed("...", "interop")` at the call site, or the capture must be
`const` (immutable-by-type), or the capture must be non-`may_share`.
Failure rejects with `HYB-Exxxx`. No curated Go API database is consulted;
the check is decidable from Hybrid source alone: (1) closure body writes
a captured binding, (2) closure is passed to a foreign call.

**Decision: fourth check-site class (D5 promoted, W-016 M2).**

This resolves Spike-6 ambiguity **A5** by promoting the
`hy-lint/go-escape-mut-shared` lint from diagnostic-only to a hard checker
check-site. The closed-world rule needs no curated API database: flagging
"concurrent" Go APIs required a per-package curated list that would always
lag Go's ecosystem — a compatibility treadmill. The closed-world rule
needs no API knowledge, expressing the check purely over Hybrid syntax.

**Why fourth check-site over scoped-claim:** (1) The Go side does not
cooperate — freezing sender aliases cannot stop *Go* code from mutating a
shared value, so a hard check would promise race-freedom the runtime
cannot deliver. The check does not promise race-freedom; it promises an
*auditable escape*: the call site must take `managed` or prove the capture
is non-shared/immutable. (2) The crawler treadmill argument is
structurally absent — no API knowledge is in the rule. (3) Example 06
(the pinned case) is race-free and compiles unmodified under both options
(its closures capture `next` but never write), so the stricter one wins
without breaking the must-compile constraint. [decided: D5 promoted;
logged by W-016 M2, PR #2195]

**Census measurement (W-016 M2):** `tools/escape_sites.py` (M1, PR #2149)
re-measured at merge time against the current 127-program checker corpus
plus all 10 examples (137 files, post-M1-fix tree, 2026-10-31): exactly 1
triple found (program 10,
`hits++` → `mux.handle`), example 06 yields 0 triples. 1/127 ≈ 0.8%, well
under the 25% adoption-cost tripwire. D5 adoption cost accepted.

## 9. Ownership, borrowing, and the safe/unsafe boundary — TIER 0

The deepest chapter in this spec. It encodes the Tier-0 model confirmed by
Spike 6 (10/10 target programs, **0 lifetime annotations, 0 `managed`, 0
outside-model**, under the strictest rule readings — hand-traced,
believed-level), resolves the eight ambiguities Spike 6 found (**A1–A8**),
and carries the human's Q1 ruling as binding normative text. Sources, in
precedence order: QUESTIONS.md Q1 (2026-10-12 ruling),
research/ownership-models.md ("Recommendation for Spike 6"),
spikes/spike6-ownership/RESULTS.md, PLAN.md Design Decisions in Force.

### 9.0 The model in three layers

**O1 — Three layers, none with lifetime syntax.**

| layer | values | memory-safety story | race story |
|---|---|---|---|
| **L1 — Go-side GC** | structs, slices, maps, sets, strings, channels, closures, interfaces | Go's GC (UAF impossible) | sendability checks at spawn/send (O4–O6) |
| **L2 — Rust-side owned** | values whose storage lives in Rust; in MVP they surface as own-handles (O13) | single owner + moves (O13) | moves are exclusive by construction |
| **L3 — boundary handles** | own/borrow handles to objects on either side (O14) | handle tables, generation tags (B6) | handle modes own/borrow/mutate (O7) |

**O2 — The no-visible-lifetimes promise.** Loans are **function-local**:
they are created and die inside one function body, checked by a
non-lexical-liveness dataflow pass (O15), and **never appear in any
user-facing signature** — no lifetime parameters, no region syntax, no
borrow-mode markers on declarations. A function needing longer-lived access
returns an index/id/handle (example 08's `node_id` pattern) or uses
`managed`. This is pre-registered as a checker invariant (Spike 6's
P2-falsifier discipline): if any corpus program ever needs an annotation to
compile, Tier 0 is wrong and the fallback is rustc-oracle +
`managed`-by-default — lifetime syntax is never invented to save it.

**O3 — The loan forms (the complete list).** Exactly four shapes create
loans in Tier 0:

1. `*T` method receivers — a **unique-write loan** of the receiver, live for
   the call;
2. call-argument **views** — a slice/buffer/struct passed to a call is lent
   for that call only (the callee's contract, per its descriptor or package
   API, already forbids retention);
3. **map-slot borrows** `n := &m[k]` — a unique loan on one map slot (O8);
4. **borrow-mode handle uses** — calls on an L3 handle in borrow mode (O7).

Everything else — assignments, field reads, map/slice reads, receives — is a
copy or a GC reference, not a loan (O9). If a fifth loan shape seems
necessary, the answer is a corpus program + a logged decision, not a silent
checker accretion (Spike 6 risk #4).

### 9.1 Loan rules (resolving A2, A3, A4, A6, A7)

**O7 — Handle loans and modes (resolves A2).** Every L3 handle use has a
mode, derived from the callee's descriptor: **own** (consumes the handle),
**borrow** (read loan), **mutate** (write access). Rule: a **borrow-mode use
creates a loan on the handle** that lives (non-lexically) until its last use;
a **mutating-mode call conflicts with any live borrow-mode loan on the same
handle** — compile error `HYB-E3101`. Mutating-mode calls are call-scoped unique
accesses (two never overlap); own-mode consumes (O13).

```hy
for u in doc.array("users")? { … }   // borrow loan on `doc` lives across the loop
doc.remove("users")                  // reject: HYB-E3101 — mutating call on `doc` while
                                     // the element-view loan (line above) is live
```

**Why:** without this rule, iteration over a Rust-side collection while
mutating it is a Rust-side aliasing hole (Spike 6 found the rule text
missing and spike-added it; it is now normative). Spike-6 program 05 passes
because `doc` is never mutated while element views are live.

**O8 — Map-slot borrows (resolves A3).** `n := &m[k]` is legal, creates a
unique loan on slot `k`, live from creation to last use (non-lexical). It is
**invalidated by any `insert`/`delete` on the same map inside the live
window** (a rehash may move the slot) — compile error `HYB-E3102`. Since Go has
no addressable map slots, the emitter lowers every map-slot borrow to
**read-modify-write**: copy the slot out, mutate the copy, store it back at
the window's end. Semantics identical; the loan is a checker fact, not a
runtime pointer.

```hy
// fragment:
fn (g *graph) connect(from node_id, to node_id) {
    n := &g.nodes[from]              // LOAN: unique map-slot borrow
    n.edges = append(n.edges, to)    // last use — loan dies here; no map
}                                    // insert/delete occurred in the window (example 08)
```

**O9 — Go-heap reads are COPIES, not loans (resolves A4 — the load-bearing
divergence from rustc).** Reading a GC value — `v, ok := m[k]`,
`x := s[i]`, `evict := c.order[0]`, ranging over a slice (the header is read
once) — yields a **copy**. Subsequent mutation of the container does not
invalidate the read, and no liveness window is opened.

```hy
// fragment:
if v, ok := c.entries[key]; ok {     // v is a COPY — no loan on c.entries
    c.touch(key)                     // receiver loan AFTER the read: no conflict
    return some(v)                   // example 07 — the exact shape where Rust's
}                                    // map.get would hold a loan and need NLL
```

**Why stated as its own rule:** this is *the* place Hybrid deliberately
diverges from rustc's borrow semantics (a Go heap read cannot be a loan
because the GC heap has no borrow discipline), and it is why example 07
needs zero annotations where the Rust transliteration would lean on NLL. It
is also the documented divergence point for the rustc shadow-translation CI
leg (O16): the shadow encoding must model reads as copies or it will
over-reject.

**O10 — The Go-API escape perimeter (resolves A5).** Normative statement in
C9: escapes into foreign Go code are outside the checked claim; Go's DRF-SC
+ `go test -race` is the backstop; an opt-in diagnostic lint exists. The
honest-claims paragraph (§9.2) carries the wording.

**O11 — Handle parameters are borrow-by-default (resolves A7).** Passing an
L3 handle to a Hybrid function **lends** it: the callee may use it in
borrow mode (and mutate mode, subject to O7), the caller keeps it. Own
transfer happens only at `return` of the handle, at consuming APIs (which
take `move`), and at spawn/send (which require `move`, O5). Mojo-shaped
argument conventions, zero new syntax.

```hy
// fragment:
fn active_names(doc serde_json.value) ([]string, error) { … }  // borrows `doc`;
fn users_json(names []string) serde_json.value { … return v }  // caller keeps it;
                                                               // `return v` owns out
```

**Why:** both Spike-6 readings accepted all ten programs; borrow-by-default
needs no new syntax and matches call-argument views (O3.2). **Reversal:**
own-by-default would require a borrow marker — rejected on syntax economy.

**O12 — No loans in fields or closure captures (resolves A6).** Struct
fields hold owned values, GC values, ids, or **own**-handles — never
borrows, never borrow-handles, never loans. Closures capture GC values,
never live loans. Both are compile errors (`HYB-E3103`) with the fix-it:
"return an id/handle and re-borrow at the use site, or restructure."

```hy
struct iter {
    cur &node        // reject: HYB-E3103 — fields hold owned values, GC values, ids,
}                    //          or own-handles — never borrows (store a node_id)
```

**Why:** this is *why* zero annotations is achievable — the #1
annotation-forcing shape in Rust (a borrow stored in a struct) is designed
out, and the id-referenced arena pattern (example 08) is the sanctioned
replacement. **Reversal:** none within Tier 0 (it would reintroduce
lifetime-escape analysis — Tier 1 territory, cut per SPEC §7 order 3).

### 9.2 The honest claims paragraph — BINDING, QUOTE-SHAPE

> **The safety claim, stated once, in full.** Hybrid's compiler guarantees
> freedom from data races **only for Hybrid-checked sites in the checked
> subset**: every value that crosses a `task` spawn or a channel send is
> verified sendable by the Tier-0 rules of §9, and every loan inside a
> function body is verified conflict-free. Values inside `managed` blocks
> are **outside this guarantee** — there the contract is Go's data-race-free
> sequential-consistency discipline, with `-race` instrumentation as the
> backstop. Values escaping into foreign Go code (net/http handlers, Go-API
> callbacks) are likewise outside it: **Go-API escapes follow Go's DRF-SC
> memory model with `go test -race` as the backstop.** Memory safety on the
> Go side is Go's GC; on the Rust side it is single ownership, moves, and
> function-local borrows; at the boundary it is owning-side handle tables
> with generation tags. No claim is made about `unsafe` code, about foreign
> code the checker cannot see, or about any program the checker accepts in
> error — an accepted unsound program is a P0 bug (SPEC §10), and this
> claim is exactly as strong as the checker and its adversarial corpus
> prove it to be.

This paragraph is normative (Q1 addition 6): marketing, README, and
docs must quote it or weaken nothing beyond it. The `managed` boundary
appears **in the same paragraph** as the claim — never in a footnote.

### 9.3 Sendability mechanics (check-sites, composites, `move`)

**O4.0 — Check-sites (preamble to the classes below).** The two
Hybrid-level check-sites are `task` spawn arguments and channel sends (C5).
At each site, every argument/sent value is classified by O4's sendable
classes; failures are `HYB-E3001` (spawn) / `HYB-E3002` (send) with the fix-it
menu of §9.6. **The spawn-callee proof:** a spawn is proven only when the
callee resolves to a declared top-level `fn` — a static call where nothing
but the arguments crosses. A closure callee (literal or closure-typed
binding) shares its captured environment with the new task and is rejected
(A5, `HYB-E3001`); a callee that does not resolve (an undeclared name, a
method value, a computed callee) is fail-closed unproven (`HYB-E3005`, rule
FC). The explicit hatch (`managed("…", "actor")`) remains the auditable
route for a deliberate actor-loop closure.

**O4.6 — Deep sendability of composites.** A struct/slice/map literal or
value is sendable iff **every reference-typed element/field is itself
sendable**. A fresh literal is *not* automatically safe: `job{data:
sharedMap}` embeds an alias to `sharedMap` and is rejected unless
`sharedMap` is moved/managed/channel-typed. Primitives inside composites
are copies (class 1) and always fine. This is the rule that makes example
03's fresh-literal sends sound rather than lucky.

```hy
results <- result{worker: id, value: j.input * 2}  // ok: fresh literal, primitive fields
jobs <- job{input: v, data: shared_map}            // reject: HYB-E3002 — `shared_map`
                                                   // (line 30) is a live mutable map
                                                   // referenced by the literal
```

**O4.9 — The fail-closed boundary (rule FC; human directive 2026-10-13,
resolves Finding 1 of the customer-feedback review).** A value whose type
the checker **cannot resolve** — an unresolvable name, a binding whose
initializer's shape did not resolve and which carries no ascription, an
expression whose result shape is unknown (including a foreign call the
boundary descriptors do not describe) — is **UNPROVEN** at a check-site:
it is rejected with `HYB-E3005` (rule FC) and **never silently passes as
safe**. What the checker cannot prove, it does not call safe — that is the
whole product premise (§9.2), and a permissive unknown would void it for
the whole program. This deliberately trades rejection rate for guarantee
precision: the human directive accepts more rejected code as the price of
a precise contract ("what exactly does this compiler guarantee about my
code?" now has a sharp answer: *everything accepted was proven; everything
else is named*). The two legitimate escapes, both explicit and auditable:

1. **Make the type visible** — ascribe it at the declaration
   (`var x map[string]int = ...`); the emitter carries the ascribed type,
   so a lying ascription fails Go's own build. Ascription *fills gaps*:
   where the checker's own inference resolves (boundary handles, handles'
   loan windows), inference keeps priority.
2. **Take the hatch** — wrap the site in `managed` (§9.5); the escape is
   linted (`HYB-E8001`) and counted by `--report-managed` exactly like any
   other managed share.

Diagnostics teach the fix (§9.6/O17): the E3005 fix-it menu is ascribe /
route through a known channel / take the managed hatch knowingly.

**O4.10 — Closure bodies: boundary statements are checked; captures are
not (resolves Finding 2).** A `task`/`send`/select-send **statement inside
a closure body is a concurrency boundary in the emitted Go** (a goroutine
spawn, a channel operation) and is checked exactly as in the flat walk —
fail-closed, `managed`-escapable, with the closure's ascribed params and
the enclosing function's bindings as the capture view. Everything ELSE in
a closure body — assignments, calls, captures of mutable state with no
boundary statement — remains outside the checked subset: that is the A5
perimeter of the scoped-claim ruling (QUESTIONS.md Q1; corpus 10's
`http.handler` capture shape stays ACCEPT, CONTESTED under OQ1). The
boundary is documented, not silent; the fourth check-site class that would
own API escapes is still the open question OQ1, unchanged by this rule.

```hy
mux.handle("/x", http.handler(fn(w http.response_writer) {
    ch <- counts        // reject: HYB-E3002 — a send inside a closure is a
                        // real boundary; `counts` is a live mutable map
    managed { ch <- other }   // ok: hatch taken knowingly (E8001, counted)
}))
```

**O5 — `move` and freeze (resolves A1's (b) half).**

- **O5.1 — `move x`** declares an ownership transfer of the binding `x` at
  a spawn/send/assignment/return. After it, `x` is **moved-from**: any use
  is `HYB-E3003` (use-after-move).
- **O5.2 — Freeze-after-send.** When a value is moved or sent, every **live
  alias** in the sending task is frozen: any access through a frozen alias
  — read **or** write — is `HYB-E3003`. Reads are included deliberately: the
  receiver may mutate concurrently, and Go's DRF-SC defines concurrent
  unsynchronized read/write as a race; a writes-only freeze would make the
  §9.2 claim false. *Relaxation is tagged open (D1), pinned by the
  adversarial corpus: program 05 (write-through-moved-binding) and program
  19 (pure-read-through-moved-binding, CONTESTED — flagged for human
  pre-review at G1): if real-world evidence shows read-after-send is
  idiomatic AND receivers can be proven read-only, the rule may relax to
  writes-only as a logged decision — and the §9.2 claims paragraph narrows
  in the same edit.*
- **O5.3 — Fresh literals and immutable values are not frozen** (nothing
  aliased exists to freeze); channels are never frozen (class 2 — sharing
  them is their purpose).
- **O5.4 — `move` applies to whole bindings** (`move counter`), and to
  **own-handle fields** (`move s.conn` — linear values support partial
  moves; the struct field becomes moved-from). `move` on a GC composite's
  field/path (`move x.field` where the field is a slice/map/struct) is
  `HYB-E3004`: the parent binding still aliases the storage, so the freeze
  cannot be sound — fix-it: copy the field, or move the whole value.
  The moved binding must also be exclusively owned: a parameter, method
  receiver, closure capture, or non-channel range variable — or a local
  that shares its storage, including a self-shadowing `m := m` in an inner
  block — is `HYB-E3004` (the caller / collection still holds the storage).
- **O5.5 — No last-use auto-move in MVP.** Sending a *named* mutable value
  that happens to be unused afterward still requires `move` (`jobs <- x`
  is rejected with the "add `move`" fix-it). Automatic last-use moving is
  tagged open (D17): it is exactly the ergonomics question the adversarial
  corpus's idiomatic-pattern cases and the repeated-rejection tripwire
  (§9.7) exist to answer.
- **O5.6 — Returned `const`s are traced interprocedurally; an untraceable
  callee fails closed (issue #2320).** O5.1's exclusivity proof follows a
  call's `return` positions, so a `const` that flows back through a chain of
  wrappers is still the `const` storage: moving such a value is `HYB-E3004`
  and writing through it is `HYB-E3006` (T22), exactly as for the direct
  call. Two boundaries are **fail-closed** (rule FC, O4.9) rather than
  silent: a chain deeper than the checker's interprocedural walk bound, and
  a callee whose body this file cannot see — a function value held in a
  binding (`cb := leaf; m := cb()`), a computed callee, or a struct field
  holding a function. Past either boundary the checker reports **every**
  may-share file-level `const` as a possible return root, so such a value is
  never moved or mutated as if it were freshly allocated. The report is limited
  to results that may **share** (or whose shape did not resolve): a result the
  checker types as a plain value — a chain returning `int`, say — cannot alias
  a `const` and keeps its previous verdict. The price is a
  rejection, never silence. A call into another package or a builtin is
  excluded deliberately: `const`s are file-level, so another file's function
  cannot return one of ours, and builtins return fresh values.

```hy
counter := map[string]int{}
task tally(move counter)          // ok: exclusive transfer; counter now moved-from
total := len(counter)             // reject: HYB-E3003 — `counter` was moved at the
                                  // spawn above (frozen alias; receiver may mutate)
```

### 9.4 A1 — the sendability ruling: strict (c)+(b), PROVISIONAL

**O4 — Sendable classes (resolves A1's (c) half).** A value is **sendable**
at a spawn/send check-site iff it belongs to exactly one of:

| class | values | why safe |
|---|---|---|
| 1 | primitives (`bool`, ints, floats — value copies) | copied; no shared state |
| 2 | channels; atomic-typed values when the curated surface lands (C8) | internally synchronized by construction |
| 3 | `move`d values (O5) | exclusivity transferred; aliases frozen |
| 4 | immutable-by-type values: `string`, unit-only enums, `const` values, `option[T]`/arrays of sendable classes | no mutation possible, so no race |
| 5 | fresh literals whose reference-typed parts are sendable (O4.6) | the receiver's reference is the only one — no alias exists to freeze |
| 6 | `managed` values (O6) | escape hatch — outside the guarantee |

Everything else — **a named mutable GC value** (map, slice, struct,
interface) — is **not sendable**: sharing mutable GC state across tasks
must go through a channel, a copy, an explicit `move`, or `managed`. This
is the strictest reading of the Tier-0 rule text, adopted by the human's
Q1 ruling:

> **Ruling (QUESTIONS.md Q1, 2026-10-12): reading (c)+(b), the strictest.**
> Sendable only if channel/atomic/immutable, owned `move`, or `managed`
> (c) — **plus** freeze-after-send: post-send mutations through live
> aliases are errors (b). Kept as a properly-qualified escalation because
> a language-level promise is expensive to reverse; the ruling's six
> binding additions (adversarial corpus as spec, diagnostic-not-punitive
> errors, auditable `managed` with `--report-managed`, the >~25%
> tripwire, the scoped claim paragraph, recorded alternatives) are
> normative requirements on this chapter, not commentary.

**Why the strictest reading earns its cost (Q1 addition 1):** Go-compatible
(a) would make Hybrid "a worse Go" — the only reason to adopt a new
language over Go is a guarantee Go cannot give, and (c)+(b) is the only
reading under which the §9.2 claim is true.

**⚠️ PROVISIONAL — the decision record (Q1 requirement):** the ten
Spike-6 programs **cannot distinguish the readings** (all ten share only
channels and fresh literals at their send sites — RESULTS.md's most
valuable negative finding), and the adversarial corpus that *can*
distinguish them is being written in parallel
(`tests/checker-corpus/`, treated as THE spec-pinning corpus, Q1 addition
2). The three alternatives are retained here so reversal is one table-read
away:

| reading | rule | rejected because |
|---|---|---|
| **(a) Go-compatible** | any GC value sendable; sender keeps using it; races remain possible (Go's DRF-SC story) | no race-prevention claim = no reason to exist over Go |
| **(b) freeze-after-send alone** | any GC value sendable; sender's subsequent mutations rejected | partial claim only — still permits shared-mutable aliasing at spawn (two tasks aliasing one map before either "sends" it) |
| **(c) safe-by-type alone** | sendable iff channel/atomic/immutable, owned move, or managed | move semantics incomplete — the sender could race on the moved value's surviving aliases |
| **(c)+(b) — ADOPTED** | both, together (O4 + O5) | the only reading under which the §9.2 claim holds; Spike 6's 10/10 zero-annotation pass survives it unchanged |

Reversal procedure: reopen G0 (tripwire, §9.7), select a row, log in
PLAN.md's Decision Log, and revise O4/O5 + the claim paragraph together —
never one without the other.

### 9.5 `managed` — the named escape hatch

**O6 — `managed` blocks suspend the sendability checks, visibly.**

- **O6.1 — Block semantics.** Inside `managed { … }`, spawn/send check-sites
  are not checked; sharing mutable GC state across tasks follows Go's
  DRF-SC discipline (the programmer synchronizes — typically with a Go
  mutex through interop, or by channel discipline anyway). Memory safety
  inside `managed` remains Go's GC (this is a *race* escape, not a memory
  escape: no `unsafe` pointer machinery is granted).
- **O6.2 — Managed-ness taints the value, not just the block.** A value
  *born* inside a `managed` block (or received from one through any
  channel, return, or field) is a **managed value**: it is sendable
  everywhere (O4 class 6), stays outside the §9.2 guarantee for its whole
  lifetime, and the taint propagates through composites (a struct holding
  a managed map is managed). Managed status is **inferred, never spelled**
  — there is no type syntax for it.
- **O6.3 — `--report-managed` is a normative tooling requirement.** From
  the first checker release (Phase 1), `hybrid check --report-managed` and
  `hybrid build --report-managed` MUST print, per package and in total: the
  count of `managed` blocks, the count of spawn/send sites resolved via
  class 6, and their percentage of all spawn/send sites; with `--json` for
  machine consumption. The command always exits 0 (it is an audit report,
  not a gate) and CI is expected to diff it. **Why normative:** Q1
  addition 4 — `managed` must be greppable, auditable, and countable per
  project, "turning the escape hatch into a visible trust metric instead
  of a hidden hole." The keyword itself is greppable; the report makes the
  *usage* countable. **Row 14 amendment (discussion #4):** the report is a
  TABLE of invariants bypassed — per-category counts plus every block's
  reason/category/tickets — not a bare number.
- **O6.4 — The new-block gate (row 14, discussion #4).** A `managed` block
  that is not in the committed baseline (`gates/managed-baseline.json`)
  FAILS CI (`tools/managed_gate.py`); removing or keeping a baselined block
  is always allowed. Re-baselining requires review — the hatch is
  unsafe-by-default, and "a new managed block appeared" is a review event
  exactly like a new `unsafe` block in Rust. Pairs with `--deny-managed`
  (any block fails) and `--report-managed` (the audit table).

```hy
// fragment:
managed {
    counter := map[string]int{}
    task tally(counter)        // unchecked share — DRF-SC contract applies;
    task tally(counter)        // audited by --report-managed, linted, -race backstop
}
```

**Why `managed` exists at all:** cyclic or heavily-shared graphs (SPEC §7
stretch list: "GC-backed escape hatch for cyclic graphs") have no
Tier-0-legal spelling without it; example 08 shows the *checked* idiom
(id-referenced arena) that makes the hatch unnecessary for plain cycles.
**Reversal:** `managed` is cut order #6 (last before never-cut); removing
it entirely is possible if the corpus never uses it — the report will tell
us. [decided: SPEC §7 + Q1 addition 4]

### 9.6 Checker diagnostics — DIAGNOSTIC, NOT PUNITIVE (Q1 addition 3)

**O17 — Every rejection teaches.** Each Tier-0 rejection carries: a stable
code from the registry below, the span, the *reason in ownership terms*,
and a **fix-it suggestion** naming the legal routes (channel it, copy it,
`move` it, or `managed` it — and which one fits this site). Diagnostic
quality is tested like code (SPEC §11 Phase 1: "error message quality" in
unit tests). Real implementation time is budgeted for fix-its — this is a
product requirement, not polish (Q1 addition 3: "People forgive strictness
when the compiler says exactly what to do").

**Diagnostic code registry — canonical format `HYB-Exxxx`** (format decided
2026-10-12 by the lead, grounded in the GALA/Lisette evidence from
`research/competitive-dx-study.md`: GALA's 73 append-only `GALA-Exxxx` codes
with one embedded reference page per code and offline `gala explain`;
Lisette's 544-code category taxonomy with a severity debug-assert and a
machine-applicable Fix/Edit model — the data structure fix-its need):

| range | category | notes |
|---|---|---|
| `HYB-E0nnn` | parse / lexical | |
| `HYB-E1nnn` | resolve (names, imports, visibility) | |
| `HYB-E2nnn` | type errors | |
| `HYB-E3nnn` | ownership & sendability | `E30xx` send/move/freeze; `E31xx` loans |
| `HYB-E4nnn` | nil / zero-value safety | the null-safety feature's codes |
| `HYB-E5nnn` | exhaustiveness & patterns | |
| `HYB-E6nnn` | integer overflow / bounds | |
| `HYB-E7nnn` | boundary (ABI, layout, foreign calls) | |
| `HYB-E8nnn` | lint (warnings-only; can never fail the build) | severity debug-assert per Lisette |
| `HYB-E9nnn` | tooling (build, cache, explain) | |

**Registry discipline (normative, Phase 1):** append-only — codes are never
renumbered and never reused; a retired code leaves a permanent hole (GALA's
retired-but-never-renumbered `E0016` is the precedent). Every code ships
one embedded reference page (cause → examples → fix-it menu), reachable
offline via `hybrid explain HYB-E3001` (aliases: `E3001`, `3001`). The
registry's seed set has grown append-only to 21 codes as of the O4.9
revision (rule FC added `HYB-E3005`). `E3nnn`
rejections MUST carry fix-it suggestions (O17); a JSON diagnostic envelope
(`{"diagnostics":[…]}` — an object, not an array, so it can grow fields)
lands at MVP-0 for tooling consumption.

**Normative example messages** (shape and content are normative; exact
wording may vary without weakening content):

```text
error[HYB-E3001]: value is not sendable at spawn site
  --> pool.hy:41:5
   |
41 |     task worker(counter)
   |          ^^^^^^^ `counter` (declared pool.hy:33:12) is a mutable map
   |                   shared with the spawned task
   |
   = only channels, atomics, immutable values, moved values, and `managed`
     values may cross a spawn (Tier-0 rule O4)
   = help: choose one —
     · route updates through a channel instead of sharing the map
     · copy the data the task needs and send the copy
     · transfer ownership: `task worker(move counter)` (counter must be
       unused afterward — its aliases are frozen)
     · wrap the shared state in a `managed` block (unchecked: Go DRF-SC)
```

```text
error[HYB-E3003]: use of frozen alias
  --> pool.hy:52:9
   |
52 |     total := len(counter)
   |               ^^^^^^^ `counter` was moved to a task at pool.hy:41:5
   |
   = after a value moves to another task, every live alias is frozen: the
     receiver may mutate it concurrently, so reads and writes here would
     be a data race (Tier-0 rule O5)
   = help: snapshot what you need before the spawn (copy the entries into
     a local), or receive results back through a channel
```

```text
error[HYB-E3101]: mutating call conflicts with a live loan
  --> json.hy:19:5
   |
19 |     doc.remove("users")
   |     ^^^^^^^^^^ `doc` is mutably borrowed here
   |
15 |     for u in doc.array("users")? {
   |              -------- the borrow-mode loan on `doc` is live until line 22
   |
   = a mutating call on a handle invalidates outstanding borrow-mode loans
     (Tier-0 rule O7): the element views would alias mutated state
   = help: finish iterating before mutating — move `doc.remove(...)` after
     the loop, or collect the keys to remove and apply them in a second pass
```

### 9.7 The tripwires (Q1 addition 5)

**O18 — Two conditions reopen G0:**

1. **Managed-share tripwire:** if **more than ~25% of the adversarial
   corpus's spawn/send sites** require `managed` to compile, the strict
   reading is failing real Go-shaped programs — reopen G0 with §9.4's table
   and the `--report-managed` data.
2. **Repeated-rejection tripwire:** if reviewers/users **repeatedly hit the
   same rejection on idiomatic patterns** (the anticipated case: a shared
   map + worker pool that idiomatic Go writes with a mutex), the same
   reopening fires, with the pattern attached as a corpus program.

The ~25% figure is the lead's concretization of the human's "~X%" and is
recorded as **tunable** — it is a smoke alarm, not a contract. Without a
tripwire, "provisional" becomes permanent by inertia (Q1 addition 5).
Tripwire evaluation happens at fixed points: end of Phase 1 (checker +
corpus exist), G1, G2 — and any time a reviewer requests it with evidence.

**O19 — The adversarial corpus is THE spec-pinning corpus.**
`tests/checker-corpus/` is being written in parallel with this document and
*before* the checker lands (Q1 addition 2). Its pre-registered expected
verdicts (compile-fail, managed-required, compile-ok) rank equal to this
chapter where they conflict — and where they conflict, both documents get
revised, with the change logged. This document deliberately does not assume
the corpus's contents beyond its charter: adversarial programs targeting
the A1 readings, sender-keeps-using, borrow-returned-from-function, and the
other annotation-forcing shapes Spike 6's corpus lacked
(spikes/spike6-ownership/RESULTS.md "Implications" #1). The ten Spike-6
`.hy` files are the corpus's first *positive* tests, and their mutants
(§14.7) its first negatives.

### 9.8 Layers 2 and 3; the checker engine; the rustc oracle

**O13 — Layer-2 owned values: single owner, moves, function-local borrows
(Mojo-shaped).** Owned values move on assignment/pass/return; use-after-move
is `HYB-E3003`; borrows inside a function body follow O3's forms with
non-lexical liveness; loans never escape functions (O2). In MVP, Layer-2
values surface in Hybrid code **only as own-handles** (L3): the ten
programs never manipulate Rust-side storage directly, and
interfaces/abi-types.md routes every `rust` crate type through a handle.
Direct Layer-2 value syntax (a `rust.vec`-shaped value used by-value in
Hybrid) is therefore `[TBD]`-post-MVP, gated on a corpus program that needs
it. [decided: corpus silence + ABI rule zero]

**O14 — Layer-3 boundary rules (WIT own/borrow shape, per
interfaces/abi-types.md).** **Own** = transfer + destructor-on-drop: the
handle moves (linear); the owning side's refcount reaching zero frees
(Go-side wrappers release via `runtime.AddCleanup` or explicit
CAS-guarded `release` — idempotent, run-any-thread,
generation-protected). **Borrow** = call-scoped loan enforced by the
checker (O7) and, in debug builds, by handle-table lend-count assertions.
Handle ids are `(generation << 32) | (index + 1)` u64s on the Rust side;
a stale generation reads **INVALID, never corrupts** (Spike 3's
TestUseAfterReleaseSentinel). No callbacks cross in v0 (B1); nothing
unlisted in abi-types.md crosses at all (B1, rule zero).

**O15 — The checker engine.** A Polonius-style non-lexical dataflow pass
over Hybrid MIR with ghost borrow operations (`borrow` / `end_borrow`,
Hylo's IR shape), facts: loans, accesses, spawn/send sites, and — Spike 6's
addition — **handle-mode facts (own/borrow/mutate) per boundary call** for
rule O7. Naive rules first; a location-insensitive variant only if
profiling demands it (research/ownership-models.md §5). The MIR is
dumpable (`--emit=mir`, deterministic per SPEC §11) so checker verdicts
are reproducible artifacts.

**O16 — rustc is a CI oracle, never the user-facing checker (Spike 6
verdict, confirmed).** The real bridge crates compile under pinned rustc
1.99.0 anyway; an *optional, CI-only* shadow translation (the E-ids
encoding: GC values/channels as `Copy` ids, own-handles as non-`Copy`
newtypes) is diffed against our checker's verdicts with pre-registered
agreement thresholds (§14.7 discipline). User-facing diagnostics never
originate from rustc. **Why not delegation as the checker:** the
distinguishing programs (shared mutable map across tasks; sender
uses-after-send) have no faithful stock-rustc encoding — delegation either
over-rejects legal Hybrid or silently under-checks (Spike 6's delegation
ceiling; Valen's patched-rustc evidence). The shadow encoding must model
O9 (reads are copies) or it will disagree on exactly the programs that
matter — which is the point of running it.

### 9.9 The `unsafe` boundary

**O20 — `unsafe` is reserved, not implemented, in MVP.** The keyword is
reserved (LX3); the design (post-MVP, stretch tier) follows Hylo's
*named-unsafe* discipline: explicit, greppable, documented with
`Requires:` contracts, never load-bearing for ordinary code. The MVP escape
hatch is `managed` (races) and there is **no** memory-unsafe escape at all:
raw cross-language pointers are forbidden in safe mode, period
(abi-types.md rule; P7/P8 demonstrated the hazards). [decided: hazard
rules; `unsafe` shape TBD post-MVP, D13]

## 10. Modules, visibility, imports, versioning

**M1 — Hybrid packages are Go packages.** One `package name` per file,
one package per directory; the emitted Go tree mirrors the Hybrid tree
1:1 (Spike 2's emission discipline). A Hybrid package compiles to a Go
package in the same module path, so the adoption story (example 09) is a
plain Go import: `import "example.com/app/hybrid"`.

**M2 — Visibility: `pub` or package-private.** `pub` exports; the emitter
maps exported names to Go's Capitalized form with a deterministic
snake→Camel rule (`pub fn validate_email` → `func ValidateEmail`;
`pub struct lru_cache` → `type LruCache`). Unexported names stay
lowercase in the emitted Go (Go's own visibility then enforces the same
boundary). Package-internal visibility is the only level in MVP — no
`crate`-private/friend granularity. [decided: Go model]

**M2b — Package emission (`hyb build --package`, landed with example 09).**
The adoption story is a BUILD MODE, not a different language: the same
pipeline emits `package <name>` with `pub fn` exported (M2's mapping) into
a directory carrying the runtime pieces it needs (hybridge core, per-crate
bindings, the staticlib) — a Go consumer imports it with **no Hybrid
toolchain installed**, exactly spike 2's zero-install route. `?` inside a
package fn whose results end in `error` propagates via the zero-results
return (`return "", err`), and `return _, err` fills the `_` slot with
that result's zero value (E3's sugar; unnamed Go results cannot say `_`).

**M3 — Import forms.**

```hy
// fragment:
import "net/http"              // a Go package from the curated surface (M5)
import "rust/serde_json"       // a pinned Rust crate via the bridge (M6)
import "example.com/app/util"  // another Hybrid package (same module)
```

**M4 — Foreign-name normalization (Go → Hybrid).** Go identifiers arrive
snake_cased by a deterministic rule: split CamelCase at case boundaries and
initialisms (`NewServeMux` → `new_serve_mux`, `URL` → `url`,
`Atoi` → `atoi`); the mapping is recorded in the binding table the
generator emits, and `//line`-style source mapping (Spike 2 E1) points
diagnostics back. If two distinct Go names normalize to the same Hybrid
name, the import is a compile error (`HYB-E1001`) until one is imported
under a spelled alias — the alias form is `[TBD]` (D8, pinned when a real
collision shows up in the curated surface). Rust crate names are already
snake_case (`serde_json`) and pass through unchanged.

**M5 — The curated Go surface.** MVP Hybrid may import a **curated
allowlist** of Go packages, extended as the target programs demand. Pinned
so far (by the ten examples): `fmt`, `os`, `io`, `time`, `sort`,
`strconv`, `net/http`, `httptest` (06/02/03/04/10). Anything outside the
allowlist is a clear compile error naming the allowlist, not a silent
link failure. `sync` (WaitGroup) and `sync/atomic` are `[TBD]` (D4).
**Why an allowlist at all:** binding quality (name normalization,
concurrency flags for the A5 lint, API contract extraction) is per-package
work; promising "any Go package" and delivering broken bindings would be
the dishonest version. The list grows by Decision Log entries.

**M6 — Rust crate imports and pinning.** `import "rust/serde_json"`
resolves the crate through the module manifest (`hybrid.mod`), which pins
**exact versions** (`serde_json = "1.0.140"` — no ranges; reproducibility
is a SPEC §11 Phase-5 requirement, adopted now to avoid a migration).
The manifest also records the bridge descriptor version. Emission: one
Rust staticlib per binary (P4), prebuilt artifacts committed alongside
generated Go source for zero-install consumption (Spike 2's 14/14 story:
a Go developer with no Hybrid toolchain `go build`s the module).

**M7 — Versioning.** Go-side dependencies ride the module's `go.mod`
(the emitted package is an ordinary Go module citizen); Rust-side
dependencies ride `hybrid.mod` + a committed lockfile; the bridge
descriptors are versioned and hashed with a runtime handshake that
**aborts on mismatch** (abi-types.md §5). The Hybrid *language* version
pins to the toolchain (`hybrid` 1.x); the compatibility policy (what may
change in a minor) is `[TBD]` at G1, per SPEC §12's versioning item.

**M8 — No cyclic imports** (Go's rule, kept). Package cycles are
compile errors; the checker's package graph is a DAG.

---

## 11. Integer overflow, bounds checking, numeric conversions

**X1 — Signed integer overflow WRAPS (two's complement, defined).**
`i8 + 1` on `127` yields `-128`, in every build mode, deterministically.
**Why:** Go's rule; Rust's debug-panic/release-wrap split would make
behavior differ between build modes — poison for differential testing
against the `go run` oracle (SPEC §10) and for the never-break-compile-
speed crowd's mental model. **Reversal:** a checked-math lint (not a mode
flip) may flag wraparound-on-purpose patterns post-MVP. [decided: Go
compat; the differential corpus is the pin]

**X2 — Unsigned overflow wraps** (defined, Go); shifts follow Go
(`1 << 64` on u64 = 0; negative shift count at runtime panics).

**X3 — Bounds checks are always on.** `s[i]`, `s[a:b]` with `a > b` or
`b > len(s)`, negative or `> len` indices: runtime panic (Go's message
shape). There is no unchecked indexing in safe mode, no `get_unchecked`
shape, ever — that class belongs to `unsafe` (post-MVP) and the boundary
forbids it regardless.

```hy
s := [1, 2, 3]
x := s[5]          // panics: runtime error: index out of range [5] with length 3
```

**X4 — Conversions are explicit and truncating (Go's `T(x)`).** All
numeric conversions are spelled `T(x)`; narrowing truncates high bits
(defined, Go); float→int truncates toward zero; int→float rounds per Go.
No implicit conversions between distinct numeric types; untyped constants
convert implicitly on first use only.

```hy
pi := 3.9
n  := int(pi)               // expect: 3  (truncation toward zero)
b  := u8(300)               // expect: 44 (0x12C truncated to 0x2C)
```

**X5 — Division/modulo by zero panics** (Go, both int and float: float
division by zero yields Inf/NaN per IEEE — X8).

**X6 — No `as`-style cast keyword; no reinterpretation cast.** Bit-level
reinterpretation (f64 ↔ u64 bits) is `[TBD]` post-MVP (needed by hash
implementations porting Go's `math.Float64bits` — the curated surface
will expose that function instead). [decided: keep the cast surface tiny]

**X7 — Word-sized `int`/`uint` cross as `i64`/`u64`** in descriptors on
LP64 targets (B9); the language-level `int` remains Go's `int`
(platform-word, distinct type from `i64`).

**X8 — Float semantics are Go's**: IEEE-754 binary32/64; NaN ≠ NaN;
no `-ffast-math`-shaped anything; the emitter must not enable Go
compiler flags that change float semantics. [decided: differential-oracle
safety]

**X9 — No 128-bit integers in MVP.** The boundary set stops at 64-bit
(abi-types.md); `u128`/`i128` in Rust crate signatures make that function
non-crossable (rule zero) until a corpus program demands the layout work.
[decided: ABI alignment]

---

## 12. Memory layout guarantees for boundary-crossing types

This chapter aligns 1:1 with `interfaces/abi-types.md` (draft v0.1, becomes
authoritative at G1). **Rule zero: anything not listed there may not cross
the boundary in safe mode** — this chapter restates the rules as language
guarantees and adds the struct-layout rules the ABI doc implies.

**B1 — Rule zero (crossing allowlist).** Only the following cross; a
descriptor referencing anything else fails generation:

| Hybrid type | crossing | layout |
|---|---|---|
| `u8..u64`, `i8..i64`, `f32`, `f64`, `bool` | by value in registers | C-compatible |
| `int`, `uint` | by value | normalized `i64`/`u64` on LP64 (B9) |
| `string` ≤ 16 bytes | by value (copied) | two words: ptr+len |
| `string` > 16 bytes, `[]T` buffers | pinned `(ptr, len)` view for ONE call | never stored across calls |
| small structs/arrays (≤ 4 words, no interior Go pointers) | by value (copied) | generated C-compatible layout (B2) |
| maps, channels, interfaces, closures, sets, goroutine-related values | opaque Go-side handle | `u64` table id |
| `rust` crate objects | opaque Rust-side handle | `u64 = (gen << 32) \| (index+1)` |
| `error` values | per Spike 5: two-word HyResult + sentinel table (E7) | never a raw pointer |
| function pointers / callbacks | **forbidden in v0** | — |

**B2 — Struct layout for boundary structs is repr(C)-equivalent.** Fields
in **declaration order**, each at its **natural C alignment**, struct size
padded to the alignment of its widest member; no reordering, no packing,
no bitfields (post-MVP `[TBD]` for an explicit packed escape). The
compiler computes the layout once from the versioned descriptor and emits
the matching `#[repr(C)]` shape on the Rust side — both sides agree by
construction, and the runtime handshake aborts on hash mismatch. Interior
Go pointers (slices/maps/chans/interfaces/strings-as-reference) in a
by-value struct are forbidden (`HYB-E7001`); route them as fields per their
own rules or keep the struct Go-side.

**B3 — Alignment and size of primitives**: C rules (`u8` 1/1, `u16` 2/2,
`u32`/`f32` 4/4, `u64`/`f64`/pointers 8/8 on the MVP targets). `bool` is
one byte, `0` or `1` only (the Rust side may assume it).

**B4 — Strings and slices as views.** A `>16-byte` string or a `[]T`
crossing is a `(ptr, len)` view valid **for the duration of one call**:
pinned (copy-small/pin-large per Spike 3's P3 fallback — `runtime.Pinner`
~55 ns, so pin once per call *loop* where the generator can prove it),
unpinned + `KeepAlive` after, never stored. The callee's Rust signature
sees `&[T]` with no `'static`. Writes through a view are allowed only for
explicit out-parameters (example 10's digest buffer).

**B5 — Handles are values.** A handle id is a `u64` copied by value; ids
never dangle (table entries are GC roots or refcounted; stale generations
read INVALID — B6/O14). Passing a handle back to its owning side costs
one table lookup (~0.84 ns measured, Spike 3 P2).

**B6 — Generation tags (Rust-side handles).** `u64 = (generation << 32) |
(index + 1)`; release is CAS-guarded and idempotent; a late cleanup on a
reused slot reads INVALID, never corrupts (verified:
TestUseAfterReleaseSentinel).

**B7 — Zero values cross as C zeroes**: a by-value boundary struct's
Hybrid zero value is the all-zero-bytes C struct; `""` is ptr=non-nil-or-
null+len=0 (len is what matters; ptr of an empty string is unspecified —
the callee must check `len` first).

**B8 — Endianness and targets:** little-endian only for MVP (linux
amd64 + arm64 per SPEC §12's platform list); a big-endian target
re-opens this chapter. arm64 carries the rust2go LR-clobber risk noted in
abi-types.md §6 — port after amd64 locks.

**B9 — `int`/`uint`/`usize` normalization** at the descriptor level
(X7); the descriptor records the normalized type, not the alias.

**B10 — Descriptors are versioned + hashed**; the runtime handshake
aborts on mismatch (abi-types.md §5). This is the ABI-stability story for
a world where Rust has no stable ABI: stability is *ours*, provided by
the pinned descriptor format, not Rust's.

## 13. Closing: examples-are-tests, open decisions, pointers

### 13.1 Spec examples are tests (SPEC §14.4, binding on this document)

Extraction is marker-driven, over `.hy` code blocks only:

- blocks containing `// expect: …` lines are **runnable test-claims**: the
  Phase-1 CI extractor runs each and diffs stdout against the `expect` lines;
  a block that leans on declarations shown in an earlier block of the same
  chapter is concatenated with them (the extractor walks chapters in order,
  so spec examples stay short without becoming ambiguous);
- blocks containing `// reject:` lines are **compile-fail test-claims**: the
  extractor asserts the stated diagnostic code is emitted (a block may mix
  legal and illegal lines — the claim is the reject);
- blocks containing `// panics:` lines are **panic-contract claims** (bounds
  checks X3, unwrap T20, close-on-closed-channel C3): run, assert the panic
  text;
- blocks marked `// fragment:` — and blocks with none of the three markers —
  are illustrations, not extracted.

The expected outputs stated inline are **part of the rules they
demonstrate**: changing an `expect` line is a spec change and gets logged.
Where a rule has no executable example in this document, the checker
corpus supplies it — the ten Spike-6 `.hy` files (with their
byte-for-byte `expected.txt` outputs) are this spec's largest existing
test suite, and the reference interpreter (SPEC §14.4) is the oracle for
every Hybrid-only feature (ownership, enums, no-nil, `?`), since `go run`
cannot be.

### 13.2 Decisions still open (ordered by risk to the MVP; each tagged with its pin)

| # | decision | where | pinned by |
|---|---|---|---|
| D1 | freeze semantics: reads banned with writes (O5.2) — relax to writes-only? | §9.3 | corpus programs 05 (write) + **19 (pure read, CONTESTED — human pre-review at G1)** + 20 (alias write, uncontested) |
| D2 | `task` of a closure literal: capture semantics at spawn | §8.1 | checker corpus closure-spawn program |
| D3 | foreign-Go nil dereference: lint → hard error? | §3.1 N2 | examples 02/06 + differential corpus nil-panic cases |
| D4 | curated Go surface: `sync` (WaitGroup), `sync/atomic` | §10 M5 | example 03 (counting join is the pinned MVP shape); any corpus atomics program |
| D5 | A5 perimeter: promote the go-escape lint to a fourth check-site? | §8.5 | example 06 + Phase-3 `-race` legs + corpus go-escape races → **PROMOTED (W-016 M2, PR #2195)** |
| D6 | `result[T, E]` as a library enum post-MVP | §2.11 | positioning evidence (r/rust audience); no MVP pin by design |
| D7 | monomorphize marked hot generic instantiations | §4 G2 | example 07 macro-benchmark (Phase 4) |
| D8 | import alias syntax for Go-name normalization collisions | §10 M4 | first real collision in the curated surface |
| D9 | `match` as an expression | §2.5 T14 | checker-corpus ergonomics pass |
| D10 | equality for payload-carrying enums | §2.5 T16 | checker-corpus case |
| D11 | map zero-value auto-alloc guard: keep (N5) vs Go-exact | §3.1 N5 | differential corpus perf + behavior diff |
| D12 | `?`-exit runs defers (E6) vs `os.exit` shape | §6.2 | example 02 exit-path test at MVP-0 |
| D13 | `unsafe` block design | §9.9 | post-MVP; a program that `managed` cannot express |
| D14 | corpus normalization: `let x := e` (spike-6 files) folds into `let x = e` | §1.3 | MVP-0, when the ten `.hy` files become compiler input |
| D15 | `switch` port vs literal-patterns in `match` | §1.5 | differential corpus (Go shared subset) |
| D16 | `errors.as` surface | §6.1 E2 | an example/corpus program needing type-directed error extraction |
| D17 | last-use auto-move at send sites (O5.5) | §9.3 | adversarial corpus idiomatic-pattern cases + repeated-rejection tripwire |

D1, D5, and D17 are the ones that can move language-level promises; all
three are wired to the adversarial corpus and the tripwires (§9.7), so they
cannot drift silently.

### 13.3 The A1–A8 resolution index (one line each)

| ambiguity (spike 6) | resolution | rule |
|---|---|---|
| **A1** sendability of mutable GC values | **strict (c)+(b)**: safe-by-type classes + freeze-after-send; PROVISIONAL with alternatives table | O4, O5 (§9.3–9.4) |
| **A2** handle-loan rule (was missing) | borrow-mode uses create non-lexical loans on the handle; mutating-mode calls conflict with live borrow loans | O7 (§9.1) |
| **A3** map-slot borrows `&m[k]` | legal, unique, non-lexical; invalidated by insert/delete in the window; lowered to read-modify-write | O8 (§9.1) |
| **A4** Go-heap reads | **copies, not loans** — the load-bearing divergence from rustc | O9 (§9.1) |
| **A5** Go-API escape perimeter | scoped claim + `-race` backstop + opt-in lint; no fourth check-site in MVP | C9 / O10 (§8.5, §9.2) |
| **A6** borrows in struct fields | forbidden — fields hold owned values, GC values, ids, or own-handles; same for closure captures | O12 (§9.1) |
| **A7** handle-parameter mode | **borrow-by-default**; own-transfer at `return`, consuming APIs, and spawn/send | O11 (§9.1) |
| **A8** top-level `?` exit contract | defers run, then `<prog>: {err}` to stderr, exit 1 | E6 (§6.2) |

### 13.4 Pointers

- **QUESTIONS.md** — Q1's full ruling text (the six binding additions and
  the provisional-marking requirement) is the source of §9's normative
  shape; Q2's reclassified thread-budget design is C6.
- **tests/checker-corpus/** — THE spec-pinning adversarial corpus, being
  written in parallel and *before* the checker lands (Q1 addition 2).
  This document intentionally does not assume its contents beyond the
  charter recorded in O19.
- **research/positioning.md** — the human-supplied strategy that orders
  §2's priorities and constrains syntax (Go-feel, never Rust syntax,
  never break compile speed).
- **research/ownership-models.md** — Tier 0's intellectual provenance
  (three layers, Polonius engine, Mojo conventions, rustc-oracle verdict).
- **research/spike-report.md** — the Phase-0 chosen design this spec must
  stay consistent with (emission, crossing, handles, HyResult, two-tier
  scheduling, Tier 0).
- **interfaces/abi-types.md** — §12's authoritative detail (rule zero;
  becomes binding at G1).
- **spikes/spike6-ownership/** — RESULTS.md (A1–A8, the count matrix, the
  corpus-narrowness finding) and `work/*.hy` (the ten programs that are
  the checker's first positive tests).
- **PLAN.md** — Design Decisions in Force and the Decision Log (every
  reversal path this document references).

---

*End of first draft. The next revision is triggered by: the adversarial
checker corpus landing, MVP-0's walking skeleton, or a tripwire (§9.7) —
whichever comes first.*









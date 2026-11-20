//! alloc_budget.rs - Compositional heap-allocation model and opt-in budget checker (#4492).
//!
//! Milestone 1 + static checker pass for Checked Allocation in Hybrid's checked subset:
//!   * Summary lattice (`AllocSummary`):
//!       - `Const(u64)`: at most `k` dynamic heap allocations along any execution path
//!       - `Linear { param, coeff, base }`: at most `coeff * |param| + base` heap allocations
//!         in a single unmodified parameter of the enclosing function
//!       - `Unbounded`: data-dependent, loop-amplified, recursive, or unmodeled foreign
//!         allocation (fail-closed, mirroring rule O4.9 / FC)
//!   * Tracked Go heap-allocation forms:
//!       - Map literals (`map[K]V{...}`), `set[T]{...}`, map/set inserts, N5 zero-map inits
//!       - Slice literals (`[]T{...}`) and `append` growth (fixed elements vs spread)
//!       - `make` with compile-time constant vs runtime length/capacity
//!       - String interpolation (`fmt.Sprintf` lowering), runtime string `+` concatenation,
//!         and runtime `string(...)` / `[]byte(...)` / `[]rune(...)` conversions
//!       - Heap-escaping `new(T)`, non-slot-loan `&expr`, interface `any` boxing, and
//!         capturing / escaping closures (`FnLit`)
//!       - Loop amplification: `Const(0)` body stays `Const(0)`; non-zero body scales by
//!         constant trip count `K`, lifts to `Linear` over an unmodified parameter bound,
//!         or widens fail-closed to `Unbounded` on `loop` / `for cond` / channel range
//!       - Interprocedural composition across calls, methods, local closures, and `task`
//!         spawns via Tarjan SCC decomposition (allocating recursion -> `Unbounded`)
//!       - Explicit `managed("reason", "category") { ... }` hatch accounting: sites inside
//!         `managed` are counted in `managed_sites` and `total_summary`, and exempted from
//!         the checked-subset bound (`checked_summary`) unless `--deny-managed` rejects them.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::path::Path;
use std::time::Instant;

use hy_diag::{AllocAuditReport, Diagnostic, FnAllocAudit, Span};
use hy_syntax::{
    AssignOp, BinOp, Block, CallArg, Decl, Expr, File, FnDecl, ForKind, InterpPart, LitKind,
    LocalDecl, LocalKind, Place, PlaceKind, SelectArmKind, Stmt, TypeExpr, UnOp,
};

use crate::const_overflow::ConstFold;
use crate::foreign;
use crate::model::{unwrap_parens, Checker, FnState, TypeShape};

// ---------------------------------------------------------------------------
// Summary lattice & budget language
// ---------------------------------------------------------------------------

/// Compositional upper bound on dynamic Go heap allocations for a region or function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AllocSummary {
    /// Provably bounded by a constant number of dynamic heap allocations.
    Const(u64),
    /// Provably bounded by `coeff * |param| + base` heap allocations in one
    /// unmodified parameter (`int` count or `len(param)` of a collection/string).
    Linear {
        param: String,
        coeff: u64,
        base: u64,
    },
    /// Cannot be certified as `Const` or single-parameter `Linear` (fail-closed).
    Unbounded,
}

impl Default for AllocSummary {
    fn default() -> Self {
        AllocSummary::Const(0)
    }
}

impl AllocSummary {
    pub const fn zero() -> Self {
        AllocSummary::Const(0)
    }

    pub fn linear(param: impl Into<String>, coeff: u64, base: u64) -> Self {
        if coeff == 0 {
            AllocSummary::Const(base)
        } else {
            AllocSummary::Linear {
                param: param.into(),
                coeff,
                base,
            }
        }
    }

    pub fn is_zero(&self) -> bool {
        matches!(self, AllocSummary::Const(0))
    }

    pub fn is_const(&self) -> bool {
        matches!(self, AllocSummary::Const(_))
    }

    pub fn is_bounded(&self) -> bool {
        !matches!(self, AllocSummary::Unbounded)
    }

    /// Sequential composition (`A` followed by `B`).
    pub fn seq(&self, other: &Self) -> Self {
        match (self, other) {
            (AllocSummary::Unbounded, _) | (_, AllocSummary::Unbounded) => AllocSummary::Unbounded,
            (AllocSummary::Const(a), AllocSummary::Const(b)) => {
                AllocSummary::Const(a.saturating_add(*b))
            }
            (AllocSummary::Const(a), AllocSummary::Linear { param, coeff, base })
            | (AllocSummary::Linear { param, coeff, base }, AllocSummary::Const(a)) => {
                AllocSummary::linear(param.clone(), *coeff, base.saturating_add(*a))
            }
            (
                AllocSummary::Linear {
                    param: p1,
                    coeff: c1,
                    base: b1,
                },
                AllocSummary::Linear {
                    param: p2,
                    coeff: c2,
                    base: b2,
                },
            ) => {
                if p1 == p2 {
                    AllocSummary::linear(p1.clone(), c1.saturating_add(*c2), b1.saturating_add(*b2))
                } else {
                    AllocSummary::Unbounded
                }
            }
        }
    }

    /// Branch / path join (`if`/`else`, `match`, `select`): pointwise upper bound
    /// valid for every non-negative parameter magnitude.
    pub fn join(&self, other: &Self) -> Self {
        match (self, other) {
            (AllocSummary::Unbounded, _) | (_, AllocSummary::Unbounded) => AllocSummary::Unbounded,
            (AllocSummary::Const(a), AllocSummary::Const(b)) => AllocSummary::Const((*a).max(*b)),
            (AllocSummary::Const(a), AllocSummary::Linear { param, coeff, base })
            | (AllocSummary::Linear { param, coeff, base }, AllocSummary::Const(a)) => {
                AllocSummary::linear(param.clone(), *coeff, (*base).max(*a))
            }
            (
                AllocSummary::Linear {
                    param: p1,
                    coeff: c1,
                    base: b1,
                },
                AllocSummary::Linear {
                    param: p2,
                    coeff: c2,
                    base: b2,
                },
            ) => {
                if p1 == p2 {
                    AllocSummary::linear(p1.clone(), (*c1).max(*c2), (*b1).max(*b2))
                } else {
                    AllocSummary::Unbounded
                }
            }
        }
    }

    /// Amplify a loop body summary by a known compile-time constant trip count `k`.
    pub fn amplify_const(&self, k: u64) -> Self {
        if k == 0 || self.is_zero() {
            return AllocSummary::Const(0);
        }
        match self {
            AllocSummary::Const(b) => AllocSummary::Const(b.saturating_mul(k)),
            AllocSummary::Linear { param, coeff, base } => AllocSummary::linear(
                param.clone(),
                coeff.saturating_mul(k),
                base.saturating_mul(k),
            ),
            AllocSummary::Unbounded => AllocSummary::Unbounded,
        }
    }

    /// Amplify a loop body summary over `[0, |param| + extra_trips)`.
    pub fn amplify_param(&self, param: &str, extra_trips: u64) -> Self {
        if self.is_zero() {
            return AllocSummary::Const(0);
        }
        match self {
            AllocSummary::Const(b) => {
                AllocSummary::linear(param.to_string(), *b, b.saturating_mul(extra_trips))
            }
            // Nested parameter loops are quadratic or bilinear -> Unbounded in M1.
            AllocSummary::Linear { .. } | AllocSummary::Unbounded => AllocSummary::Unbounded,
        }
    }

    /// Does this proven summary satisfy `budget`?
    pub fn satisfies(&self, budget: &HeapBudget) -> bool {
        match (self, budget) {
            (AllocSummary::Unbounded, _) => false,
            (_, HeapBudget::Bounded) => true,
            (AllocSummary::Const(actual), HeapBudget::Const(max)) => actual <= max,
            (AllocSummary::Linear { .. }, HeapBudget::Const(_)) => false,
            (AllocSummary::Const(actual), HeapBudget::Linear { max_base, .. }) => {
                max_base.is_none_or(|mb| *actual <= mb)
            }
            (
                AllocSummary::Linear { param, coeff, base },
                HeapBudget::Linear {
                    param: want_param,
                    max_coeff,
                    max_base,
                },
            ) => {
                let param_ok = want_param.as_ref().is_none_or(|wp| wp == param);
                let coeff_ok = max_coeff.is_none_or(|mc| *coeff <= mc);
                let base_ok = max_base.is_none_or(|mb| *base <= mb);
                param_ok && coeff_ok && base_ok
            }
        }
    }
}

impl fmt::Display for AllocSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AllocSummary::Const(n) => write!(f, "const({n})"),
            AllocSummary::Linear { param, coeff, base } => {
                if *base == 0 {
                    write!(f, "linear({coeff} * {param})")
                } else {
                    write!(f, "linear({coeff} * {param} + {base})")
                }
            }
            AllocSummary::Unbounded => write!(f, "unbounded"),
        }
    }
}

/// Declared heap-allocation budget against which an `AllocSummary` is checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeapBudget {
    /// Require `Const(k)` with `k <= max`.
    Const(u64),
    /// Require `Const` or `Linear` matching optional parameter, coeff, and base caps.
    Linear {
        param: Option<String>,
        max_coeff: Option<u64>,
        max_base: Option<u64>,
    },
    /// Require any bounded summary (`Const` or `Linear`); reject `Unbounded`.
    Bounded,
}

impl fmt::Display for HeapBudget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HeapBudget::Const(n) => write!(f, "const({n})"),
            HeapBudget::Bounded => write!(f, "bounded"),
            HeapBudget::Linear {
                param,
                max_coeff,
                max_base,
            } => match (param, max_coeff, max_base) {
                (None, None, None) => write!(f, "linear"),
                (Some(p), None, None) => write!(f, "linear({p})"),
                (Some(p), Some(c), None) => write!(f, "linear({c} * {p})"),
                (Some(p), Some(c), Some(b)) => write!(f, "linear({c} * {p} + {b})"),
                (p, c, b) => write!(
                    f,
                    "linear(param={}, coeff={}, base={})",
                    p.as_deref().unwrap_or("*"),
                    c.map(|v| v.to_string()).unwrap_or_else(|| "*".into()),
                    b.map(|v| v.to_string()).unwrap_or_else(|| "*".into())
                ),
            },
        }
    }
}

/// Parse a budget specification string from CLI (`--heap-budget <spec>`) or pragma.
///
/// Supported spellings:
///   * `<N>` or `const(<N>)`: e.g. `0`, `16`, `const(4)`
///   * `bounded`: permits any `Const` or `Linear`, rejects `Unbounded`
///   * `linear`: permits any `Const` or `Linear`
///   * `linear(<param>)`: permits `Const` or `Linear` in `<param>`
///   * `linear(<param>, <coeff>, <base>)` or `linear:<param>:<coeff>+<base>`
pub fn parse_budget_spec(raw: &str) -> Result<HeapBudget, String> {
    let s = raw.trim();
    if s.is_empty() {
        return Err("empty heap budget specification".to_string());
    }
    if s == "bounded" {
        return Ok(HeapBudget::Bounded);
    }
    if s == "linear" {
        return Ok(HeapBudget::Linear {
            param: None,
            max_coeff: None,
            max_base: None,
        });
    }
    if let Ok(n) = s.parse::<u64>() {
        return Ok(HeapBudget::Const(n));
    }
    if let Some(inner) = s.strip_prefix("const(").and_then(|r| r.strip_suffix(')')) {
        let n = inner
            .trim()
            .parse::<u64>()
            .map_err(|_| format!("invalid integer in heap budget `{s}`"))?;
        return Ok(HeapBudget::Const(n));
    }
    if let Some(inner) = s.strip_prefix("linear(").and_then(|r| r.strip_suffix(')')) {
        let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
        return match parts.as_slice() {
            [""] => Err(format!("missing parameter name in `{s}`")),
            [param] if is_valid_ident(param) => Ok(HeapBudget::Linear {
                param: Some((*param).to_string()),
                max_coeff: None,
                max_base: None,
            }),
            [param, coeff] if is_valid_ident(param) => {
                let c = coeff
                    .parse::<u64>()
                    .map_err(|_| format!("invalid linear coefficient in `{s}`"))?;
                Ok(HeapBudget::Linear {
                    param: Some((*param).to_string()),
                    max_coeff: Some(c),
                    max_base: None,
                })
            }
            [param, coeff, base] if is_valid_ident(param) => {
                let c = coeff
                    .parse::<u64>()
                    .map_err(|_| format!("invalid linear coefficient in `{s}`"))?;
                let b = base
                    .parse::<u64>()
                    .map_err(|_| format!("invalid linear base in `{s}`"))?;
                Ok(HeapBudget::Linear {
                    param: Some((*param).to_string()),
                    max_coeff: Some(c),
                    max_base: Some(b),
                })
            }
            _ => Err(format!(
                "invalid linear heap budget `{s}`; expected `linear(param)` or `linear(param, coeff, base)`"
            )),
        };
    }
    if let Some(rest) = s.strip_prefix("linear:") {
        let segments: Vec<&str> = rest.split(':').collect();
        return match segments.as_slice() {
            [param] if is_valid_ident(param) => Ok(HeapBudget::Linear {
                param: Some((*param).to_string()),
                max_coeff: None,
                max_base: None,
            }),
            [param, expr] if is_valid_ident(param) => {
                if let Some((c_str, b_str)) = expr.split_once('+') {
                    let c = c_str
                        .trim()
                        .parse::<u64>()
                        .map_err(|_| format!("invalid coefficient in `{s}`"))?;
                    let b = b_str
                        .trim()
                        .parse::<u64>()
                        .map_err(|_| format!("invalid base in `{s}`"))?;
                    Ok(HeapBudget::Linear {
                        param: Some((*param).to_string()),
                        max_coeff: Some(c),
                        max_base: Some(b),
                    })
                } else {
                    let c = expr
                        .trim()
                        .parse::<u64>()
                        .map_err(|_| format!("invalid coefficient in `{s}`"))?;
                    Ok(HeapBudget::Linear {
                        param: Some((*param).to_string()),
                        max_coeff: Some(c),
                        max_base: None,
                    })
                }
            }
            _ => Err(format!(
                "invalid linear heap budget `{s}`; expected `linear:<param>` or `linear:<param>:<coeff>+<base>`"
            )),
        };
    }
    Err(format!(
        "invalid heap budget `{s}`; expected `<N>`, `const(<N>)`, `bounded`, `linear(<param>)`, or `<fn>=<budget>`"
    ))
}

/// Parse a CLI `--heap-budget` argument, which may target the whole program (`16`, `bounded`)
/// or a named function (`tally=linear(shard, 1, 1)`).
pub fn parse_cli_budget_arg(raw: &str) -> Result<(Option<String>, HeapBudget), String> {
    let trimmed = raw.trim();
    if let Some((fn_part, spec_part)) = trimmed.split_once('=') {
        let fn_name = fn_part.trim();
        if fn_name.is_empty() || !is_valid_fn_target(fn_name) {
            return Err(format!(
                "invalid function name `{fn_name}` in `--heap-budget {raw}`"
            ));
        }
        let budget = parse_budget_spec(spec_part)?;
        Ok((Some(fn_name.to_string()), budget))
    } else {
        let budget = parse_budget_spec(trimmed)?;
        Ok((None, budget))
    }
}

fn is_valid_ident(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn is_valid_fn_target(s: &str) -> bool {
    s.split('.').all(is_valid_ident)
}

// ---------------------------------------------------------------------------
// Allocation site & report structures
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AllocKind {
    MapLit,
    MapInsert,
    ZeroMapInit,
    SliceLit,
    SliceAppend,
    MakeConst,
    MakeRuntime,
    StringInterp,
    StringConcat,
    StringByteConv,
    NewAlloc,
    AddrEscape,
    ClosureCapture,
    ClosureEscape,
    InterfaceBox,
    TaskSpawn,
    ForeignAlloc,
    ForeignUnmodeled,
}

impl AllocKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            AllocKind::MapLit => "map_lit",
            AllocKind::MapInsert => "map_insert",
            AllocKind::ZeroMapInit => "zero_map_init",
            AllocKind::SliceLit => "slice_lit",
            AllocKind::SliceAppend => "slice_append",
            AllocKind::MakeConst => "make_const",
            AllocKind::MakeRuntime => "make_runtime",
            AllocKind::StringInterp => "string_interp",
            AllocKind::StringConcat => "string_concat",
            AllocKind::StringByteConv => "string_byte_conv",
            AllocKind::NewAlloc => "new_alloc",
            AllocKind::AddrEscape => "addr_escape",
            AllocKind::ClosureCapture => "closure_capture",
            AllocKind::ClosureEscape => "closure_escape",
            AllocKind::InterfaceBox => "interface_box",
            AllocKind::TaskSpawn => "task_spawn",
            AllocKind::ForeignAlloc => "foreign_alloc",
            AllocKind::ForeignUnmodeled => "foreign_unmodeled",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            AllocKind::MapLit => "map/set composite literal",
            AllocKind::MapInsert => "map/set key insertion",
            AllocKind::ZeroMapInit => "N5 zero-value map initialization",
            AllocKind::SliceLit => "slice composite literal",
            AllocKind::SliceAppend => "slice `append` growth",
            AllocKind::MakeConst => "constant-capacity `make`",
            AllocKind::MakeRuntime => "runtime-length `make`",
            AllocKind::StringInterp => "string interpolation (`fmt.Sprintf`)",
            AllocKind::StringConcat => "dynamic string concatenation (`+`)",
            AllocKind::StringByteConv => "runtime string/byte-slice conversion",
            AllocKind::NewAlloc => "`new(T)` heap allocation",
            AllocKind::AddrEscape => "escaping `&` pointer allocation",
            AllocKind::ClosureCapture => "capturing closure literal",
            AllocKind::ClosureEscape => "escaping allocating closure",
            AllocKind::InterfaceBox => "`any` interface boxing",
            AllocKind::TaskSpawn => "`task` goroutine spawn",
            AllocKind::ForeignAlloc => "allocating runtime/stdlib call",
            AllocKind::ForeignUnmodeled => "unmodeled foreign/dynamic call",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AllocSite {
    pub fn_name: String,
    pub span: Span,
    pub kind: AllocKind,
    pub in_managed: bool,
    pub detail: String,
}

#[derive(Debug, Clone)]
pub struct FnAllocReport {
    pub name: String,
    pub span: Span,
    pub checked_summary: AllocSummary,
    pub total_summary: AllocSummary,
    pub declared_budget: Option<HeapBudget>,
    pub checked_sites: usize,
    pub managed_sites: usize,
    pub unbounded_reasons: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct AllocReport {
    pub functions: BTreeMap<String, FnAllocReport>,
    pub program_checked_summary: AllocSummary,
    pub program_total_summary: AllocSummary,
    pub sites: Vec<AllocSite>,
    pub total_sites: usize,
    pub checked_sites: usize,
    pub managed_sites: usize,
    pub by_kind: BTreeMap<String, usize>,
}

impl AllocReport {
    pub fn to_diag_audit(&self) -> AllocAuditReport {
        let functions = self
            .functions
            .values()
            .map(|f| FnAllocAudit {
                name: f.name.clone(),
                checked_bound: f.checked_summary.to_string(),
                total_bound: f.total_summary.to_string(),
                checked_sites: f.checked_sites,
                managed_sites: f.managed_sites,
                declared_budget: f.declared_budget.as_ref().map(ToString::to_string),
            })
            .collect();
        AllocAuditReport {
            program_checked_bound: self.program_checked_summary.to_string(),
            program_total_bound: self.program_total_summary.to_string(),
            total_sites: self.total_sites,
            checked_sites: self.checked_sites,
            managed_sites: self.managed_sites,
            by_kind: self.by_kind.clone(),
            functions,
        }
    }
}

/// Opt-in configuration for enforcing heap-allocation budgets.
#[derive(Debug, Clone, Default)]
pub struct AllocCheckConfig {
    /// Enforce a budget on `main` (if declared) or on every top-level function.
    pub program_budget: Option<HeapBudget>,
    /// Per-function budgets keyed by function name (`tally` or `counter.bump`).
    pub fn_budgets: BTreeMap<String, HeapBudget>,
    /// Require every function in the file to have a bounded (`Const` or `Linear`) summary.
    pub require_bounded_all_fns: bool,
    /// Parse and enforce `// @heap-budget: <spec>` pragmas above `fn` declarations.
    pub honor_source_pragmas: bool,
}

impl AllocCheckConfig {
    pub fn is_active(&self) -> bool {
        self.program_budget.is_some()
            || !self.fn_budgets.is_empty()
            || self.require_bounded_all_fns
            || self.honor_source_pragmas
    }
}

// ---------------------------------------------------------------------------
// Internal dual summary (checked subset vs total including managed)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
struct DualSummary {
    checked: AllocSummary,
    total: AllocSummary,
}

impl DualSummary {
    fn zero() -> Self {
        Self {
            checked: AllocSummary::Const(0),
            total: AllocSummary::Const(0),
        }
    }

    fn site(s: AllocSummary, in_managed: bool) -> Self {
        if in_managed {
            Self {
                checked: AllocSummary::Const(0),
                total: s,
            }
        } else {
            Self {
                checked: s.clone(),
                total: s,
            }
        }
    }

    fn seq(&self, other: &Self) -> Self {
        Self {
            checked: self.checked.seq(&other.checked),
            total: self.total.seq(&other.total),
        }
    }

    fn join(&self, other: &Self) -> Self {
        Self {
            checked: self.checked.join(&other.checked),
            total: self.total.join(&other.total),
        }
    }

    fn amplify_const(&self, k: u64) -> Self {
        Self {
            checked: self.checked.amplify_const(k),
            total: self.total.amplify_const(k),
        }
    }

    fn amplify_param(&self, param: &str, extra_trips: u64) -> Self {
        Self {
            checked: self.checked.amplify_param(param, extra_trips),
            total: self.total.amplify_param(param, extra_trips),
        }
    }

    fn amplify_unbounded(&self) -> Self {
        Self {
            checked: if self.checked.is_zero() {
                AllocSummary::Const(0)
            } else {
                AllocSummary::Unbounded
            },
            total: if self.total.is_zero() {
                AllocSummary::Const(0)
            } else {
                AllocSummary::Unbounded
            },
        }
    }
}

#[derive(Debug, Clone)]
struct LocalClosureInfo {
    params: Vec<String>,
    body_summary: DualSummary,
}

#[derive(Debug, Clone, Default)]
struct AllocEnv {
    /// Parameter names declared on the enclosing function (in signature order).
    params: Vec<String>,
    /// Parameters whose value or collection length is mutated in the function body.
    mutated_params: HashSet<String>,
    /// Scope stack of local bindings (innermost last) so shadows hide outer params.
    scopes: Vec<HashMap<String, TypeShape>>,
    /// Base bindings (receiver + params + named results).
    base_shapes: HashMap<String, TypeShape>,
    ///Base declared TypeExprs where known.
    base_types: HashMap<String, TypeExpr>,
    scope_types: Vec<HashMap<String, TypeExpr>>,
    /// Unmutated local integer constants (`k := 4`, `const k = 4`).
    const_ints: HashMap<String, i128>,
    /// Unmutated local string constants (`s := "abc"`).
    const_strs: HashMap<String, String>,
    /// Unmutated local collections with known constant element count.
    const_lens: HashMap<String, u64>,
    /// Unmutated local bindings holding a map-slot loan (`p := &m[k]`).
    slot_loans: HashSet<String>,
    /// Local bindings known to hold an `option[T]` value (`o := some(x)`).
    option_locals: HashSet<String>,
    /// Unmutated local closures (`f := fn(x int) int { ... }`).
    local_closures: HashMap<String, LocalClosureInfo>,
    /// All local names reassigned anywhere in the function body (conservative).
    reassigned_locals: HashSet<String>,
    /// Nesting depth inside `managed(...) { ... }`.
    managed_depth: usize,
    /// Incrementally maintained `FnState` for zero-clone `Checker::shape_of_expr` queries.
    fn_state: FnState,
}

impl AllocEnv {
    fn in_managed(&self) -> bool {
        self.managed_depth > 0
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
        self.scope_types.push(HashMap::new());
        self.fn_state.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
        self.scope_types.pop();
        self.fn_state.scopes.pop();
    }

    fn define(&mut self, name: &str, shape: TypeShape, ty: Option<TypeExpr>) {
        let binding = crate::model::Binding {
            shape: shape.clone(),
            ty: ty.clone(),
            immutable: false,
            is_let: false,
            managed_taint: false,
            moved: None,
            non_owned_origin: None,
            slot_loan: false,
        };
        if let Some(top) = self.scopes.last_mut() {
            top.insert(name.to_string(), shape);
        } else {
            self.base_shapes.insert(name.to_string(), shape);
        }
        if let Some(t) = ty {
            if let Some(top_t) = self.scope_types.last_mut() {
                top_t.insert(name.to_string(), t);
            } else {
                self.base_types.insert(name.to_string(), t);
            }
        }
        if let Some(top_b) = self.fn_state.scopes.last_mut() {
            top_b.insert(name.to_string(), binding);
        } else {
            self.fn_state.env.insert(name.to_string(), binding);
        }
    }

    fn lookup_shape(&self, name: &str) -> Option<&TypeShape> {
        for sc in self.scopes.iter().rev() {
            if let Some(s) = sc.get(name) {
                return Some(s);
            }
        }
        self.base_shapes.get(name)
    }

    fn lookup_type(&self, name: &str) -> Option<&TypeExpr> {
        for sc in self.scope_types.iter().rev() {
            if let Some(t) = sc.get(name) {
                return Some(t);
            }
        }
        self.base_types.get(name)
    }

    fn is_shadowed(&self, name: &str) -> bool {
        self.scopes.iter().any(|sc| sc.contains_key(name))
    }

    /// True iff `name` resolves directly to an unshadowed, unmodified parameter of this fn.
    fn is_clean_param(&self, name: &str) -> bool {
        !self.is_shadowed(name)
            && self.params.iter().any(|p| p == name)
            && !self.mutated_params.contains(name)
    }

    fn fn_state(&self) -> &FnState {
        &self.fn_state
    }

    /// Record an O8 map-slot loan binding and mirror it into the innermost `FnState` binding.
    fn mark_slot_loan(&mut self, name: &str) {
        self.slot_loans.insert(name.to_string());
        let innermost = self
            .fn_state
            .scopes
            .iter_mut()
            .rev()
            .find_map(|sc| sc.get_mut(name));
        if let Some(b) = innermost {
            b.slot_loan = true;
        } else if let Some(b) = self.fn_state.env.get_mut(name) {
            b.slot_loan = true;
        }
    }
}

// ---------------------------------------------------------------------------
// Analyzer implementation
// ---------------------------------------------------------------------------

pub(crate) fn analyze_file_allocations(
    ck: &Checker<'_>,
    file: &File,
    src: Option<&str>,
) -> AllocReport {
    let pragmas = src
        .map(|s| parse_source_pragmas(s, file))
        .unwrap_or_default();
    let mut fn_list: Vec<(String, &FnDecl)> = Vec::new();
    let mut fn_by_key: HashMap<String, &FnDecl> = HashMap::new();
    for d in &file.decls {
        if let Decl::Fn(f) = d {
            let key = canonical_fn_key(f);
            fn_list.push((key.clone(), f));
            fn_by_key.insert(key, f);
        }
    }

    // Build static call + spawn graph for Tarjan SCC ordering.
    let mut edges: HashMap<String, BTreeMap<String, ()>> = HashMap::new();
    for (key, f) in &fn_list {
        let mut callees = BTreeMap::new();
        if let Some(body) = &f.body {
            collect_static_callees_block(ck, &fn_by_key, body, &mut callees);
        }
        edges.insert(key.clone(), callees);
    }

    let sccs = tarjan_scc(
        &fn_list.iter().map(|(k, _)| k.clone()).collect::<Vec<_>>(),
        &edges,
    );

    let mut finalized: HashMap<String, DualSummary> = HashMap::new();
    let mut fn_reports: BTreeMap<String, FnAllocReport> = BTreeMap::new();
    let mut all_sites: Vec<AllocSite> = Vec::new();

    // `tarjan_scc` yields SCCs in reverse topological order (callees before callers).
    for scc in sccs {
        let is_recursive = scc.len() > 1
            || (scc.len() == 1
                && edges
                    .get(&scc[0])
                    .is_some_and(|cs| cs.contains_key(&scc[0])));

        if !is_recursive {
            let key = &scc[0];
            if let Some(f) = fn_by_key.get(key) {
                let mut sites = Vec::new();
                let mut reasons = Vec::new();
                let sum = analyze_one_fn(
                    ck,
                    &fn_by_key,
                    &finalized,
                    key,
                    f,
                    true,
                    &mut sites,
                    &mut reasons,
                );
                let checked_sites = sites.iter().filter(|s| !s.in_managed).count();
                let managed_sites = sites.iter().filter(|s| s.in_managed).count();
                all_sites.extend(sites);
                fn_reports.insert(
                    key.clone(),
                    FnAllocReport {
                        name: key.clone(),
                        span: f.span,
                        checked_summary: sum.checked.clone(),
                        total_summary: sum.total.clone(),
                        declared_budget: pragmas.get(key).cloned(),
                        checked_sites,
                        managed_sites,
                        unbounded_reasons: dedup_preserving_order(reasons),
                    },
                );
                finalized.insert(key.clone(), sum);
            }
        } else {
            // Recursive SCC: seed intra-SCC callees with Const(0), evaluate two rounds,
            // and widen any non-zero cycle to Unbounded (fail-closed).
            for k in &scc {
                finalized.insert(k.clone(), DualSummary::zero());
            }
            let mut round1: HashMap<String, DualSummary> = HashMap::new();
            for k in &scc {
                if let Some(f) = fn_by_key.get(k) {
                    let mut dummy_sites = Vec::new();
                    let mut dummy_reasons = Vec::new();
                    let s = analyze_one_fn(
                        ck,
                        &fn_by_key,
                        &finalized,
                        k,
                        f,
                        false,
                        &mut dummy_sites,
                        &mut dummy_reasons,
                    );
                    round1.insert(k.clone(), s);
                }
            }
            for (k, s) in &round1 {
                finalized.insert(k.clone(), s.clone());
            }
            let mut any_checked_nonzero = false;
            let mut any_total_nonzero = false;
            for s in round1.values() {
                if !s.checked.is_zero() {
                    any_checked_nonzero = true;
                }
                if !s.total.is_zero() {
                    any_total_nonzero = true;
                }
            }
            for k in &scc {
                if let Some(f) = fn_by_key.get(k) {
                    let mut sites = Vec::new();
                    let mut reasons = Vec::new();
                    let mut sum = analyze_one_fn(
                        ck,
                        &fn_by_key,
                        &finalized,
                        k,
                        f,
                        true,
                        &mut sites,
                        &mut reasons,
                    );
                    if any_checked_nonzero {
                        sum.checked = AllocSummary::Unbounded;
                        reasons.push(format!(
                            "recursive call cycle involving `{k}` performs heap allocation"
                        ));
                    }
                    if any_total_nonzero {
                        sum.total = AllocSummary::Unbounded;
                    }
                    let checked_sites = sites.iter().filter(|s| !s.in_managed).count();
                    let managed_sites = sites.iter().filter(|s| s.in_managed).count();
                    all_sites.extend(sites);
                    fn_reports.insert(
                        k.clone(),
                        FnAllocReport {
                            name: k.clone(),
                            span: f.span,
                            checked_summary: sum.checked.clone(),
                            total_summary: sum.total.clone(),
                            declared_budget: pragmas.get(k).cloned(),
                            checked_sites,
                            managed_sites,
                            unbounded_reasons: dedup_preserving_order(reasons),
                        },
                    );
                    finalized.insert(k.clone(), sum);
                }
            }
        }
    }

    all_sites.sort_by(|a, b| {
        (
            a.span.start.line,
            a.span.start.col,
            a.span.end.line,
            a.span.end.col,
        )
            .cmp(&(
                b.span.start.line,
                b.span.start.col,
                b.span.end.line,
                b.span.end.col,
            ))
    });

    let mut by_kind = BTreeMap::new();
    for s in &all_sites {
        *by_kind.entry(s.kind.as_str().to_string()).or_insert(0usize) += 1;
    }

    let (program_checked_summary, program_total_summary) =
        if let Some(main_fn) = fn_reports.get("main") {
            (
                main_fn.checked_summary.clone(),
                main_fn.total_summary.clone(),
            )
        } else {
            let mut c = AllocSummary::Const(0);
            let mut t = AllocSummary::Const(0);
            for rep in fn_reports.values() {
                c = c.join(&rep.checked_summary);
                t = t.join(&rep.total_summary);
            }
            (c, t)
        };

    let checked_sites = all_sites.iter().filter(|s| !s.in_managed).count();
    let managed_sites = all_sites.iter().filter(|s| s.in_managed).count();
    let total_sites = all_sites.len();

    AllocReport {
        functions: fn_reports,
        program_checked_summary,
        program_total_summary,
        sites: all_sites,
        total_sites,
        checked_sites,
        managed_sites,
        by_kind,
    }
}

/// Enforce `AllocCheckConfig` against an `AllocReport`, returning source-ordered diagnostics.
pub(crate) fn enforce_alloc_budgets(
    file_path: &str,
    report: &AllocReport,
    cfg: &AllocCheckConfig,
) -> Vec<Diagnostic> {
    if !cfg.is_active() {
        return Vec::new();
    }
    let mut diags = Vec::new();
    let mut checked_fns: HashSet<String> = HashSet::new();

    // 1. Explicit per-function CLI/API budgets (`--heap-budget tally=...`).
    for (target, budget) in &cfg.fn_budgets {
        if let Some(rep) = resolve_fn_report(&report.functions, target) {
            checked_fns.insert(rep.name.clone());
            check_one_fn_budget(file_path, rep, &report.sites, budget, &mut diags);
        } else {
            diags.push(
                Diagnostic::error(
                    "HYB-E6007",
                    file_path.to_string(),
                    Span::point(hy_diag::Pos::new(1, 1)),
                    format!("declared heap budget targets unknown function `{target}` (HYB-E6007)"),
                )
                .with_notes(vec![format!(
                    "available functions in `{file_path}`: {}",
                    if report.functions.is_empty() {
                        "(none)".to_string()
                    } else {
                        report
                            .functions
                            .keys()
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(", ")
                    }
                )]),
            );
        }
    }

    // 2. Source pragma budgets (`// @heap-budget: ...`).
    if cfg.honor_source_pragmas {
        for rep in report.functions.values() {
            if let Some(budget) = &rep.declared_budget {
                if checked_fns.insert(rep.name.clone()) {
                    check_one_fn_budget(file_path, rep, &report.sites, budget, &mut diags);
                }
            }
        }
    }

    // 3. Program-level budget (`--heap-budget <N>`): checks `main` if present,
    //    else checks every function not already covered by a specific fn budget.
    if let Some(prog_budget) = &cfg.program_budget {
        if let Some(main_rep) = report.functions.get("main") {
            if !checked_fns.contains("main") {
                checked_fns.insert("main".to_string());
                check_one_fn_budget(file_path, main_rep, &report.sites, prog_budget, &mut diags);
            }
        } else {
            for rep in report.functions.values() {
                if checked_fns.insert(rep.name.clone()) {
                    check_one_fn_budget(file_path, rep, &report.sites, prog_budget, &mut diags);
                }
            }
        }
    }

    // 4. Whole-file boundedness (`--check-alloc`): every remaining function must be
    //    provably bounded (`Const` or `Linear`, never `Unbounded`).
    if cfg.require_bounded_all_fns {
        for rep in report.functions.values() {
            if !checked_fns.contains(&rep.name) {
                check_one_fn_budget(
                    file_path,
                    rep,
                    &report.sites,
                    &HeapBudget::Bounded,
                    &mut diags,
                );
            }
        }
    }

    diags
}

fn resolve_fn_report<'a>(
    functions: &'a BTreeMap<String, FnAllocReport>,
    target: &str,
) -> Option<&'a FnAllocReport> {
    if let Some(exact) = functions.get(target) {
        return Some(exact);
    }
    let matches: Vec<&FnAllocReport> = functions
        .values()
        .filter(|f| f.name.rsplit('.').next() == Some(target))
        .collect();
    if matches.len() == 1 {
        Some(matches[0])
    } else {
        None
    }
}

fn check_one_fn_budget(
    file_path: &str,
    rep: &FnAllocReport,
    all_sites: &[AllocSite],
    budget: &HeapBudget,
    diags: &mut Vec<Diagnostic>,
) {
    if rep.checked_summary.satisfies(budget) {
        return;
    }

    let fn_sites: Vec<&AllocSite> = all_sites
        .iter()
        .filter(|s| s.fn_name == rep.name && !s.in_managed)
        .collect();
    let diag_span = fn_sites.first().map(|s| s.span).unwrap_or(rep.span);

    let mut notes: Vec<String> = Vec::new();
    notes.push(format!(
        "function `{}` has checked heap summary `{}` against declared budget `{}` ({} checked site(s), {} via managed)",
        rep.name, rep.checked_summary, budget, rep.checked_sites, rep.managed_sites
    ));
    for reason in rep.unbounded_reasons.iter().take(4) {
        notes.push(format!("cause: {reason}"));
    }
    for site in fn_sites.iter().take(4) {
        notes.push(format!(
            "site at {}:{}: {} ({})",
            file_path,
            site.span.start,
            site.kind.label(),
            site.detail
        ));
    }

    match &rep.checked_summary {
        AllocSummary::Unbounded => {
            diags.push(
                Diagnostic::error(
                    "HYB-E6008",
                    file_path.to_string(),
                    diag_span,
                    format!(
                        "function `{}` has an unprovable `unbounded` heap allocation under budget `{}` (fail-closed, HYB-E6008)",
                        rep.name, budget
                    ),
                )
                .with_notes(notes),
            );
        }
        bounded => {
            diags.push(
                Diagnostic::error(
                    "HYB-E6007",
                    file_path.to_string(),
                    diag_span,
                    format!(
                        "function `{}` allocates `{}` on the heap, exceeding declared budget `{}` (HYB-E6007)",
                        rep.name, bounded, budget
                    ),
                )
                .with_notes(notes),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Per-function compositional walker
// ---------------------------------------------------------------------------

pub fn canonical_fn_key(f: &FnDecl) -> String {
    if let Some(recv) = &f.receiver {
        if let Some(base) = Checker::base_named_type(&recv.ty) {
            return format!("{base}.{}", f.name);
        }
    }
    f.name.clone()
}

#[allow(clippy::too_many_arguments)]
fn analyze_one_fn(
    ck: &Checker<'_>,
    fn_by_key: &HashMap<String, &FnDecl>,
    summaries: &HashMap<String, DualSummary>,
    fn_key: &str,
    f: &FnDecl,
    record_sites: bool,
    sites: &mut Vec<AllocSite>,
    reasons: &mut Vec<String>,
) -> DualSummary {
    let Some(body) = &f.body else {
        return DualSummary::zero();
    };

    let mut env = AllocEnv::default();
    if let Some(r) = &f.receiver {
        let base_shape = ck.shape_of_type(&r.ty);
        let (shape, ty) = if r.is_pointer {
            (
                TypeShape::Pointer(Box::new(base_shape)),
                TypeExpr::Pointer {
                    to: Box::new(r.ty.clone()),
                    span: r.ty.span(),
                },
            )
        } else {
            (base_shape, r.ty.clone())
        };
        env.params.push(r.name.clone());
        env.define(&r.name, shape, Some(ty));
    }
    for p in &f.params {
        let shape = ck.shape_of_type(&p.ty);
        env.params.push(p.name.clone());
        env.define(&p.name, shape, Some(p.ty.clone()));
    }
    let mut init_sum = DualSummary::zero();
    for r in &f.results {
        if let Some(name) = &r.name {
            if name != "_" {
                let shape = ck.shape_of_type(&r.ty);
                env.define(name, shape, Some(r.ty.clone()));
                // Named result variables start zero-initialized at function entry (N5).
                let z = n5_zero_map_count(ck, &r.ty, 0);
                if !z.is_zero() {
                    if record_sites {
                        sites.push(AllocSite {
                            fn_name: fn_key.to_string(),
                            span: r.ty.span(),
                            kind: AllocKind::ZeroMapInit,
                            in_managed: false,
                            detail: format!("named result `{name}` zero-value map init"),
                        });
                    }
                    init_sum = init_sum.seq(&DualSummary::site(z, false));
                }
            }
        }
    }

    collect_mutations_block(
        &env.base_shapes,
        body,
        &mut env.mutated_params,
        &mut env.reassigned_locals,
    );

    let mut ctx = FnWalkCtx {
        ck,
        fn_by_key,
        summaries,
        fn_key,
        result_types: f.results.iter().map(|r| r.ty.clone()).collect(),
        record_sites,
        sites,
        reasons,
    };

    let body_sum = ctx.walk_block(&mut env, body, false);
    init_sum.seq(&body_sum)
}

struct FnWalkCtx<'a, 'b> {
    ck: &'a Checker<'a>,
    fn_by_key: &'b HashMap<String, &'a FnDecl>,
    summaries: &'b HashMap<String, DualSummary>,
    fn_key: &'b str,
    result_types: Vec<TypeExpr>,
    record_sites: bool,
    sites: &'b mut Vec<AllocSite>,
    reasons: &'b mut Vec<String>,
}

impl<'a, 'b> FnWalkCtx<'a, 'b> {
    fn emit_site(
        &mut self,
        in_managed: bool,
        span: Span,
        kind: AllocKind,
        detail: String,
        bound: AllocSummary,
    ) -> DualSummary {
        if self.record_sites {
            self.sites.push(AllocSite {
                fn_name: self.fn_key.to_string(),
                span,
                kind,
                in_managed,
                detail,
            });
        }
        DualSummary::site(bound, in_managed)
    }

    fn note_unbounded(&mut self, in_managed: bool, msg: String) {
        if !in_managed && self.record_sites {
            self.reasons.push(msg);
        }
    }

    fn walk_block(&mut self, env: &mut AllocEnv, block: &Block, scoped: bool) -> DualSummary {
        if scoped {
            env.push_scope();
        }
        let mut acc = DualSummary::zero();
        for stmt in &block.stmts {
            let s = self.walk_stmt(env, stmt);
            acc = acc.seq(&s);
        }
        if scoped {
            env.pop_scope();
        }
        acc
    }

    fn walk_stmt(&mut self, env: &mut AllocEnv, stmt: &Stmt) -> DualSummary {
        let in_mgd = env.in_managed();
        match stmt {
            Stmt::Local(l) => self.walk_local(env, l),

            Stmt::Assign {
                targets,
                op,
                values,
                span,
            } => {
                let mut acc = DualSummary::zero();
                for t in targets {
                    acc = acc.seq(&self.walk_place_indices(env, t));
                }
                for (i, v) in values.iter().enumerate() {
                    acc = acc.seq(&self.walk_expr(env, v));
                    if let Some(t) = targets.get(i) {
                        if self.place_is_any(env, t) && !self.expr_is_already_any_or_nil(env, v) {
                            let b = self.emit_site(
                                in_mgd,
                                v.span(),
                                AllocKind::InterfaceBox,
                                format!(
                                    "boxing `{}` into `any` target `{}`",
                                    v.render(),
                                    t.render()
                                ),
                                AllocSummary::Const(1),
                            );
                            acc = acc.seq(&b);
                        }
                        if self.expr_is_allocating_local_closure(env, v)
                            && !matches!(t.kind, PlaceKind::Ident(_))
                        {
                            self.note_unbounded(
                                in_mgd,
                                format!(
                                    "allocating closure `{}` stored into `{}` at {}",
                                    v.render(),
                                    t.render(),
                                    v.span().start
                                ),
                            );
                            let esc = self.emit_site(
                                in_mgd,
                                v.span(),
                                AllocKind::ClosureEscape,
                                format!(
                                    "closure `{}` stored into container/field `{}`",
                                    v.render(),
                                    t.render()
                                ),
                                AllocSummary::Unbounded,
                            );
                            acc = acc.seq(&esc);
                        }
                    }
                }
                // Map insertion (`m[k] = v`, `m[k]++`, `m[k] += v`) can allocate a bucket.
                for t in targets {
                    if let PlaceKind::Index { base, .. } = &t.kind {
                        if self.place_is_map(env, base) {
                            let ins = self.emit_site(
                                in_mgd,
                                *span,
                                AllocKind::MapInsert,
                                format!("map slot write `{}`", t.render()),
                                AllocSummary::Const(1),
                            );
                            acc = acc.seq(&ins);
                        }
                    }
                }
                // Dynamic string `+=` concatenation (`s += chunk`).
                if matches!(op, AssignOp::AddAssign) {
                    if let (Some(t), Some(v)) = (targets.first(), values.first()) {
                        if self.place_is_string(env, t) {
                            let cat = self.emit_site(
                                in_mgd,
                                *span,
                                AllocKind::StringConcat,
                                format!(
                                    "string concatenation `{}` += `{}`",
                                    t.render(),
                                    v.render()
                                ),
                                AllocSummary::Const(1),
                            );
                            acc = acc.seq(&cat);
                        }
                    }
                }
                acc
            }

            Stmt::Expr { expr, .. } => self.walk_expr(env, expr),

            Stmt::Send {
                chan, value, span, ..
            } => {
                let mut acc = self.walk_expr(env, chan);
                acc = acc.seq(&self.walk_expr(env, value));
                if self.chan_elem_is_any(env, chan) && !self.expr_is_already_any_or_nil(env, value)
                {
                    let b = self.emit_site(
                        in_mgd,
                        *span,
                        AllocKind::InterfaceBox,
                        format!("boxing `{}` sent on `chan any`", value.render()),
                        AllocSummary::Const(1),
                    );
                    acc = acc.seq(&b);
                }
                acc
            }

            Stmt::If {
                init,
                cond,
                then,
                otherwise,
                ..
            } => {
                env.push_scope();
                let mut prefix = DualSummary::zero();
                if let Some(i) = init {
                    prefix = prefix.seq(&self.walk_stmt(env, i));
                }
                prefix = prefix.seq(&self.walk_expr(env, cond));
                let then_sum = self.walk_block(env, then, true);
                let else_sum = match otherwise {
                    Some(o) => self.walk_stmt(env, o),
                    None => DualSummary::zero(),
                };
                env.pop_scope();
                prefix.seq(&then_sum.join(&else_sum))
            }

            Stmt::For { kind, body, span } => self.walk_for(env, kind, body, *span),

            Stmt::Return { values, .. } => {
                let mut acc = DualSummary::zero();
                for (i, v) in values.iter().enumerate() {
                    acc = acc.seq(&self.walk_expr(env, v));
                    if let Some(rty) = self.result_types.get(i) {
                        if self.type_is_any(rty) && !self.expr_is_already_any_or_nil(env, v) {
                            let b = self.emit_site(
                                in_mgd,
                                v.span(),
                                AllocKind::InterfaceBox,
                                format!("boxing return value `{}` as `any`", v.render()),
                                AllocSummary::Const(1),
                            );
                            acc = acc.seq(&b);
                        }
                    }
                    if self.expr_is_allocating_local_closure(env, v) {
                        self.note_unbounded(
                            in_mgd,
                            format!(
                                "returned closure `{}` at {} allocates when invoked by caller",
                                v.render(),
                                v.span().start
                            ),
                        );
                        let esc = self.emit_site(
                            in_mgd,
                            v.span(),
                            AllocKind::ClosureEscape,
                            format!("returned closure `{}` with allocating body", v.render()),
                            AllocSummary::Unbounded,
                        );
                        acc = acc.seq(&esc);
                    }
                }
                acc
            }

            Stmt::Task { callee, args, span } => {
                let mut acc = DualSummary::zero();
                for a in args {
                    acc = acc.seq(&self.walk_expr(env, &a.value));
                }
                let spawn_site = self.emit_site(
                    in_mgd,
                    *span,
                    AllocKind::TaskSpawn,
                    format!("`task {}(...)` goroutine spawn", callee.render()),
                    AllocSummary::Const(1),
                );
                acc = acc.seq(&spawn_site);
                let callee_sum = self.eval_call_target(env, callee, args, *span, true);
                acc.seq(&callee_sum)
            }

            Stmt::Managed { body, .. } => {
                env.managed_depth += 1;
                let s = self.walk_block(env, body, true);
                env.managed_depth -= 1;
                s
            }

            Stmt::Match { subject, arms, .. } => {
                let subj_sum = self.walk_expr(env, subject);
                let subj_shape = self.ck.shape_of_expr(env.fn_state(), subject);
                let mut arms_join = DualSummary::zero();
                for (idx, arm) in arms.iter().enumerate() {
                    env.push_scope();
                    if let hy_syntax::MatchPattern::Path {
                        binding: Some(bname),
                        ..
                    } = &arm.pattern
                    {
                        env.define(bname, subj_shape.clone(), None);
                    }
                    let mut arm_sum = DualSummary::zero();
                    if let Some(g) = &arm.guard {
                        arm_sum = arm_sum.seq(&self.walk_expr(env, g));
                    }
                    arm_sum = arm_sum.seq(&self.walk_block(env, &arm.body, false));
                    env.pop_scope();
                    arms_join = if idx == 0 {
                        arm_sum
                    } else {
                        arms_join.join(&arm_sum)
                    };
                }
                subj_sum.seq(&arms_join)
            }

            Stmt::Defer { call, .. } => self.walk_expr(env, call),

            Stmt::Break { .. } | Stmt::Continue { .. } => DualSummary::zero(),

            Stmt::Block { block, .. } => self.walk_block(env, block, true),

            Stmt::Select { arms, .. } => {
                let mut arms_join = DualSummary::zero();
                for (idx, arm) in arms.iter().enumerate() {
                    env.push_scope();
                    let mut arm_sum = match &arm.kind {
                        SelectArmKind::Send { chan, value, .. } => {
                            let mut s = self.walk_expr(env, chan).seq(&self.walk_expr(env, value));
                            if self.chan_elem_is_any(env, chan)
                                && !self.expr_is_already_any_or_nil(env, value)
                            {
                                let b = self.emit_site(
                                    in_mgd,
                                    value.span(),
                                    AllocKind::InterfaceBox,
                                    format!("boxing `{}` in select send", value.render()),
                                    AllocSummary::Const(1),
                                );
                                s = s.seq(&b);
                            }
                            s
                        }
                        SelectArmKind::Recv { names, chan } => {
                            let s = self.walk_expr(env, chan);
                            let elem_shape = match self.ck.shape_of_expr(env.fn_state(), chan) {
                                TypeShape::Chan(e) => *e,
                                _ => TypeShape::Unknown,
                            };
                            for (i, n) in names.iter().enumerate() {
                                let sh = if i == 0 {
                                    elem_shape.clone()
                                } else {
                                    TypeShape::Primitive
                                };
                                env.define(n, sh, None);
                            }
                            s
                        }
                        SelectArmKind::Default => DualSummary::zero(),
                    };
                    arm_sum = arm_sum.seq(&self.walk_block(env, &arm.body, false));
                    env.pop_scope();
                    arms_join = if idx == 0 {
                        arm_sum
                    } else {
                        arms_join.join(&arm_sum)
                    };
                }
                arms_join
            }
        }
    }

    fn walk_local(&mut self, env: &mut AllocEnv, l: &LocalDecl) -> DualSummary {
        let in_mgd = env.in_managed();
        let mut acc = DualSummary::zero();

        if l.values.is_empty() {
            if let Some(ty) = &l.ty {
                let shape = self.ck.shape_of_type(ty);
                let z = n5_zero_map_count(self.ck, ty, 0);
                let const_arr_len = self.const_array_len(env, ty);
                for n in &l.names {
                    env.define(n, shape.clone(), Some(ty.clone()));
                    if !env.reassigned_locals.contains(n) {
                        if let Some(len) = const_arr_len {
                            env.const_lens.insert(n.clone(), len);
                        }
                    }
                    if !z.is_zero() {
                        let s = self.emit_site(
                            in_mgd,
                            l.span,
                            AllocKind::ZeroMapInit,
                            format!("`var {n} {}` zero-value map initialization", ty.render()),
                            z.clone(),
                        );
                        acc = acc.seq(&s);
                    }
                }
            }
            return acc;
        }

        let declared_shape = l.ty.as_ref().map(|t| self.ck.shape_of_type(t));
        let declared_any = l.ty.as_ref().is_some_and(|t| self.type_is_any(t));

        for (i, v) in l.values.iter().enumerate() {
            let bound_name = l.names.get(i).map(String::as_str);

            // Detect O8 map-slot loan (`svc := &m[k]`): lowers to stack value copy + store-back,
            // NOT a Go heap pointer escape!
            if let Some(inner_idx) = self.is_map_slot_loan_init(env, v) {
                acc = acc.seq(&self.walk_expr(env, inner_idx));
                let shape = self.ck.shape_of_expr(env.fn_state(), v);
                if let Some(n) = bound_name {
                    env.define(n, shape, l.ty.clone());
                    if !env.reassigned_locals.contains(n) {
                        env.mark_slot_loan(n);
                    }
                }
                continue;
            }

            // Local non-reassigned closure binding (`f := fn(x int) int { ... }`):
            // evaluate its capture cost at definition, and record its body summary for calls.
            if let Expr::FnLit {
                params,
                results,
                body,
                span,
            } = unwrap_parens(v)
            {
                if let Some(n) = bound_name {
                    if !env.reassigned_locals.contains(n) && n != "_" {
                        let (cap_sum, clos_info) =
                            self.analyze_closure_def(env, params, results, body, *span);
                        acc = acc.seq(&cap_sum);
                        env.define(n, TypeShape::Closure, l.ty.clone());
                        env.local_closures.insert(n.to_string(), clos_info);
                        continue;
                    }
                }
            }

            acc = acc.seq(&self.walk_expr(env, v));
            if declared_any && !self.expr_is_already_any_or_nil(env, v) {
                let b = self.emit_site(
                    in_mgd,
                    v.span(),
                    AllocKind::InterfaceBox,
                    format!("boxing `{}` into `any` local", v.render()),
                    AllocSummary::Const(1),
                );
                acc = acc.seq(&b);
            }
        }

        if l.values.len() == 1 && l.names.len() > 1 {
            // Multi-value call unpack (`a, b := two()`).
            let shapes = self.multi_return_shapes(env.fn_state(), &l.values[0], l.names.len());
            for (n, sh) in l.names.iter().zip(shapes) {
                env.define(n, sh, None);
            }
        } else {
            let inferred_shapes: Vec<TypeShape> = (0..l.names.len())
                .map(|i| {
                    let v = l.values.get(i);
                    declared_shape
                        .clone()
                        .or_else(|| v.map(|e| self.ck.shape_of_expr(env.fn_state(), e)))
                        .unwrap_or(TypeShape::Unknown)
                })
                .collect();
            for (i, n) in l.names.iter().enumerate() {
                let v = l.values.get(i);
                env.define(n, inferred_shapes[i].clone(), l.ty.clone());
                if let Some(e) = v {
                    if self.expr_is_option(env, e) {
                        env.option_locals.insert(n.clone());
                    }
                    if !env.reassigned_locals.contains(n) || l.kind == LocalKind::Const {
                        if let Some(iv) = self.eval_const_int(env, e) {
                            env.const_ints.insert(n.clone(), iv);
                        }
                        if let Some(sv) = self.eval_const_str(env, e) {
                            env.const_strs.insert(n.clone(), sv);
                        }
                        if let Some(clen) = self.eval_const_collection_len(env, e) {
                            env.const_lens.insert(n.clone(), clen);
                        }
                    }
                }
            }
        }

        acc
    }

    fn walk_for(
        &mut self,
        env: &mut AllocEnv,
        kind: &ForKind,
        body: &Block,
        span: Span,
    ) -> DualSummary {
        let in_mgd = env.in_managed();
        match kind {
            ForKind::Infinite => {
                let body_sum = self.walk_block(env, body, true);
                if !body_sum.checked.is_zero() {
                    self.note_unbounded(
                        in_mgd,
                        format!(
                            "infinite/unbounded `loop` at {} allocates `{}` per iteration",
                            span.start, body_sum.checked
                        ),
                    );
                }
                body_sum.amplify_unbounded()
            }

            ForKind::While { cond } => {
                let cond_sum = self.walk_expr(env, cond);
                let body_sum = self.walk_block(env, body, true);
                let total_iter = cond_sum.seq(&body_sum);
                if !total_iter.checked.is_zero() {
                    self.note_unbounded(
                        in_mgd,
                        format!(
                            "condition-controlled `for {}` loop at {} allocates `{}` per iteration without a static trip bound",
                            cond.render(),
                            span.start,
                            total_iter.checked
                        ),
                    );
                }
                cond_sum.seq(&total_iter.amplify_unbounded())
            }

            ForKind::Range {
                var,
                lo,
                hi,
                inclusive,
            } => {
                let header_sum = self.walk_expr(env, lo).seq(&self.walk_expr(env, hi));
                let lo_const = self.eval_const_int(env, lo);
                let hi_const = self.eval_const_int(env, hi);
                let hi_param = self.bound_param_of_expr(env, hi);
                let hi_const_len = self.bound_const_len_of_expr(env, hi);

                env.push_scope();
                env.define(var, TypeShape::Primitive, None);
                let body_sum = self.walk_block(env, body, false);
                env.pop_scope();

                if body_sum.checked.is_zero() && body_sum.total.is_zero() {
                    return header_sum;
                }

                // 1. Both bounds are compile-time constants (`0..4`, `0..n_shards`, `0..len(const_arr)`).
                let resolved_hi = hi_const.or_else(|| hi_const_len.map(i128::from));
                if let (Some(l), Some(h)) = (lo_const, resolved_hi) {
                    let trips: u64 = if *inclusive {
                        if h < l {
                            0
                        } else {
                            u64::try_from(h.saturating_sub(l).saturating_add(1)).unwrap_or(u64::MAX)
                        }
                    } else if h <= l {
                        0
                    } else {
                        u64::try_from(h.saturating_sub(l)).unwrap_or(u64::MAX)
                    };
                    return header_sum.seq(&body_sum.amplify_const(trips));
                }

                // 2. Non-negative constant lower bound and parameter upper bound (`0..n` or `0..len(xs)`).
                if let (Some(l), Some(param)) = (lo_const, hi_param) {
                    if l >= 0 {
                        let extra = u64::from(*inclusive);
                        let amp = body_sum.amplify_param(&param, extra);
                        if matches!(amp.checked, AllocSummary::Unbounded)
                            && !body_sum.checked.is_zero()
                        {
                            self.note_unbounded(
                                in_mgd,
                                format!(
                                    "nested or superlinear loop over `{param}` at {} exceeds M1 linear summary",
                                    span.start
                                ),
                            );
                        }
                        return header_sum.seq(&amp);
                    }
                }

                // 3. Otherwise fail-closed to Unbounded when body allocates > 0.
                if !body_sum.checked.is_zero() {
                    self.note_unbounded(
                        in_mgd,
                        format!(
                            "range loop `{}..{}{}` at {} has non-constant, non-parameter bound with allocating body `{}`",
                            lo.render(),
                            if *inclusive { "=" } else { "" },
                            hi.render(),
                            span.start,
                            body_sum.checked
                        ),
                    );
                }
                header_sum.seq(&body_sum.amplify_unbounded())
            }

            ForKind::Iterate { vars, expr } => {
                let header_sum = self.walk_expr(env, expr);
                let iter_shape = self.ck.shape_of_expr(env.fn_state(), expr);
                let is_chan = matches!(iter_shape, TypeShape::Chan(_))
                    || matches!(iter_shape, TypeShape::OwnHandle(ref h) if h == "hy.recv_cap");
                let const_trips = if is_chan {
                    None
                } else {
                    self.eval_const_collection_len(env, expr)
                };
                let param_bound = if is_chan {
                    None
                } else {
                    self.collection_param_of_expr(env, expr)
                };

                let var_shapes = self.ck.iterate_var_shapes(env.fn_state(), expr, vars.len());
                env.push_scope();
                for (vname, vshape) in vars.iter().zip(var_shapes) {
                    env.define(vname, vshape, None);
                }
                let body_sum = self.walk_block(env, body, false);
                env.pop_scope();

                if body_sum.checked.is_zero() && body_sum.total.is_zero() {
                    return header_sum;
                }

                if let Some(k) = const_trips {
                    return header_sum.seq(&body_sum.amplify_const(k));
                }

                if let Some(param) = param_bound {
                    let amp = body_sum.amplify_param(&param, 0);
                    if matches!(amp.checked, AllocSummary::Unbounded) && !body_sum.checked.is_zero()
                    {
                        self.note_unbounded(
                            in_mgd,
                            format!(
                                "nested loop iterating `{param}` at {} exceeds M1 linear summary",
                                span.start
                            ),
                        );
                    }
                    return header_sum.seq(&amp);
                }

                if !body_sum.checked.is_zero() {
                    let kind_desc = if is_chan {
                        format!("channel stream `{}`", expr.render())
                    } else {
                        format!("dynamic collection `{}`", expr.render())
                    };
                    self.note_unbounded(
                        in_mgd,
                        format!(
                            "loop over {kind_desc} at {} allocates `{}` per element without a static bound",
                            span.start, body_sum.checked
                        ),
                    );
                }
                header_sum.seq(&body_sum.amplify_unbounded())
            }
        }
    }

    fn walk_place_indices(&mut self, env: &mut AllocEnv, place: &Place) -> DualSummary {
        match &place.kind {
            PlaceKind::Ident(_) => DualSummary::zero(),
            PlaceKind::Index { base, index } => self
                .walk_place_indices(env, base)
                .seq(&self.walk_expr(env, index)),
            PlaceKind::Field { base, .. } | PlaceKind::AddrOf { base } => {
                self.walk_place_indices(env, base)
            }
        }
    }

    fn walk_expr(&mut self, env: &mut AllocEnv, expr: &Expr) -> DualSummary {
        let in_mgd = env.in_managed();
        match expr {
            Expr::Ident { .. } | Expr::Lit { .. } => DualSummary::zero(),

            Expr::Paren { inner, .. }
            | Expr::Move { operand: inner, .. }
            | Expr::Try { operand: inner, .. } => self.walk_expr(env, inner),

            Expr::Receive { chan, .. } => self.walk_expr(env, chan),

            Expr::Index { base, index, .. } => {
                self.walk_expr(env, base).seq(&self.walk_expr(env, index))
            }

            Expr::Slice { base, lo, hi, .. } => {
                let mut acc = self.walk_expr(env, base);
                if let Some(l) = lo {
                    acc = acc.seq(&self.walk_expr(env, l));
                }
                if let Some(h) = hi {
                    acc = acc.seq(&self.walk_expr(env, h));
                }
                acc
            }

            Expr::Field { base, .. } => self.walk_expr(env, base),

            Expr::Interp { parts, span } => {
                let mut acc = DualSummary::zero();
                let mut expr_count: u64 = 0;
                for part in parts {
                    if let InterpPart::Expr(x) = part {
                        expr_count += 1;
                        acc = acc.seq(&self.walk_expr(env, x));
                    }
                }
                if expr_count > 0 {
                    // Emits `fmt.Sprintf(...)`: allocates formatted output string plus
                    // potential interface boxing per interpolated operand.
                    let s = self.emit_site(
                        in_mgd,
                        *span,
                        AllocKind::StringInterp,
                        format!("interpolated string with {expr_count} expression(s)"),
                        AllocSummary::Const(1 + expr_count),
                    );
                    acc = acc.seq(&s);
                }
                acc
            }

            Expr::Binary { op, lhs, rhs, span } => {
                let mut acc = self.walk_expr(env, lhs).seq(&self.walk_expr(env, rhs));
                if matches!(op, BinOp::Add) {
                    let st = env.fn_state();
                    let is_str = self.ck.shape_of_expr(st, lhs) == TypeShape::Str
                        || self.ck.shape_of_expr(st, rhs) == TypeShape::Str;
                    if is_str {
                        let both_const = self.eval_const_str(env, lhs).is_some()
                            && self.eval_const_str(env, rhs).is_some();
                        if !both_const {
                            let s = self.emit_site(
                                in_mgd,
                                *span,
                                AllocKind::StringConcat,
                                format!("dynamic string `+` (`{}`)", expr.render()),
                                AllocSummary::Const(1),
                            );
                            acc = acc.seq(&s);
                        }
                    }
                }
                acc
            }

            Expr::Unary { op, operand, span } => match op {
                UnOp::Addr => self.walk_addr_of(env, operand, *span),
                UnOp::Not | UnOp::Neg => self.walk_expr(env, operand),
            },

            Expr::AddrOf { operand, span } => self.walk_addr_of(env, operand, *span),

            Expr::StructLit { ty, fields, span } => {
                let mut acc = DualSummary::zero();
                let field_decls = ty.as_ref().and_then(|t| self.struct_field_types(t));
                for f in fields {
                    acc = acc.seq(&self.walk_expr(env, &f.value));
                    if let Some(decls) = &field_decls {
                        if let Some((_, fty)) = decls.iter().find(|(n, _)| n == &f.name) {
                            if self.type_is_any(fty)
                                && !self.expr_is_already_any_or_nil(env, &f.value)
                            {
                                let b = self.emit_site(
                                    in_mgd,
                                    f.span,
                                    AllocKind::InterfaceBox,
                                    format!("boxing field `{}` as `any`", f.name),
                                    AllocSummary::Const(1),
                                );
                                acc = acc.seq(&b);
                            }
                        }
                    }
                    if self.expr_is_allocating_local_closure(env, &f.value) {
                        self.note_unbounded(
                            in_mgd,
                            format!(
                                "allocating closure stored in struct field `{}` at {}",
                                f.name, f.span.start
                            ),
                        );
                        let esc = self.emit_site(
                            in_mgd,
                            f.span,
                            AllocKind::ClosureEscape,
                            format!("closure stored in struct field `{}`", f.name),
                            AllocSummary::Unbounded,
                        );
                        acc = acc.seq(&esc);
                    }
                }
                if let Some(t) = ty {
                    if Self::is_set_type_expr(t) {
                        let buckets = 1 + (fields.len() as u64 / 8);
                        let s = self.emit_site(
                            in_mgd,
                            *span,
                            AllocKind::MapLit,
                            format!("`{}{{...}}` set literal", t.render()),
                            AllocSummary::Const(buckets),
                        );
                        acc = acc.seq(&s);
                    } else if let Some(decls) = &field_decls {
                        let provided: HashSet<&str> =
                            fields.iter().map(|f| f.name.as_str()).collect();
                        let mut missing_maps = AllocSummary::Const(0);
                        for (fname, fty) in decls {
                            if !provided.contains(fname.as_str()) {
                                missing_maps =
                                    missing_maps.seq(&n5_zero_map_count(self.ck, fty, 0));
                            }
                        }
                        if !missing_maps.is_zero() {
                            let s = self.emit_site(
                                in_mgd,
                                *span,
                                AllocKind::ZeroMapInit,
                                format!("omitted map field(s) in `{}{{...}}`", t.render()),
                                missing_maps,
                            );
                            acc = acc.seq(&s);
                        }
                    }
                }
                acc
            }

            Expr::SliceLit { elem, items, span } => {
                let elem_is_any = elem.as_deref().is_some_and(|t| self.type_is_any(t));
                let mut acc = DualSummary::zero();
                for it in items {
                    acc = acc.seq(&self.walk_expr(env, it));
                    if elem_is_any && !self.expr_is_already_any_or_nil(env, it) {
                        let b = self.emit_site(
                            in_mgd,
                            it.span(),
                            AllocKind::InterfaceBox,
                            format!("boxing slice item `{}` as `any`", it.render()),
                            AllocSummary::Const(1),
                        );
                        acc = acc.seq(&b);
                    }
                    if self.expr_is_allocating_local_closure(env, it) {
                        self.note_unbounded(
                            in_mgd,
                            format!(
                                "allocating closure stored in slice literal at {}",
                                it.span().start
                            ),
                        );
                        let esc = self.emit_site(
                            in_mgd,
                            it.span(),
                            AllocKind::ClosureEscape,
                            "allocating closure stored in slice literal".to_string(),
                            AllocSummary::Unbounded,
                        );
                        acc = acc.seq(&esc);
                    }
                }

                // Fixed-size value array `[N]T{...}` is stack-allocated by value,
                // only allocating N5 zero-value maps for trailing omitted slots.
                if let Some((len_opt, inner_elem, _)) = expr.as_array_lit() {
                    let per_elem = n5_zero_map_count(self.ck, inner_elem, 0);
                    if !per_elem.is_zero() {
                        if let Some(len_expr) = len_opt {
                            if let Some(n) = self.eval_const_int(env, len_expr) {
                                let trailing = (n.max(0) as u64).saturating_sub(items.len() as u64);
                                let z = per_elem.amplify_const(trailing);
                                if !z.is_zero() {
                                    let s = self.emit_site(
                                        in_mgd,
                                        *span,
                                        AllocKind::ZeroMapInit,
                                        "array literal trailing N5 zero-map init".to_string(),
                                        z,
                                    );
                                    acc = acc.seq(&s);
                                }
                            } else {
                                let s = self.emit_site(
                                    in_mgd,
                                    *span,
                                    AllocKind::ZeroMapInit,
                                    "non-constant array N5 map init".to_string(),
                                    AllocSummary::Unbounded,
                                );
                                acc = acc.seq(&s);
                            }
                        }
                    }
                    return acc;
                }

                // Slice literal `[]T{...}` allocates backing array (unless empty `[]T{}`? Note:
                // in Go `[]T{}` with 0 items may use zerobase, while escaping or `append([]T{}, ...)`
                // allocates; counting Const(1) for non-empty and Const(1) conservatively is sound).
                let cost = if items.is_empty() {
                    AllocSummary::Const(0)
                } else {
                    AllocSummary::Const(1)
                };
                if !cost.is_zero() {
                    let s = self.emit_site(
                        in_mgd,
                        *span,
                        AllocKind::SliceLit,
                        format!("slice literal with {} element(s)", items.len()),
                        cost,
                    );
                    acc = acc.seq(&s);
                }
                acc
            }

            Expr::MapLit {
                value,
                entries,
                span,
                ..
            } => {
                let val_is_any = self.type_is_any(value);
                let val_n5_maps = n5_zero_map_count(self.ck, value, 0);
                let mut acc = DualSummary::zero();
                for (k, v) in entries {
                    acc = acc
                        .seq(&self.walk_expr(env, k))
                        .seq(&self.walk_expr(env, v));
                    if val_is_any && !self.expr_is_already_any_or_nil(env, v) {
                        let b = self.emit_site(
                            in_mgd,
                            v.span(),
                            AllocKind::InterfaceBox,
                            format!("boxing map value `{}` as `any`", v.render()),
                            AllocSummary::Const(1),
                        );
                        acc = acc.seq(&b);
                    }
                    if matches!(unwrap_parens(v), Expr::StructLit { ty: None, fields, .. } if fields.is_empty())
                        && !val_n5_maps.is_zero()
                    {
                        acc = acc.seq(&DualSummary::site(val_n5_maps.clone(), in_mgd));
                    }
                }
                let buckets = 1 + (entries.len() as u64 / 8);
                let s = self.emit_site(
                    in_mgd,
                    *span,
                    AllocKind::MapLit,
                    format!("map literal with {} entry(s)", entries.len()),
                    AllocSummary::Const(buckets),
                );
                acc.seq(&s)
            }

            Expr::FnLit {
                params,
                results,
                body,
                span,
            } => {
                // Anonymous closure in expression position (not bound to a non-escaping local,
                // and not immediately called, which `walk_call` handles directly):
                // if its body allocates > 0, unknown callers could invoke it arbitrarily often.
                let (cap_sum, info) = self.analyze_closure_def(env, params, results, body, *span);
                if info.body_summary.checked.is_zero() && info.body_summary.total.is_zero() {
                    cap_sum
                } else {
                    self.note_unbounded(
                        in_mgd,
                        format!(
                            "escaping closure literal at {} allocates `{}` inside its body",
                            span.start, info.body_summary.checked
                        ),
                    );
                    let esc = self.emit_site(
                        in_mgd,
                        *span,
                        AllocKind::ClosureEscape,
                        "escaping closure literal with allocating body".to_string(),
                        AllocSummary::Unbounded,
                    );
                    cap_sum.seq(&esc)
                }
            }

            Expr::Call { callee, args, span } => self.walk_call(env, callee, args, *span),
        }
    }

    fn walk_addr_of(&mut self, env: &mut AllocEnv, operand: &Expr, span: Span) -> DualSummary {
        let in_mgd = env.in_managed();
        let inner_sum = self.walk_expr(env, operand);
        let escape = self.emit_site(
            in_mgd,
            span,
            AllocKind::AddrEscape,
            format!("address-of `&{}`", operand.render()),
            AllocSummary::Const(1),
        );
        inner_sum.seq(&escape)
    }

    fn analyze_closure_def(
        &mut self,
        env: &mut AllocEnv,
        params: &[hy_syntax::Param],
        results: &[hy_syntax::FnResult],
        body: &Block,
        span: Span,
    ) -> (DualSummary, LocalClosureInfo) {
        let in_mgd = env.in_managed();
        let param_names: HashSet<&str> = params.iter().map(|p| p.name.as_str()).collect();
        let captures = closure_captures_outer(env, &param_names, body);
        let cap_sum = if captures {
            self.emit_site(
                in_mgd,
                span,
                AllocKind::ClosureCapture,
                "closure environment capture".to_string(),
                AllocSummary::Const(1),
            )
        } else {
            DualSummary::zero()
        };

        let mut cenv = env.clone();
        cenv.params = params.iter().map(|p| p.name.clone()).collect();
        cenv.mutated_params.clear();
        cenv.push_scope();
        for p in params {
            let sh = self.ck.shape_of_type(&p.ty);
            cenv.define(&p.name, sh, Some(p.ty.clone()));
        }
        for r in results {
            if let Some(rn) = &r.name {
                if rn != "_" {
                    let sh = self.ck.shape_of_type(&r.ty);
                    cenv.define(rn, sh, Some(r.ty.clone()));
                }
            }
        }
        collect_mutations_block(
            &cenv.base_shapes,
            body,
            &mut cenv.mutated_params,
            &mut cenv.reassigned_locals,
        );
        let saved_results = std::mem::replace(
            &mut self.result_types,
            results.iter().map(|r| r.ty.clone()).collect(),
        );
        let body_summary = self.walk_block(&mut cenv, body, false);
        self.result_types = saved_results;

        let info = LocalClosureInfo {
            params: params.iter().map(|p| p.name.clone()).collect(),
            body_summary,
        };
        (cap_sum, info)
    }

    fn walk_call(
        &mut self,
        env: &mut AllocEnv,
        raw_callee: &Expr,
        args: &[CallArg],
        span: Span,
    ) -> DualSummary {
        let in_mgd = env.in_managed();
        let callee = unwrap_parens(raw_callee);

        // 0. Immediate closure invocation `fn(...) { ... }(args...)`.
        if let Expr::FnLit {
            params,
            results,
            body,
            span: fn_span,
        } = callee
        {
            let mut acc = DualSummary::zero();
            for a in args {
                acc = acc.seq(&self.walk_expr(env, &a.value));
            }
            let (cap_sum, info) = self.analyze_closure_def(env, params, results, body, *fn_span);
            acc = acc.seq(&cap_sum);
            let sub =
                self.substitute_dual(env, &info.body_summary, &info.params, args, span, "closure");
            return acc.seq(&sub);
        }

        // 1. Slice type-conversion call `[]byte(x)` / `[]rune(x)`:
        //    parses as `Call { callee: SliceLit { elem: Some(T), items: [] }, args }`.
        if let Expr::SliceLit {
            elem: Some(t),
            items,
            ..
        } = callee
        {
            if items.is_empty() && callee.as_array_lit().is_none() {
                let mut acc = DualSummary::zero();
                for a in args {
                    acc = acc.seq(&self.walk_expr(env, &a.value));
                }
                let bound = if let Some(first) = args.first() {
                    if self.eval_const_str(env, &first.value).is_some()
                        || self.eval_const_collection_len(env, &first.value).is_some()
                    {
                        AllocSummary::Const(1)
                    } else if let Some(p) = self.collection_param_of_expr(env, &first.value) {
                        AllocSummary::linear(p, 1, 1)
                    } else {
                        self.note_unbounded(
                            in_mgd,
                            format!(
                                "`[]{}` conversion of dynamic operand `{}` at {}",
                                t.render(),
                                first.value.render(),
                                span.start
                            ),
                        );
                        AllocSummary::Unbounded
                    }
                } else {
                    AllocSummary::Const(0)
                };
                let site = self.emit_site(
                    in_mgd,
                    span,
                    AllocKind::StringByteConv,
                    format!("`[]{}(...)` slice conversion", t.render()),
                    bound,
                );
                return acc.seq(&site);
            }
        }

        // 2. Unshadowed builtins: `make`, `new`, `append`, pure builtins, and primitive/alias conversions.
        if let Expr::Ident { name, .. } = callee {
            if self.is_unshadowed_builtin(env, name) {
                match name.as_str() {
                    "make" => return self.walk_make(env, args, span),
                    "new" => return self.walk_new(env, args, span),
                    "append" => return self.walk_append(env, args, span),
                    "len" | "cap" | "min" | "max" | "delete" | "close" | "clear" | "copy"
                    | "complex" | "real" | "imag" | "some" | "panic" => {
                        let mut acc = DualSummary::zero();
                        for a in args {
                            acc = acc.seq(&self.walk_expr(env, &a.value));
                        }
                        return acc;
                    }
                    "string" => {
                        let mut acc = DualSummary::zero();
                        for a in args {
                            acc = acc.seq(&self.walk_expr(env, &a.value));
                        }
                        if let Some(first) = args.first() {
                            let arg_shape = self.ck.shape_of_expr(env.fn_state(), &first.value);
                            let bound = if self.eval_const_str(env, &first.value).is_some() {
                                AllocSummary::Const(0)
                            } else if matches!(arg_shape, TypeShape::Primitive)
                                || self.eval_const_int(env, &first.value).is_some()
                                || self.eval_const_collection_len(env, &first.value).is_some()
                            {
                                AllocSummary::Const(1)
                            } else if let Some(p) = self.collection_param_of_expr(env, &first.value)
                            {
                                AllocSummary::linear(p, 1, 1)
                            } else {
                                self.note_unbounded(
                                    in_mgd,
                                    format!(
                                        "`string({})` conversion on dynamic buffer at {}",
                                        first.value.render(),
                                        span.start
                                    ),
                                );
                                AllocSummary::Unbounded
                            };
                            if !bound.is_zero() {
                                let site = self.emit_site(
                                    in_mgd,
                                    span,
                                    AllocKind::StringByteConv,
                                    format!("`string({})` conversion", first.value.render()),
                                    bound,
                                );
                                acc = acc.seq(&site);
                            }
                        }
                        return acc;
                    }
                    _ if self.is_primitive_or_alias_conversion(name) => {
                        let mut acc = DualSummary::zero();
                        for a in args {
                            acc = acc.seq(&self.walk_expr(env, &a.value));
                        }
                        return acc;
                    }
                    _ => {}
                }
            }
        }

        // 3. Regular / method / closure / foreign call: evaluate arguments first, then target.
        let mut acc = DualSummary::zero();
        for a in args {
            acc = acc.seq(&self.walk_expr(env, &a.value));
        }
        let target_sum = self.eval_call_target(env, callee, args, span, false);
        acc.seq(&target_sum)
    }

    fn walk_make(&mut self, env: &mut AllocEnv, args: &[CallArg], span: Span) -> DualSummary {
        let in_mgd = env.in_managed();
        let mut acc = DualSummary::zero();
        for a in args.iter().skip(1) {
            acc = acc.seq(&self.walk_expr(env, &a.value));
        }

        let elem_n5_maps = args
            .first()
            .and_then(|first| self.make_slice_elem_type(&first.value))
            .map(|elem_ty| match n5_zero_map_count(self.ck, &elem_ty, 0) {
                AllocSummary::Const(m) => Some(m),
                _ => None,
            })
            .unwrap_or(Some(0));

        // Omitted size (`make(chan T)`, `make(map[K]V)`) -> Const(1).
        if args.len() <= 1 {
            let site = self.emit_site(
                in_mgd,
                span,
                AllocKind::MakeConst,
                "`make(...)` with default capacity".to_string(),
                AllocSummary::Const(1),
            );
            return acc.seq(&site);
        }

        // Check whether size/cap args are compile-time constants or clean parameters.
        let mut all_const = true;
        let mut max_const_dim: u64 = 0;
        let mut param_dim: Option<String> = None;
        let mut multi_or_unknown = false;

        for a in args.iter().skip(1) {
            if let Some(c) = self
                .eval_const_int(env, &a.value)
                .or_else(|| self.bound_const_len_of_expr(env, &a.value).map(i128::from))
            {
                if c > 0 {
                    max_const_dim = max_const_dim.max(u64::try_from(c).unwrap_or(u64::MAX));
                }
            } else {
                all_const = false;
                if let Some(p) = self.bound_param_of_expr(env, &a.value) {
                    match &param_dim {
                        None => param_dim = Some(p),
                        Some(prev) if prev == &p => {}
                        Some(_) => multi_or_unknown = true,
                    }
                } else {
                    multi_or_unknown = true;
                }
            }
        }

        if all_const {
            let extra_n5 = match elem_n5_maps {
                Some(m) => max_const_dim.saturating_mul(m),
                None => {
                    let site = self.emit_site(
                        in_mgd,
                        span,
                        AllocKind::MakeRuntime,
                        "`make` with unbounded N5 element map init".to_string(),
                        AllocSummary::Unbounded,
                    );
                    return acc.seq(&site);
                }
            };
            // Only `make(map[K]V, hint)` with hint > 8 allocates both hmap + bucket array (<= 2);
            // `make([]T, len, cap)` and `make(chan T, cap)` allocate 1 backing buffer.
            let is_map_make = args
                .first()
                .and_then(|first| self.extract_type_arg_expr(&first.value))
                .is_some_and(|ty| matches!(self.ck.resolve_type_alias(&ty), TypeExpr::Map { .. }));
            let base = if is_map_make && max_const_dim > 8 {
                2
            } else {
                1
            };
            let site = self.emit_site(
                in_mgd,
                span,
                AllocKind::MakeConst,
                format!("constant-size `make` (dim <= {max_const_dim})"),
                AllocSummary::Const(base + extra_n5),
            );
            return acc.seq(&site);
        }

        if !multi_or_unknown {
            if let (Some(p), Some(m)) = (param_dim, elem_n5_maps) {
                let coeff = 1u64.saturating_add(m);
                let site = self.emit_site(
                    in_mgd,
                    span,
                    AllocKind::MakeRuntime,
                    format!("parameter-bounded `make` linear in `{p}`"),
                    AllocSummary::linear(p, coeff, 1),
                );
                return acc.seq(&site);
            }
        }

        self.note_unbounded(
            in_mgd,
            format!(
                "`make` at {} uses runtime size `{}` not proven constant or parameter-bounded",
                span.start,
                args.get(1).map(|a| a.value.render()).unwrap_or_default()
            ),
        );
        let site = self.emit_site(
            in_mgd,
            span,
            AllocKind::MakeRuntime,
            "runtime-length `make` with unproven bound".to_string(),
            AllocSummary::Unbounded,
        );
        acc.seq(&site)
    }

    fn walk_new(&mut self, env: &mut AllocEnv, args: &[CallArg], span: Span) -> DualSummary {
        let in_mgd = env.in_managed();
        let extra = args
            .first()
            .and_then(|a| self.extract_type_arg_expr(&a.value))
            .map(|ty| n5_zero_map_count(self.ck, &ty, 0))
            .unwrap_or(AllocSummary::Const(0));
        let bound = AllocSummary::Const(1).seq(&extra);
        self.emit_site(
            in_mgd,
            span,
            AllocKind::NewAlloc,
            format!(
                "`{}`",
                args.first()
                    .map(|a| format!("new({})", a.value.render()))
                    .unwrap_or_else(|| "new(...)".into())
            ),
            bound,
        )
    }

    fn walk_append(&mut self, env: &mut AllocEnv, args: &[CallArg], span: Span) -> DualSummary {
        let in_mgd = env.in_managed();
        let mut acc = DualSummary::zero();
        for a in args {
            acc = acc.seq(&self.walk_expr(env, &a.value));
        }

        let slice_elem_any = args
            .first()
            .and_then(|first| {
                if let Expr::Ident { name, .. } = unwrap_parens(&first.value) {
                    env.lookup_type(name)
                } else {
                    None
                }
            })
            .and_then(|ty| match self.ck.resolve_type_alias(ty) {
                TypeExpr::Slice { elem, .. } => Some(self.type_is_any(&elem)),
                _ => None,
            })
            .unwrap_or(false);

        if slice_elem_any {
            for a in args.iter().skip(1) {
                if !a.spread && !self.expr_is_already_any_or_nil(env, &a.value) {
                    let b = self.emit_site(
                        in_mgd,
                        a.span,
                        AllocKind::InterfaceBox,
                        format!("boxing `{}` in `append` to `[]any`", a.value.render()),
                        AllocSummary::Const(1),
                    );
                    acc = acc.seq(&b);
                }
            }
        }

        if let Some(last) = args.last() {
            if last.spread {
                let bound = if self.eval_const_collection_len(env, &last.value).is_some()
                    || self.eval_const_str(env, &last.value).is_some()
                {
                    AllocSummary::Const(1)
                } else if let Some(p) = self.collection_param_of_expr(env, &last.value) {
                    AllocSummary::linear(p, 1, 1)
                } else {
                    self.note_unbounded(
                        in_mgd,
                        format!(
                            "variadic `append(..., {}...)` at {} spreads dynamic slice without parameter bound",
                            last.value.render(),
                            span.start
                        ),
                    );
                    AllocSummary::Unbounded
                };
                let site = self.emit_site(
                    in_mgd,
                    span,
                    AllocKind::SliceAppend,
                    format!("variadic `append(..., {}...)`", last.value.render()),
                    bound,
                );
                return acc.seq(&site);
            }
        }

        let site = self.emit_site(
            in_mgd,
            span,
            AllocKind::SliceAppend,
            format!("`append` of {} element(s)", args.len().saturating_sub(1)),
            AllocSummary::Const(1),
        );
        acc.seq(&site)
    }

    fn eval_call_target(
        &mut self,
        env: &mut AllocEnv,
        raw_callee: &Expr,
        args: &[CallArg],
        span: Span,
        is_spawn: bool,
    ) -> DualSummary {
        let in_mgd = env.in_managed();
        let callee = match unwrap_parens(raw_callee) {
            Expr::Index { base, .. } => unwrap_parens(base),
            other => other,
        };

        // Case A: Bare identifier callee (`foo(x)` or local closure `f(x)`).
        if let Expr::Ident { name, .. } = callee {
            let base = Checker::base_named_type_str(name);
            if !env.is_shadowed(base) && !env.base_shapes.contains_key(base) {
                if let Some(decl) = self.fn_by_key.get(base) {
                    return self.compose_decl_call(env, base, decl, None, args, span);
                }
            }
            if let Some(clos) = env.local_closures.get(name).cloned() {
                return self.substitute_dual(
                    env,
                    &clos.body_summary,
                    &clos.params,
                    args,
                    span,
                    name,
                );
            }
            // Dynamic fn-typed parameter or reassigned closure variable -> fail-closed Unbounded.
            self.note_unbounded(
                in_mgd,
                format!(
                    "indirect call through function value `{name}` at {}",
                    span.start
                ),
            );
            return self.emit_site(
                in_mgd,
                span,
                AllocKind::ForeignUnmodeled,
                format!("dynamic function call `{name}(...)`"),
                AllocSummary::Unbounded,
            );
        }

        // Case B: Qualified / member call (`pkg.fn(...)`, `Type.assoc(...)`, `recv.method(...)`).
        if let Expr::Field { base, name, .. } = callee {
            let base_peeled = unwrap_parens(base);
            let method_head = Checker::base_named_type_str(name);

            if let Expr::Ident { name: root, .. } = base_peeled {
                let root_is_local = env.lookup_shape(root).is_some();
                if !root_is_local {
                    // Associated function `Type.new(...)`.
                    let assoc_key = format!("{root}.{method_head}");
                    if let Some(decl) = self.fn_by_key.get(&assoc_key) {
                        return self.compose_decl_call(env, &assoc_key, decl, None, args, span);
                    }
                    // Package-qualified foreign / stdlib call (`fmt.println`, `strings.split`, `serde_json.parse`, ...).
                    return self.eval_pkg_call(env, root, method_head, args, span);
                }
            }

            // Receiver expression exists: walk receiver expression.
            let _ = is_spawn;
            let recv_sum = self.walk_expr(env, base_peeled);

            // Slice `.pop()` intrinsic -> zero heap allocations.
            let recv_shape = self.ck.shape_of_expr(env.fn_state(), base_peeled);
            if method_head == "pop" && args.is_empty() && matches!(recv_shape, TypeShape::Slice(_))
            {
                return recv_sum;
            }

            // `option[T]` intrinsic methods (`is_some`, `is_none`, `unwrap`, `unwrap_or`).
            if self.expr_is_option(env, base_peeled)
                && matches!(method_head, "is_some" | "is_none" | "unwrap" | "unwrap_or")
            {
                return recv_sum;
            }

            // `set[T]` prelude methods (`insert`, `contains`, `remove`, `len`).
            if matches!(
                recv_shape,
                TypeShape::Struct(Some(ref sname)) if Checker::base_named_type_str(sname) == "set"
            ) {
                return match method_head {
                    "contains" | "remove" | "len" => recv_sum,
                    "insert" => {
                        let ins = self.emit_site(
                            in_mgd,
                            span,
                            AllocKind::MapInsert,
                            format!("`{}.insert(...)` set insertion", base_peeled.render()),
                            AllocSummary::Const(1),
                        );
                        recv_sum.seq(&ins)
                    }
                    _ => recv_sum,
                };
            }

            // User-declared method (`fn (c *counter) bump(...)`).
            if let Some(struct_base) = Self::struct_base_of_shape(&recv_shape) {
                let mkey = format!("{struct_base}.{method_head}");
                if let Some(decl) = self.fn_by_key.get(&mkey) {
                    let call_sum =
                        self.compose_decl_call(env, &mkey, decl, Some(base_peeled), args, span);
                    return recv_sum.seq(&call_sum);
                }
            }

            // Known zero-allocation / bounded foreign receiver methods.
            if let Some(foreign_sum) =
                self.eval_foreign_method(env, &recv_shape, method_head, args, span)
            {
                return recv_sum.seq(&foreign_sum);
            }

            // Struct fn-field call (`h.f(1)`) or unknown foreign method -> fail-closed Unbounded.
            self.note_unbounded(
                in_mgd,
                format!(
                    "unmodeled method or function-field call `{}.{method_head}(...)` at {}",
                    base_peeled.render(),
                    span.start
                ),
            );
            let unk = self.emit_site(
                in_mgd,
                span,
                AllocKind::ForeignUnmodeled,
                format!("`{}.{method_head}(...)`", base_peeled.render()),
                AllocSummary::Unbounded,
            );
            return recv_sum.seq(&unk);
        }

        let prefix = self.walk_expr(env, raw_callee);
        self.note_unbounded(
            in_mgd,
            format!(
                "dynamic call `{}(...)` at {}",
                raw_callee.render(),
                span.start
            ),
        );
        let unk = self.emit_site(
            in_mgd,
            span,
            AllocKind::ForeignUnmodeled,
            format!("dynamic call `{}(...)`", raw_callee.render()),
            AllocSummary::Unbounded,
        );
        prefix.seq(&unk)
    }

    fn compose_decl_call(
        &mut self,
        env: &mut AllocEnv,
        callee_key: &str,
        decl: &FnDecl,
        recv_expr: Option<&Expr>,
        args: &[CallArg],
        span: Span,
    ) -> DualSummary {
        let in_mgd = env.in_managed();
        let mut acc = DualSummary::zero();

        // Check `any` boxing and escaping closure args at the call boundary.
        for (i, a) in args.iter().enumerate() {
            if let Some(p) = decl.params.get(i) {
                if self.type_is_any(&p.ty) && !self.expr_is_already_any_or_nil(env, &a.value) {
                    let b = self.emit_site(
                        in_mgd,
                        a.span,
                        AllocKind::InterfaceBox,
                        format!(
                            "boxing argument `{}` for `any` parameter `{}` of `{callee_key}`",
                            a.value.render(),
                            p.name
                        ),
                        AllocSummary::Const(1),
                    );
                    acc = acc.seq(&b);
                }
            }
            if self.expr_is_allocating_local_closure(env, &a.value) {
                self.note_unbounded(
                    in_mgd,
                    format!(
                        "allocating closure `{}` passed to `{callee_key}` at {}",
                        a.value.render(),
                        a.span.start
                    ),
                );
                let esc = self.emit_site(
                    in_mgd,
                    a.span,
                    AllocKind::ClosureEscape,
                    format!("allocating closure passed to `{callee_key}`"),
                    AllocSummary::Unbounded,
                );
                acc = acc.seq(&esc);
            }
        }

        let Some(callee_sum) = self.summaries.get(callee_key) else {
            return acc;
        };

        let mut formal_names: Vec<String> = Vec::new();
        let mut actual_exprs: Vec<&Expr> = Vec::new();
        if let (Some(r), Some(re)) = (&decl.receiver, recv_expr) {
            formal_names.push(r.name.clone());
            actual_exprs.push(re);
        }
        for (p, a) in decl.params.iter().zip(args.iter()) {
            formal_names.push(p.name.clone());
            actual_exprs.push(&a.value);
        }

        let checked_sub = self.substitute_one(
            env,
            &callee_sum.checked,
            &formal_names,
            &actual_exprs,
            span,
            callee_key,
            in_mgd,
        );
        let total_sub = self.substitute_one(
            env,
            &callee_sum.total,
            &formal_names,
            &actual_exprs,
            span,
            callee_key,
            true,
        );
        let composed = if in_mgd {
            DualSummary {
                checked: AllocSummary::Const(0),
                total: total_sub,
            }
        } else {
            DualSummary {
                checked: checked_sub,
                total: total_sub,
            }
        };
        acc.seq(&composed)
    }

    fn substitute_dual(
        &mut self,
        env: &mut AllocEnv,
        callee_sum: &DualSummary,
        formal_names: &[String],
        args: &[CallArg],
        span: Span,
        callee_label: &str,
    ) -> DualSummary {
        let in_mgd = env.in_managed();
        let actuals: Vec<&Expr> = args.iter().map(|a| &a.value).collect();
        let checked_sub = self.substitute_one(
            env,
            &callee_sum.checked,
            formal_names,
            &actuals,
            span,
            callee_label,
            in_mgd,
        );
        let total_sub = self.substitute_one(
            env,
            &callee_sum.total,
            formal_names,
            &actuals,
            span,
            callee_label,
            true,
        );
        if in_mgd {
            DualSummary {
                checked: AllocSummary::Const(0),
                total: total_sub,
            }
        } else {
            DualSummary {
                checked: checked_sub,
                total: total_sub,
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn substitute_one(
        &mut self,
        env: &AllocEnv,
        summary: &AllocSummary,
        formal_names: &[String],
        actuals: &[&Expr],
        span: Span,
        callee_label: &str,
        suppress_reason: bool,
    ) -> AllocSummary {
        match summary {
            AllocSummary::Const(n) => AllocSummary::Const(*n),
            AllocSummary::Unbounded => {
                if !suppress_reason {
                    self.note_unbounded(
                        false,
                        format!(
                            "call to `{callee_label}` at {} propagates `unbounded` summary",
                            span.start
                        ),
                    );
                }
                AllocSummary::Unbounded
            }
            AllocSummary::Linear { param, coeff, base } => {
                let Some(idx) = formal_names.iter().position(|p| p == param) else {
                    return AllocSummary::Unbounded;
                };
                let Some(arg) = actuals.get(idx).copied() else {
                    return AllocSummary::Unbounded;
                };
                // 1. Constant integer argument (`make_buf(8)`).
                if let Some(iv) = self.eval_const_int(env, arg) {
                    if iv >= 0 {
                        let k = u64::try_from(iv).unwrap_or(u64::MAX);
                        return AllocSummary::Const(coeff.saturating_mul(k).saturating_add(*base));
                    }
                }
                // 2. Constant-length collection/string argument (`tally(["a", "b"], ch)`).
                if let Some(clen) = self.eval_const_collection_len(env, arg) {
                    return AllocSummary::Const(coeff.saturating_mul(clen).saturating_add(*base));
                }
                if let Some(s) = self.eval_const_str(env, arg) {
                    return AllocSummary::Const(
                        coeff.saturating_mul(s.len() as u64).saturating_add(*base),
                    );
                }
                // 3. Clean caller parameter (`caller_n`, `len(caller_xs)`, `caller_xs`, or subslice `caller_xs[a:b]`).
                if let Some(caller_p) = self
                    .bound_param_of_expr(env, arg)
                    .or_else(|| self.collection_param_of_expr(env, arg))
                {
                    return AllocSummary::linear(caller_p, *coeff, *base);
                }
                if !suppress_reason {
                    self.note_unbounded(
                        false,
                        format!(
                            "call to `{callee_label}` at {} passes dynamic argument `{}` for linear parameter `{param}`",
                            span.start,
                            arg.render()
                        ),
                    );
                }
                AllocSummary::Unbounded
            }
        }
    }

    fn eval_pkg_call(
        &mut self,
        env: &mut AllocEnv,
        pkg: &str,
        method: &str,
        args: &[CallArg],
        span: Span,
    ) -> DualSummary {
        let in_mgd = env.in_managed();

        // Check escaping closures passed to foreign package calls (A5 perimeter).
        let mut closure_esc = DualSummary::zero();
        for a in args {
            if self.expr_is_allocating_local_closure(env, &a.value) {
                self.note_unbounded(
                    in_mgd,
                    format!(
                        "allocating closure `{}` escapes to `{pkg}.{method}` at {}",
                        a.value.render(),
                        a.span.start
                    ),
                );
                let esc = self.emit_site(
                    in_mgd,
                    a.span,
                    AllocKind::ClosureEscape,
                    format!("closure passed to foreign call `{pkg}.{method}`"),
                    AllocSummary::Unbounded,
                );
                closure_esc = closure_esc.seq(&esc);
            }
        }

        let call_sum = match (pkg, method) {
            // Pure zero-allocation package calls.
            ("atomic", "load" | "store" | "add" | "swap" | "compare_and_swap")
            | (
                "time",
                "milliseconds" | "seconds" | "minutes" | "hours" | "nanoseconds" | "microseconds"
                | "sleep" | "now" | "since" | "until",
            )
            | (
                "strings",
                "contains" | "has_prefix" | "has_suffix" | "index" | "last_index" | "count"
                | "equal_fold" | "trim_space" | "trim_prefix" | "trim_suffix" | "cut"
                | "cut_prefix" | "cut_suffix",
            )
            | ("errors", "is") => DualSummary::zero(),

            // Channel / SPSC pipe construction.
            ("hy", "pipe") | ("time", "after" | "After") => self.emit_site(
                in_mgd,
                span,
                AllocKind::MakeConst,
                format!("`{pkg}.{method}(...)` channel allocation"),
                AllocSummary::Const(1),
            ),

            // `fmt` printing / formatting.
            (
                "fmt",
                "println" | "print" | "printf" | "eprintln" | "eprintf" | "Println" | "Print"
                | "Printf",
            ) => {
                let cost = 1 + args.len() as u64;
                self.emit_site(
                    in_mgd,
                    span,
                    AllocKind::ForeignAlloc,
                    format!("`{pkg}.{method}` formatting/boxing ({} arg(s))", args.len()),
                    AllocSummary::Const(cost),
                )
            }
            ("fmt", "sprintf" | "sprint" | "sprintln" | "errorf" | "Sprintf" | "Errorf") => {
                let cost = 1 + args.len() as u64;
                self.emit_site(
                    in_mgd,
                    span,
                    AllocKind::StringInterp,
                    format!("`{pkg}.{method}` string allocation"),
                    AllocSummary::Const(cost),
                )
            }

            // `errors.new` / `errors.wrap` + `strconv` scalar conversions.
            ("errors", "new" | "wrap" | "New" | "Wrap")
            | (
                "strconv",
                "itoa" | "atoi" | "format_int" | "format_uint" | "format_float" | "format_bool"
                | "parse_int" | "parse_uint" | "parse_float" | "parse_bool" | "Itoa" | "Atoi",
            ) => self.emit_site(
                in_mgd,
                span,
                AllocKind::ForeignAlloc,
                format!("`{pkg}.{method}(...)`"),
                AllocSummary::Const(1),
            ),

            // `strings.split` / `strings.fields`: constant on constant string, linear on param string.
            ("strings", "split" | "fields" | "Split" | "Fields") => {
                let bound = if let Some(first) = args.first() {
                    if self.eval_const_str(env, &first.value).is_some() {
                        AllocSummary::Const(1)
                    } else if let Some(p) = self.collection_param_of_expr(env, &first.value) {
                        AllocSummary::linear(p, 1, 1)
                    } else {
                        self.note_unbounded(
                            in_mgd,
                            format!(
                                "`{pkg}.{method}` on dynamic string `{}` at {}",
                                first.value.render(),
                                span.start
                            ),
                        );
                        AllocSummary::Unbounded
                    }
                } else {
                    AllocSummary::Unbounded
                };
                self.emit_site(
                    in_mgd,
                    span,
                    AllocKind::ForeignAlloc,
                    format!("`{pkg}.{method}(...)`"),
                    bound,
                )
            }

            ("strings", "join" | "repeat" | "replace_all" | "to_lower" | "to_upper") => {
                let bound = if let Some(first) = args.first() {
                    if (self.eval_const_str(env, &first.value).is_some()
                        || self.eval_const_collection_len(env, &first.value).is_some())
                        && args.get(1).is_none_or(|a2| {
                            self.eval_const_int(env, &a2.value).is_some()
                                || self.eval_const_str(env, &a2.value).is_some()
                        })
                    {
                        AllocSummary::Const(1)
                    } else if let Some(p) = self.collection_param_of_expr(env, &first.value) {
                        AllocSummary::linear(p, 1, 1)
                    } else {
                        self.note_unbounded(
                            in_mgd,
                            format!("`{pkg}.{method}` on dynamic input at {}", span.start),
                        );
                        AllocSummary::Unbounded
                    }
                } else {
                    AllocSummary::Unbounded
                };
                self.emit_site(
                    in_mgd,
                    span,
                    AllocKind::ForeignAlloc,
                    format!("`{pkg}.{method}(...)`"),
                    bound,
                )
            }

            // `sort.ints` / `sort.strings` / `sort.float64s`: in-place slice sort (1 small interface header).
            ("sort", "ints" | "strings" | "float64s" | "Ints" | "Strings" | "Float64s") => self
                .emit_site(
                    in_mgd,
                    span,
                    AllocKind::ForeignAlloc,
                    format!("`{pkg}.{method}(...)` slice header box"),
                    AllocSummary::Const(1),
                ),

            // Rust Layer-3 boundary handles (`serde_json`, `flate2`).
            ("serde_json", "new_object" | "new_array") | ("flate2", "new_writer") => self
                .emit_site(
                    in_mgd,
                    span,
                    AllocKind::ForeignAlloc,
                    format!("`{pkg}.{method}()` boundary handle"),
                    AllocSummary::Const(1),
                ),
            ("serde_json", "parse") => {
                let bound = if let Some(first) = args.first() {
                    if self.eval_const_str(env, &first.value).is_some() {
                        AllocSummary::Const(1)
                    } else if let Some(p) = self.collection_param_of_expr(env, &first.value) {
                        AllocSummary::linear(p, 1, 1)
                    } else {
                        self.note_unbounded(
                            in_mgd,
                            format!("`serde_json.parse` on dynamic input at {}", span.start),
                        );
                        AllocSummary::Unbounded
                    }
                } else {
                    AllocSummary::Unbounded
                };
                self.emit_site(
                    in_mgd,
                    span,
                    AllocKind::ForeignAlloc,
                    "`serde_json.parse(...)`".to_string(),
                    bound,
                )
            }
            ("serde_json", "to_string_pretty") => self.emit_site(
                in_mgd,
                span,
                AllocKind::ForeignAlloc,
                "`serde_json.to_string_pretty(...)` Go string copy".to_string(),
                AllocSummary::Const(1),
            ),

            // Everything else (`http.*`, `os.*`, `io.read_all`, `net.*`, unmodeled foreign packages):
            // fail-closed Unbounded.
            _ => {
                self.note_unbounded(
                    in_mgd,
                    format!(
                        "foreign call `{pkg}.{method}(...)` at {} has no static heap bound",
                        span.start
                    ),
                );
                self.emit_site(
                    in_mgd,
                    span,
                    AllocKind::ForeignUnmodeled,
                    format!("unmodeled foreign call `{pkg}.{method}(...)`"),
                    AllocSummary::Unbounded,
                )
            }
        };

        closure_esc.seq(&call_sum)
    }

    fn eval_foreign_method(
        &mut self,
        env: &mut AllocEnv,
        recv_shape: &TypeShape,
        method: &str,
        _args: &[CallArg],
        span: Span,
    ) -> Option<DualSummary> {
        let in_mgd = env.in_managed();
        let inner = match recv_shape {
            TypeShape::Pointer(inner) => inner.as_ref(),
            other => other,
        };
        match inner {
            TypeShape::Atomic => Some(DualSummary::zero()),
            TypeShape::ForeignOpaque(t) if t.starts_with("sync.") => Some(DualSummary::zero()),
            TypeShape::ForeignOpaque(t) if t == "error" && method == "error" => {
                Some(DualSummary::zero())
            }
            TypeShape::OwnHandle(h) if h == "hy.send_cap" || h == "hy.recv_cap" => {
                Some(DualSummary::zero())
            }
            TypeShape::OwnHandle(h) | TypeShape::BorrowHandle(h)
                if h.starts_with("serde_json.") =>
            {
                match method {
                    "int" | "bool" | "float" | "len" | "get" | "set" | "remove" | "push_string"
                    | "array_push_string" => Some(DualSummary::zero()),
                    "array" | "string" | "to_string_pretty" => Some(self.emit_site(
                        in_mgd,
                        span,
                        AllocKind::ForeignAlloc,
                        format!("`{h}.{method}(...)` boundary copy"),
                        AllocSummary::Const(1),
                    )),
                    _ => None,
                }
            }
            TypeShape::OwnHandle(h) if h == "flate2.writer" => match method {
                "write" => Some(DualSummary::zero()),
                "finish" => Some(self.emit_site(
                    in_mgd,
                    span,
                    AllocKind::ForeignAlloc,
                    "`flate2.writer.finish()` output slice".to_string(),
                    AllocSummary::Const(1),
                )),
                _ => None,
            },
            _ => None,
        }
    }

    // -----------------------------------------------------------------------
    // Shape, bound, and constant-evaluation helpers
    // -----------------------------------------------------------------------

    fn is_unshadowed_builtin(&self, env: &AllocEnv, name: &str) -> bool {
        !self.fn_by_key.contains_key(name)
            && env.lookup_shape(name).is_none()
            && !self.ck.consts.contains_key(name)
    }

    fn is_primitive_or_alias_conversion(&self, name: &str) -> bool {
        if matches!(
            name,
            "int"
                | "i8"
                | "int8"
                | "i16"
                | "int16"
                | "i32"
                | "int32"
                | "i64"
                | "int64"
                | "u8"
                | "uint8"
                | "u16"
                | "uint16"
                | "u32"
                | "uint32"
                | "u64"
                | "uint64"
                | "usize"
                | "isize"
                | "uintptr"
                | "float32"
                | "float64"
                | "f32"
                | "f64"
                | "bool"
                | "byte"
                | "rune"
                | "char"
        ) {
            return true;
        }
        if let Some(target) = self.ck.type_aliases.get(name) {
            return matches!(self.ck.shape_of_type(target), TypeShape::Primitive);
        }
        false
    }

    fn type_is_any(&self, ty: &TypeExpr) -> bool {
        matches!(
            self.ck.resolve_type_alias(ty),
            TypeExpr::Named { ref name, .. } if name == "any" || name == "interface{}"
        )
    }

    fn expr_is_already_any_or_nil(&self, env: &AllocEnv, expr: &Expr) -> bool {
        let e = unwrap_parens(expr);
        if matches!(
            e,
            Expr::Lit {
                kind: LitKind::Nil,
                ..
            }
        ) {
            return true;
        }
        if let Expr::Ident { name, .. } = e {
            if let Some(ty) = env.lookup_type(name) {
                return self.type_is_any(ty);
            }
        }
        false
    }

    fn place_is_any(&self, env: &AllocEnv, place: &Place) -> bool {
        match &place.kind {
            PlaceKind::Ident(n) => env.lookup_type(n).is_some_and(|t| self.type_is_any(t)),
            PlaceKind::Field { base, name } => {
                let root_shape = self.shape_of_place(env, base);
                if let Some(sname) = Self::struct_base_of_shape(&root_shape) {
                    if let Some(fields) = self.ck.structs.get(&sname) {
                        if let Some((_, fty)) = fields.iter().find(|(f, _)| f == name) {
                            return self.type_is_any(fty);
                        }
                    }
                }
                false
            }
            PlaceKind::Index { base, .. } => {
                if let PlaceKind::Ident(n) = &base.kind {
                    if let Some(ty) = env.lookup_type(n) {
                        return match self.ck.resolve_type_alias(ty) {
                            TypeExpr::Slice { elem, .. } | TypeExpr::Array { elem, .. } => {
                                self.type_is_any(&elem)
                            }
                            TypeExpr::Map { value, .. } => self.type_is_any(&value),
                            _ => false,
                        };
                    }
                }
                false
            }
            PlaceKind::AddrOf { .. } => false,
        }
    }

    fn chan_elem_is_any(&self, env: &AllocEnv, chan: &Expr) -> bool {
        if let Expr::Ident { name, .. } = unwrap_parens(chan) {
            if let Some(ty) = env.lookup_type(name) {
                if let TypeExpr::Chan { elem, .. } = self.ck.resolve_type_alias(ty) {
                    return self.type_is_any(&elem);
                }
            }
        }
        false
    }

    fn place_is_map(&self, env: &AllocEnv, place: &Place) -> bool {
        matches!(self.shape_of_place(env, place), TypeShape::Map(_, _))
    }

    fn place_is_string(&self, env: &AllocEnv, place: &Place) -> bool {
        matches!(self.shape_of_place(env, place), TypeShape::Str)
    }

    fn shape_of_place(&self, env: &AllocEnv, place: &Place) -> TypeShape {
        match &place.kind {
            PlaceKind::Ident(n) => env
                .lookup_shape(n)
                .cloned()
                .or_else(|| self.ck.consts.get(n).map(|b| b.shape.clone()))
                .unwrap_or(TypeShape::Unknown),
            PlaceKind::Field { base, name } => {
                let bs = self.shape_of_place(env, base);
                let inner = match bs {
                    TypeShape::Pointer(p) => *p,
                    other => other,
                };
                self.ck.field_shape(&inner, name)
            }
            PlaceKind::Index { base, .. } => match self.shape_of_place(env, base) {
                TypeShape::Map(_, v) => *v,
                TypeShape::Slice(e) | TypeShape::Array(e) => *e,
                TypeShape::Pointer(inner) => match *inner {
                    TypeShape::Array(e) => *e,
                    _ => TypeShape::Unknown,
                },
                TypeShape::Str => TypeShape::Primitive,
                _ => TypeShape::Unknown,
            },
            PlaceKind::AddrOf { base } => {
                TypeShape::Pointer(Box::new(self.shape_of_place(env, base)))
            }
        }
    }

    fn expr_is_option(&self, env: &AllocEnv, expr: &Expr) -> bool {
        let e = unwrap_parens(expr);
        if let Expr::Ident { name, .. } = e {
            if env.option_locals.contains(name) {
                return true;
            }
            if let Some(ty) = env.lookup_type(name) {
                return matches!(self.ck.resolve_type_alias(ty), TypeExpr::Option { .. });
            }
        }
        if let Expr::Call { callee, .. } = e {
            if let Expr::Ident { name, .. } = unwrap_parens(callee) {
                if name == "some" && self.is_unshadowed_builtin(env, "some") {
                    return true;
                }
            }
        }
        false
    }

    fn is_map_slot_loan_init<'e>(&self, env: &AllocEnv, expr: &'e Expr) -> Option<&'e Expr> {
        let inner = match unwrap_parens(expr) {
            Expr::AddrOf { operand, .. }
            | Expr::Unary {
                op: UnOp::Addr,
                operand,
                ..
            } => unwrap_parens(operand),
            _ => return None,
        };
        if let Expr::Index { base, index, .. } = inner {
            if matches!(
                self.ck.shape_of_expr(env.fn_state(), base),
                TypeShape::Map(_, _)
            ) {
                return Some(index);
            }
        }
        None
    }

    fn expr_is_allocating_local_closure(&self, env: &AllocEnv, expr: &Expr) -> bool {
        if let Expr::Ident { name, .. } = unwrap_parens(expr) {
            if let Some(info) = env.local_closures.get(name) {
                return !info.body_summary.checked.is_zero();
            }
        }
        false
    }

    fn struct_base_of_shape(shape: &TypeShape) -> Option<String> {
        match shape {
            TypeShape::Struct(Some(s)) => Some(Checker::base_named_type_str(s).to_string()),
            TypeShape::Pointer(inner) => Self::struct_base_of_shape(inner),
            _ => None,
        }
    }

    fn struct_field_types(&self, ty: &TypeExpr) -> Option<Vec<(String, TypeExpr)>> {
        let resolved = self.ck.resolve_type_alias(ty);
        if let TypeExpr::Named { name, .. } = resolved {
            return self.ck.struct_fields_for_sname(&name);
        }
        None
    }

    fn is_set_type_expr(ty: &TypeExpr) -> bool {
        matches!(
            ty,
            TypeExpr::Named { name, .. } if Checker::base_named_type_str(name) == "set"
        )
    }

    fn make_slice_elem_type(&self, first_arg: &Expr) -> Option<TypeExpr> {
        let val = unwrap_parens(first_arg);
        match val {
            Expr::SliceLit {
                elem: Some(t),
                items,
                ..
            } if items.is_empty() && val.as_array_lit().is_none() => Some((**t).clone()),
            Expr::Ident { name, .. } => {
                self.ck
                    .type_aliases
                    .get(name)
                    .and_then(|ty| match self.ck.resolve_type_alias(ty) {
                        TypeExpr::Slice { elem, .. } => Some(*elem),
                        _ => None,
                    })
            }
            _ => None,
        }
    }

    fn extract_type_arg_expr(&self, first_arg: &Expr) -> Option<TypeExpr> {
        let val = unwrap_parens(first_arg);
        match val {
            Expr::StructLit {
                ty: Some(t),
                fields,
                ..
            } if fields.is_empty() => Some(t.clone()),
            Expr::SliceLit {
                elem: Some(t),
                items,
                span,
            } if items.is_empty() => {
                if val.as_array_lit().is_some() {
                    Some((**t).clone())
                } else {
                    Some(TypeExpr::Slice {
                        elem: t.clone(),
                        span: *span,
                    })
                }
            }
            Expr::MapLit {
                key,
                value,
                entries,
                span,
            } if entries.is_empty() => Some(TypeExpr::Map {
                key: key.clone(),
                value: value.clone(),
                span: *span,
            }),
            Expr::Ident { name, span } => Some(TypeExpr::Named {
                name: name.clone(),
                span: *span,
            }),
            _ => None,
        }
    }

    fn multi_return_shapes(&self, st: &FnState, expr: &Expr, want: usize) -> Vec<TypeShape> {
        let e = match unwrap_parens(expr) {
            Expr::Try { operand, .. } => unwrap_parens(operand),
            other => other,
        };
        if let Expr::Call { callee, .. } = e {
            if let Expr::Ident { name, .. } = unwrap_parens(callee) {
                let base = Checker::base_named_type_str(name);
                if let Some(decl) = self.fn_by_key.get(base) {
                    return decl
                        .results
                        .iter()
                        .take(want)
                        .map(|r| self.ck.shape_of_type(&r.ty))
                        .collect();
                }
            }
        }
        let single = self.ck.shape_of_expr(st, expr);
        vec![single; want]
    }

    fn eval_const_int(&self, env: &AllocEnv, expr: &Expr) -> Option<i128> {
        self.eval_const_int_depth(env, expr, 0)
    }

    fn eval_const_int_depth(&self, env: &AllocEnv, expr: &Expr, depth: usize) -> Option<i128> {
        if depth > 16 {
            return None;
        }
        let e = unwrap_parens(expr);
        if let Expr::Ident { name, .. } = e {
            if let Some(&v) = env.const_ints.get(name) {
                return Some(v);
            }
        }
        if let Expr::Call { callee, args, .. } = e {
            if let Expr::Ident { name, .. } = unwrap_parens(callee) {
                if name == "len" && args.len() == 1 && self.is_unshadowed_builtin(env, "len") {
                    if let Some(clen) =
                        self.eval_const_collection_len_depth(env, &args[0].value, depth + 1)
                    {
                        return Some(i128::from(clen));
                    }
                    if let Some(s) = self.eval_const_str_depth(env, &args[0].value, depth + 1) {
                        return Some(s.len() as i128);
                    }
                }
            }
        }
        match self.ck.const_int_fold_expr(e, Some(env.fn_state())) {
            ConstFold::Val(v) => Some(v),
            _ => None,
        }
    }

    fn eval_const_str(&self, env: &AllocEnv, expr: &Expr) -> Option<String> {
        self.eval_const_str_depth(env, expr, 0)
    }

    fn eval_const_str_depth(&self, env: &AllocEnv, expr: &Expr, depth: usize) -> Option<String> {
        if depth > 16 {
            return None;
        }
        let e = unwrap_parens(expr);
        match e {
            Expr::Lit {
                kind: LitKind::Str(s) | LitKind::RawStr(s),
                ..
            } => Some(s.clone()),
            Expr::Interp { parts, .. } if parts.iter().all(|p| matches!(p, InterpPart::Lit(_))) => {
                let mut out = String::new();
                for p in parts {
                    if let InterpPart::Lit(s) = p {
                        out.push_str(s);
                    }
                }
                Some(out)
            }
            Expr::Ident { name, .. } => {
                if let Some(s) = env.const_strs.get(name) {
                    return Some(s.clone());
                }
                if !env.is_shadowed(name) && env.lookup_shape(name).is_none() {
                    if let Some(cval) = self.ck.const_values.get(name) {
                        return self.eval_const_str_depth(env, cval, depth + 1);
                    }
                }
                None
            }
            Expr::Binary {
                op: BinOp::Add,
                lhs,
                rhs,
                ..
            } => {
                let l = self.eval_const_str_depth(env, lhs, depth + 1)?;
                let r = self.eval_const_str_depth(env, rhs, depth + 1)?;
                Some(format!("{l}{r}"))
            }
            _ => None,
        }
    }

    fn const_array_len(&self, env: &AllocEnv, ty: &TypeExpr) -> Option<u64> {
        self.const_array_len_depth(env, ty, 0)
    }

    fn const_array_len_depth(&self, env: &AllocEnv, ty: &TypeExpr, depth: usize) -> Option<u64> {
        if depth > 16 {
            return None;
        }
        let resolved = self.ck.resolve_type_alias(ty);
        if let TypeExpr::Array { len: Some(le), .. } = resolved {
            if let Some(v) = self.eval_const_int_depth(env, &le, depth + 1) {
                if v >= 0 {
                    return u64::try_from(v).ok();
                }
            }
        }
        None
    }

    fn eval_const_collection_len(&self, env: &AllocEnv, expr: &Expr) -> Option<u64> {
        self.eval_const_collection_len_depth(env, expr, 0)
    }

    fn eval_const_collection_len_depth(
        &self,
        env: &AllocEnv,
        expr: &Expr,
        depth: usize,
    ) -> Option<u64> {
        if depth > 16 {
            return None;
        }
        let e = unwrap_parens(expr);
        match e {
            Expr::SliceLit { items, .. } => {
                if let Some((Some(len_expr), _, _)) = e.as_array_lit() {
                    return self
                        .eval_const_int_depth(env, len_expr, depth + 1)
                        .and_then(|v| (v >= 0).then(|| u64::try_from(v).ok()).flatten());
                }
                Some(items.len() as u64)
            }
            Expr::MapLit { entries, .. } => Some(entries.len() as u64),
            Expr::Ident { name, .. } => {
                if let Some(&n) = env.const_lens.get(name) {
                    return Some(n);
                }
                if let Some(ty) = env.lookup_type(name) {
                    if let Some(n) = self.const_array_len_depth(env, ty, depth + 1) {
                        return Some(n);
                    }
                }
                if !env.is_shadowed(name) && env.lookup_shape(name).is_none() {
                    if let Some(cval) = self.ck.const_values.get(name) {
                        return self.eval_const_collection_len_depth(env, cval, depth + 1);
                    }
                }
                None
            }
            Expr::Slice { base, lo, hi, .. } => {
                let base_len = self
                    .eval_const_collection_len_depth(env, base, depth + 1)
                    .or_else(|| {
                        self.eval_const_str_depth(env, base, depth + 1)
                            .map(|s| s.len() as u64)
                    })?;
                let l = match lo {
                    Some(le) => {
                        u64::try_from(self.eval_const_int_depth(env, le, depth + 1)?.max(0)).ok()?
                    }
                    None => 0,
                };
                let h = match hi {
                    Some(he) => {
                        u64::try_from(self.eval_const_int_depth(env, he, depth + 1)?.max(0)).ok()?
                    }
                    None => base_len,
                };
                Some(h.min(base_len).saturating_sub(l))
            }
            Expr::Call { callee, args, .. } => {
                let callee_p = unwrap_parens(callee);
                if let Expr::Ident { name, .. } = callee_p {
                    if name == "make" && args.len() >= 2 && self.is_unshadowed_builtin(env, "make")
                    {
                        if let Some(v) = self.eval_const_int_depth(env, &args[1].value, depth + 1) {
                            if v >= 0 {
                                return u64::try_from(v).ok();
                            }
                        }
                    }
                }
                if let Expr::Field { base, name, .. } = callee_p {
                    if let Expr::Ident { name: pkg, .. } = unwrap_parens(base) {
                        if pkg == "strings" && env.lookup_shape(pkg).is_none() && !args.is_empty() {
                            if (name == "split" || name == "Split") && args.len() == 2 {
                                let s =
                                    self.eval_const_str_depth(env, &args[0].value, depth + 1)?;
                                let sep =
                                    self.eval_const_str_depth(env, &args[1].value, depth + 1)?;
                                let cnt = if sep.is_empty() {
                                    s.chars().count() as u64
                                } else {
                                    s.split(&sep).count() as u64
                                };
                                return Some(cnt);
                            }
                            if (name == "fields" || name == "Fields") && args.len() == 1 {
                                let s =
                                    self.eval_const_str_depth(env, &args[0].value, depth + 1)?;
                                return Some(s.split_whitespace().count() as u64);
                            }
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

    fn bound_const_len_of_expr(&self, env: &AllocEnv, expr: &Expr) -> Option<u64> {
        let e = unwrap_parens(expr);
        if let Expr::Call { callee, args, .. } = e {
            if let Expr::Ident { name, .. } = unwrap_parens(callee) {
                if name == "len" && args.len() == 1 && self.is_unshadowed_builtin(env, "len") {
                    return self
                        .eval_const_collection_len(env, &args[0].value)
                        .or_else(|| {
                            self.eval_const_str(env, &args[0].value)
                                .map(|s| s.len() as u64)
                        });
                }
            }
        }
        None
    }

    /// Recognize an integer bound rooted in an unmodified parameter (`n` or `len(xs)`).
    fn bound_param_of_expr(&self, env: &AllocEnv, expr: &Expr) -> Option<String> {
        let e = unwrap_parens(expr);
        match e {
            Expr::Ident { name, .. } if env.is_clean_param(name) => {
                if matches!(env.lookup_shape(name), Some(TypeShape::Primitive)) {
                    Some(name.clone())
                } else {
                    None
                }
            }
            Expr::Call { callee, args, .. } => {
                if let Expr::Ident { name, .. } = unwrap_parens(callee) {
                    if name == "len" && args.len() == 1 && self.is_unshadowed_builtin(env, "len") {
                        return self.collection_param_of_expr(env, &args[0].value);
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Recognize a collection/string expression whose length is bounded by `|param|`
    /// (`xs` or a subslice `xs[a:b]` where `xs` is an unmodified slice/map/string/array param).
    fn collection_param_of_expr(&self, env: &AllocEnv, expr: &Expr) -> Option<String> {
        let e = unwrap_parens(expr);
        match e {
            Expr::Ident { name, .. } if env.is_clean_param(name) => match env.lookup_shape(name) {
                Some(
                    TypeShape::Slice(_)
                    | TypeShape::Map(_, _)
                    | TypeShape::Str
                    | TypeShape::Array(_),
                ) => Some(name.clone()),
                Some(TypeShape::Struct(Some(sname)))
                    if Checker::base_named_type_str(sname) == "set" =>
                {
                    Some(name.clone())
                }
                _ => None,
            },
            Expr::Slice { base, .. } => self.collection_param_of_expr(env, base),
            Expr::Move { operand, .. } => self.collection_param_of_expr(env, operand),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// N5 zero-value map counter, mutation pre-pass, closure capture scan, Tarjan SCC
// ---------------------------------------------------------------------------

fn n5_zero_map_count(ck: &Checker<'_>, ty: &TypeExpr, depth: usize) -> AllocSummary {
    if depth > 12 {
        return AllocSummary::Unbounded;
    }
    let resolved = ck.resolve_type_alias(ty);
    match resolved {
        TypeExpr::Map { .. } => AllocSummary::Const(1),
        TypeExpr::Named { ref name, .. } if Checker::base_named_type_str(name) == "set" => {
            AllocSummary::Const(1)
        }
        TypeExpr::Named { ref name, .. } => {
            let Some(fields) = ck.struct_fields_for_sname(name) else {
                return AllocSummary::Const(0);
            };
            let mut sum = AllocSummary::Const(0);
            for (_, fty) in fields {
                sum = sum.seq(&n5_zero_map_count(ck, &fty, depth + 1));
            }
            sum
        }
        TypeExpr::Array { len, elem, .. } => {
            let elem_maps = n5_zero_map_count(ck, &elem, depth + 1);
            if elem_maps.is_zero() {
                return AllocSummary::Const(0);
            }
            match len {
                Some(le) => match ck.const_int_fold_expr(&le, None) {
                    ConstFold::Val(v) if v >= 0 => {
                        elem_maps.amplify_const(u64::try_from(v).unwrap_or(u64::MAX))
                    }
                    _ => AllocSummary::Unbounded,
                },
                None => AllocSummary::Const(0),
            }
        }
        _ => AllocSummary::Const(0),
    }
}

fn collect_mutations_block(
    base_shapes: &HashMap<String, TypeShape>,
    block: &Block,
    mutated_params: &mut HashSet<String>,
    reassigned_locals: &mut HashSet<String>,
) {
    let mut shadowed: Vec<HashSet<String>> = Vec::new();
    collect_mutations_stmts(
        base_shapes,
        &block.stmts,
        &mut shadowed,
        mutated_params,
        reassigned_locals,
    );
}

fn collect_mutations_stmts(
    base_shapes: &HashMap<String, TypeShape>,
    stmts: &[Stmt],
    shadowed: &mut Vec<HashSet<String>>,
    mutated_params: &mut HashSet<String>,
    reassigned_locals: &mut HashSet<String>,
) {
    let is_shadowed = |name: &str, sh: &[HashSet<String>]| sh.iter().any(|s| s.contains(name));

    for stmt in stmts {
        match stmt {
            Stmt::Local(l) => {
                for v in &l.values {
                    collect_mutations_expr(
                        base_shapes,
                        v,
                        shadowed,
                        mutated_params,
                        reassigned_locals,
                    );
                }
                if let Some(top) = shadowed.last_mut() {
                    for n in &l.names {
                        top.insert(n.clone());
                    }
                }
            }
            Stmt::Assign {
                targets, values, ..
            } => {
                for v in values {
                    collect_mutations_expr(
                        base_shapes,
                        v,
                        shadowed,
                        mutated_params,
                        reassigned_locals,
                    );
                }
                for t in targets {
                    let root = t.root();
                    match &t.kind {
                        PlaceKind::Ident(_) => {
                            reassigned_locals.insert(root.to_string());
                            if !is_shadowed(root, shadowed) && base_shapes.contains_key(root) {
                                mutated_params.insert(root.to_string());
                            }
                        }
                        PlaceKind::Index { .. } => {
                            // Map element insertion changes `len(m)`, whereas slice `s[i] = v` keeps `len(s)`.
                            if !is_shadowed(root, shadowed) {
                                if matches!(base_shapes.get(root), Some(TypeShape::Map(_, _))) {
                                    mutated_params.insert(root.to_string());
                                }
                            } else {
                                reassigned_locals.insert(root.to_string());
                            }
                        }
                        PlaceKind::Field { .. } | PlaceKind::AddrOf { .. } => {}
                    }
                }
            }
            Stmt::Expr { expr, .. } | Stmt::Defer { call: expr, .. } => {
                collect_mutations_expr(
                    base_shapes,
                    expr,
                    shadowed,
                    mutated_params,
                    reassigned_locals,
                );
            }
            Stmt::Send { chan, value, .. } => {
                collect_mutations_expr(
                    base_shapes,
                    chan,
                    shadowed,
                    mutated_params,
                    reassigned_locals,
                );
                collect_mutations_expr(
                    base_shapes,
                    value,
                    shadowed,
                    mutated_params,
                    reassigned_locals,
                );
            }
            Stmt::Return { values, .. } => {
                for v in values {
                    collect_mutations_expr(
                        base_shapes,
                        v,
                        shadowed,
                        mutated_params,
                        reassigned_locals,
                    );
                }
            }
            Stmt::Task { callee, args, .. } => {
                collect_mutations_expr(
                    base_shapes,
                    callee,
                    shadowed,
                    mutated_params,
                    reassigned_locals,
                );
                for a in args {
                    collect_mutations_expr(
                        base_shapes,
                        &a.value,
                        shadowed,
                        mutated_params,
                        reassigned_locals,
                    );
                }
            }
            Stmt::If {
                init,
                cond,
                then,
                otherwise,
                ..
            } => {
                shadowed.push(HashSet::new());
                if let Some(i) = init {
                    collect_mutations_stmts(
                        base_shapes,
                        std::slice::from_ref(i.as_ref()),
                        shadowed,
                        mutated_params,
                        reassigned_locals,
                    );
                }
                collect_mutations_expr(
                    base_shapes,
                    cond,
                    shadowed,
                    mutated_params,
                    reassigned_locals,
                );
                shadowed.push(HashSet::new());
                collect_mutations_stmts(
                    base_shapes,
                    &then.stmts,
                    shadowed,
                    mutated_params,
                    reassigned_locals,
                );
                shadowed.pop();
                if let Some(o) = otherwise {
                    collect_mutations_stmts(
                        base_shapes,
                        std::slice::from_ref(o.as_ref()),
                        shadowed,
                        mutated_params,
                        reassigned_locals,
                    );
                }
                shadowed.pop();
            }
            Stmt::For { kind, body, .. } => {
                let mut binders = HashSet::new();
                match kind {
                    ForKind::Infinite => {}
                    ForKind::While { cond } => {
                        collect_mutations_expr(
                            base_shapes,
                            cond,
                            shadowed,
                            mutated_params,
                            reassigned_locals,
                        );
                    }
                    ForKind::Range { var, lo, hi, .. } => {
                        collect_mutations_expr(
                            base_shapes,
                            lo,
                            shadowed,
                            mutated_params,
                            reassigned_locals,
                        );
                        collect_mutations_expr(
                            base_shapes,
                            hi,
                            shadowed,
                            mutated_params,
                            reassigned_locals,
                        );
                        binders.insert(var.clone());
                    }
                    ForKind::Iterate { vars, expr } => {
                        collect_mutations_expr(
                            base_shapes,
                            expr,
                            shadowed,
                            mutated_params,
                            reassigned_locals,
                        );
                        for v in vars {
                            binders.insert(v.clone());
                        }
                    }
                }
                shadowed.push(binders);
                collect_mutations_stmts(
                    base_shapes,
                    &body.stmts,
                    shadowed,
                    mutated_params,
                    reassigned_locals,
                );
                shadowed.pop();
            }
            Stmt::Managed { body, .. } | Stmt::Block { block: body, .. } => {
                shadowed.push(HashSet::new());
                collect_mutations_stmts(
                    base_shapes,
                    &body.stmts,
                    shadowed,
                    mutated_params,
                    reassigned_locals,
                );
                shadowed.pop();
            }
            Stmt::Match { subject, arms, .. } => {
                collect_mutations_expr(
                    base_shapes,
                    subject,
                    shadowed,
                    mutated_params,
                    reassigned_locals,
                );
                for arm in arms {
                    let mut binders = HashSet::new();
                    if let hy_syntax::MatchPattern::Path {
                        binding: Some(b), ..
                    } = &arm.pattern
                    {
                        binders.insert(b.clone());
                    }
                    shadowed.push(binders);
                    if let Some(g) = &arm.guard {
                        collect_mutations_expr(
                            base_shapes,
                            g,
                            shadowed,
                            mutated_params,
                            reassigned_locals,
                        );
                    }
                    collect_mutations_stmts(
                        base_shapes,
                        &arm.body.stmts,
                        shadowed,
                        mutated_params,
                        reassigned_locals,
                    );
                    shadowed.pop();
                }
            }
            Stmt::Select { arms, .. } => {
                for arm in arms {
                    let mut binders = HashSet::new();
                    match &arm.kind {
                        SelectArmKind::Send { chan, value, .. } => {
                            collect_mutations_expr(
                                base_shapes,
                                chan,
                                shadowed,
                                mutated_params,
                                reassigned_locals,
                            );
                            collect_mutations_expr(
                                base_shapes,
                                value,
                                shadowed,
                                mutated_params,
                                reassigned_locals,
                            );
                        }
                        SelectArmKind::Recv { names, chan } => {
                            collect_mutations_expr(
                                base_shapes,
                                chan,
                                shadowed,
                                mutated_params,
                                reassigned_locals,
                            );
                            for n in names {
                                binders.insert(n.clone());
                            }
                        }
                        SelectArmKind::Default => {}
                    }
                    shadowed.push(binders);
                    collect_mutations_stmts(
                        base_shapes,
                        &arm.body.stmts,
                        shadowed,
                        mutated_params,
                        reassigned_locals,
                    );
                    shadowed.pop();
                }
            }
            Stmt::Break { .. } | Stmt::Continue { .. } => {}
        }
    }
}

fn collect_mutations_expr(
    base_shapes: &HashMap<String, TypeShape>,
    expr: &Expr,
    shadowed: &mut Vec<HashSet<String>>,
    mutated_params: &mut HashSet<String>,
    reassigned_locals: &mut HashSet<String>,
) {
    let is_shadowed = |name: &str, sh: &[HashSet<String>]| sh.iter().any(|s| s.contains(name));
    match expr {
        Expr::Ident { .. } | Expr::Lit { .. } => {}
        Expr::Paren { inner, .. }
        | Expr::Move { operand: inner, .. }
        | Expr::Try { operand: inner, .. }
        | Expr::Unary { operand: inner, .. }
        | Expr::AddrOf { operand: inner, .. }
        | Expr::Receive { chan: inner, .. }
        | Expr::Field { base: inner, .. } => {
            collect_mutations_expr(
                base_shapes,
                inner,
                shadowed,
                mutated_params,
                reassigned_locals,
            );
        }
        Expr::Interp { parts, .. } => {
            for p in parts {
                if let InterpPart::Expr(x) = p {
                    collect_mutations_expr(
                        base_shapes,
                        x,
                        shadowed,
                        mutated_params,
                        reassigned_locals,
                    );
                }
            }
        }
        Expr::Binary { lhs, rhs, .. }
        | Expr::Index {
            base: lhs,
            index: rhs,
            ..
        } => {
            collect_mutations_expr(
                base_shapes,
                lhs,
                shadowed,
                mutated_params,
                reassigned_locals,
            );
            collect_mutations_expr(
                base_shapes,
                rhs,
                shadowed,
                mutated_params,
                reassigned_locals,
            );
        }
        Expr::Slice { base, lo, hi, .. } => {
            collect_mutations_expr(
                base_shapes,
                base,
                shadowed,
                mutated_params,
                reassigned_locals,
            );
            if let Some(l) = lo {
                collect_mutations_expr(base_shapes, l, shadowed, mutated_params, reassigned_locals);
            }
            if let Some(h) = hi {
                collect_mutations_expr(base_shapes, h, shadowed, mutated_params, reassigned_locals);
            }
        }
        Expr::Call { callee, args, .. } => {
            // `s.pop()` mutates `len(s)` in place; `delete(m, k)` / `clear(m)` / `s.insert(x)` mutate containers.
            if let Expr::Field { base, name, .. } = unwrap_parens(callee) {
                if matches!(name.as_str(), "pop" | "insert" | "remove") {
                    if let Expr::Ident { name: root, .. } = unwrap_parens(base) {
                        reassigned_locals.insert(root.clone());
                        if !is_shadowed(root, shadowed) && base_shapes.contains_key(root) {
                            mutated_params.insert(root.clone());
                        }
                    }
                }
            }
            if let Expr::Ident { name, .. } = unwrap_parens(callee) {
                if matches!(name.as_str(), "delete" | "clear") {
                    if let Some(first) = args.first() {
                        if let Expr::Ident { name: root, .. } = unwrap_parens(&first.value) {
                            reassigned_locals.insert(root.clone());
                            if !is_shadowed(root, shadowed) && base_shapes.contains_key(root) {
                                mutated_params.insert(root.clone());
                            }
                        }
                    }
                }
            }
            collect_mutations_expr(
                base_shapes,
                callee,
                shadowed,
                mutated_params,
                reassigned_locals,
            );
            for a in args {
                collect_mutations_expr(
                    base_shapes,
                    &a.value,
                    shadowed,
                    mutated_params,
                    reassigned_locals,
                );
            }
        }
        Expr::StructLit { fields, .. } => {
            for f in fields {
                collect_mutations_expr(
                    base_shapes,
                    &f.value,
                    shadowed,
                    mutated_params,
                    reassigned_locals,
                );
            }
        }
        Expr::SliceLit { items, .. } => {
            for it in items {
                collect_mutations_expr(
                    base_shapes,
                    it,
                    shadowed,
                    mutated_params,
                    reassigned_locals,
                );
            }
        }
        Expr::MapLit { entries, .. } => {
            for (k, v) in entries {
                collect_mutations_expr(base_shapes, k, shadowed, mutated_params, reassigned_locals);
                collect_mutations_expr(base_shapes, v, shadowed, mutated_params, reassigned_locals);
            }
        }
        Expr::FnLit { params, body, .. } => {
            let binders: HashSet<String> = params.iter().map(|p| p.name.clone()).collect();
            shadowed.push(binders);
            collect_mutations_stmts(
                base_shapes,
                &body.stmts,
                shadowed,
                mutated_params,
                reassigned_locals,
            );
            shadowed.pop();
        }
    }
}

fn closure_captures_outer(env: &AllocEnv, param_names: &HashSet<&str>, body: &Block) -> bool {
    let mut locals: HashSet<String> = param_names.iter().map(|s| (*s).to_string()).collect();
    block_mentions_outer(env, &mut locals, body)
}

fn block_mentions_outer(env: &AllocEnv, locals: &mut HashSet<String>, block: &Block) -> bool {
    for s in &block.stmts {
        if stmt_mentions_outer(env, locals, s) {
            return true;
        }
    }
    false
}

fn stmt_mentions_outer(env: &AllocEnv, locals: &mut HashSet<String>, stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Local(l) => {
            for v in &l.values {
                if expr_mentions_outer(env, locals, v) {
                    return true;
                }
            }
            for n in &l.names {
                locals.insert(n.clone());
            }
            false
        }
        Stmt::Assign {
            targets, values, ..
        } => {
            for t in targets {
                let root = t.root();
                if !locals.contains(root) && env.lookup_shape(root).is_some() {
                    return true;
                }
            }
            values.iter().any(|v| expr_mentions_outer(env, locals, v))
        }
        Stmt::Expr { expr, .. } | Stmt::Defer { call: expr, .. } => {
            expr_mentions_outer(env, locals, expr)
        }
        Stmt::Send { chan, value, .. } => {
            expr_mentions_outer(env, locals, chan) || expr_mentions_outer(env, locals, value)
        }
        Stmt::Return { values, .. } => values.iter().any(|v| expr_mentions_outer(env, locals, v)),
        Stmt::Task { callee, args, .. } => {
            expr_mentions_outer(env, locals, callee)
                || args
                    .iter()
                    .any(|a| expr_mentions_outer(env, locals, &a.value))
        }
        Stmt::If {
            init,
            cond,
            then,
            otherwise,
            ..
        } => {
            let mut inner = locals.clone();
            if let Some(i) = init {
                if stmt_mentions_outer(env, &mut inner, i) {
                    return true;
                }
            }
            if expr_mentions_outer(env, &mut inner, cond) {
                return true;
            }
            let mut then_locals = inner.clone();
            if block_mentions_outer(env, &mut then_locals, then) {
                return true;
            }
            if let Some(o) = otherwise {
                if stmt_mentions_outer(env, &mut inner, o) {
                    return true;
                }
            }
            false
        }
        Stmt::For { kind, body, .. } => {
            let mut inner = locals.clone();
            match kind {
                ForKind::Infinite => {}
                ForKind::While { cond } => {
                    if expr_mentions_outer(env, &mut inner, cond) {
                        return true;
                    }
                }
                ForKind::Range { var, lo, hi, .. } => {
                    if expr_mentions_outer(env, &mut inner, lo)
                        || expr_mentions_outer(env, &mut inner, hi)
                    {
                        return true;
                    }
                    inner.insert(var.clone());
                }
                ForKind::Iterate { vars, expr } => {
                    if expr_mentions_outer(env, &mut inner, expr) {
                        return true;
                    }
                    inner.extend(vars.iter().cloned());
                }
            }
            block_mentions_outer(env, &mut inner, body)
        }
        Stmt::Managed { body, .. } | Stmt::Block { block: body, .. } => {
            let mut inner = locals.clone();
            block_mentions_outer(env, &mut inner, body)
        }
        Stmt::Match { subject, arms, .. } => {
            if expr_mentions_outer(env, locals, subject) {
                return true;
            }
            for arm in arms {
                let mut inner = locals.clone();
                if let hy_syntax::MatchPattern::Path {
                    binding: Some(b), ..
                } = &arm.pattern
                {
                    inner.insert(b.clone());
                }
                if arm
                    .guard
                    .as_ref()
                    .is_some_and(|g| expr_mentions_outer(env, &mut inner, g))
                {
                    return true;
                }
                if block_mentions_outer(env, &mut inner, &arm.body) {
                    return true;
                }
            }
            false
        }
        Stmt::Select { arms, .. } => {
            for arm in arms {
                let mut inner = locals.clone();
                match &arm.kind {
                    SelectArmKind::Send { chan, value, .. } => {
                        if expr_mentions_outer(env, &mut inner, chan)
                            || expr_mentions_outer(env, &mut inner, value)
                        {
                            return true;
                        }
                    }
                    SelectArmKind::Recv { names, chan } => {
                        if expr_mentions_outer(env, &mut inner, chan) {
                            return true;
                        }
                        inner.extend(names.iter().cloned());
                    }
                    SelectArmKind::Default => {}
                }
                if block_mentions_outer(env, &mut inner, &arm.body) {
                    return true;
                }
            }
            false
        }
        Stmt::Break { .. } | Stmt::Continue { .. } => false,
    }
}

fn expr_mentions_outer(env: &AllocEnv, locals: &mut HashSet<String>, expr: &Expr) -> bool {
    match expr {
        Expr::Ident { name, .. } => !locals.contains(name) && env.lookup_shape(name).is_some(),
        Expr::Lit { .. } => false,
        Expr::Paren { inner, .. }
        | Expr::Move { operand: inner, .. }
        | Expr::Try { operand: inner, .. }
        | Expr::Unary { operand: inner, .. }
        | Expr::AddrOf { operand: inner, .. }
        | Expr::Receive { chan: inner, .. }
        | Expr::Field { base: inner, .. } => expr_mentions_outer(env, locals, inner),
        Expr::Interp { parts, .. } => parts.iter().any(|p| match p {
            InterpPart::Expr(x) => expr_mentions_outer(env, locals, x),
            InterpPart::Lit(_) => false,
        }),
        Expr::Binary { lhs, rhs, .. }
        | Expr::Index {
            base: lhs,
            index: rhs,
            ..
        } => expr_mentions_outer(env, locals, lhs) || expr_mentions_outer(env, locals, rhs),
        Expr::Slice { base, lo, hi, .. } => {
            expr_mentions_outer(env, locals, base)
                || lo
                    .as_ref()
                    .is_some_and(|l| expr_mentions_outer(env, locals, l))
                || hi
                    .as_ref()
                    .is_some_and(|h| expr_mentions_outer(env, locals, h))
        }
        Expr::Call { callee, args, .. } => {
            expr_mentions_outer(env, locals, callee)
                || args
                    .iter()
                    .any(|a| expr_mentions_outer(env, locals, &a.value))
        }
        Expr::StructLit { fields, .. } => fields
            .iter()
            .any(|f| expr_mentions_outer(env, locals, &f.value)),
        Expr::SliceLit { items, .. } => items.iter().any(|it| expr_mentions_outer(env, locals, it)),
        Expr::MapLit { entries, .. } => entries.iter().any(|(k, v)| {
            expr_mentions_outer(env, locals, k) || expr_mentions_outer(env, locals, v)
        }),
        Expr::FnLit { params, body, .. } => {
            let mut inner = locals.clone();
            for p in params {
                inner.insert(p.name.clone());
            }
            block_mentions_outer(env, &mut inner, body)
        }
    }
}

fn collect_static_callees_block(
    ck: &Checker<'_>,
    fn_by_key: &HashMap<String, &FnDecl>,
    block: &Block,
    out: &mut BTreeMap<String, ()>,
) {
    for stmt in &block.stmts {
        collect_static_callees_stmt(ck, fn_by_key, stmt, out);
    }
}

fn collect_static_callees_stmt(
    ck: &Checker<'_>,
    fn_by_key: &HashMap<String, &FnDecl>,
    stmt: &Stmt,
    out: &mut BTreeMap<String, ()>,
) {
    match stmt {
        Stmt::Local(l) => {
            for v in &l.values {
                collect_static_callees_expr(ck, fn_by_key, v, out);
            }
        }
        Stmt::Assign { values, .. } | Stmt::Return { values, .. } => {
            for v in values {
                collect_static_callees_expr(ck, fn_by_key, v, out);
            }
        }
        Stmt::Expr { expr, .. } | Stmt::Defer { call: expr, .. } => {
            collect_static_callees_expr(ck, fn_by_key, expr, out);
        }
        Stmt::Send { chan, value, .. } => {
            collect_static_callees_expr(ck, fn_by_key, chan, out);
            collect_static_callees_expr(ck, fn_by_key, value, out);
        }
        Stmt::Task { callee, args, .. } => {
            record_callee_expr_edge(ck, fn_by_key, callee, out);
            collect_static_callees_expr(ck, fn_by_key, callee, out);
            for a in args {
                collect_static_callees_expr(ck, fn_by_key, &a.value, out);
            }
        }
        Stmt::If {
            init,
            cond,
            then,
            otherwise,
            ..
        } => {
            if let Some(i) = init {
                collect_static_callees_stmt(ck, fn_by_key, i, out);
            }
            collect_static_callees_expr(ck, fn_by_key, cond, out);
            collect_static_callees_block(ck, fn_by_key, then, out);
            if let Some(o) = otherwise {
                collect_static_callees_stmt(ck, fn_by_key, o, out);
            }
        }
        Stmt::For { kind, body, .. } => {
            match kind {
                ForKind::Infinite => {}
                ForKind::While { cond } => collect_static_callees_expr(ck, fn_by_key, cond, out),
                ForKind::Range { lo, hi, .. } => {
                    collect_static_callees_expr(ck, fn_by_key, lo, out);
                    collect_static_callees_expr(ck, fn_by_key, hi, out);
                }
                ForKind::Iterate { expr, .. } => {
                    collect_static_callees_expr(ck, fn_by_key, expr, out);
                }
            }
            collect_static_callees_block(ck, fn_by_key, body, out);
        }
        Stmt::Managed { body, .. } | Stmt::Block { block: body, .. } => {
            collect_static_callees_block(ck, fn_by_key, body, out);
        }
        Stmt::Match { subject, arms, .. } => {
            collect_static_callees_expr(ck, fn_by_key, subject, out);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    collect_static_callees_expr(ck, fn_by_key, g, out);
                }
                collect_static_callees_block(ck, fn_by_key, &arm.body, out);
            }
        }
        Stmt::Select { arms, .. } => {
            for arm in arms {
                match &arm.kind {
                    SelectArmKind::Send { chan, value, .. } => {
                        collect_static_callees_expr(ck, fn_by_key, chan, out);
                        collect_static_callees_expr(ck, fn_by_key, value, out);
                    }
                    SelectArmKind::Recv { chan, .. } => {
                        collect_static_callees_expr(ck, fn_by_key, chan, out);
                    }
                    SelectArmKind::Default => {}
                }
                collect_static_callees_block(ck, fn_by_key, &arm.body, out);
            }
        }
        Stmt::Break { .. } | Stmt::Continue { .. } => {}
    }
}

fn collect_static_callees_expr(
    ck: &Checker<'_>,
    fn_by_key: &HashMap<String, &FnDecl>,
    expr: &Expr,
    out: &mut BTreeMap<String, ()>,
) {
    match expr {
        Expr::Ident { .. } | Expr::Lit { .. } => {}
        Expr::Paren { inner, .. }
        | Expr::Move { operand: inner, .. }
        | Expr::Try { operand: inner, .. }
        | Expr::Unary { operand: inner, .. }
        | Expr::AddrOf { operand: inner, .. }
        | Expr::Receive { chan: inner, .. }
        | Expr::Field { base: inner, .. } => collect_static_callees_expr(ck, fn_by_key, inner, out),
        Expr::Interp { parts, .. } => {
            for p in parts {
                if let InterpPart::Expr(x) = p {
                    collect_static_callees_expr(ck, fn_by_key, x, out);
                }
            }
        }
        Expr::Binary { lhs, rhs, .. }
        | Expr::Index {
            base: lhs,
            index: rhs,
            ..
        } => {
            collect_static_callees_expr(ck, fn_by_key, lhs, out);
            collect_static_callees_expr(ck, fn_by_key, rhs, out);
        }
        Expr::Slice { base, lo, hi, .. } => {
            collect_static_callees_expr(ck, fn_by_key, base, out);
            if let Some(l) = lo {
                collect_static_callees_expr(ck, fn_by_key, l, out);
            }
            if let Some(h) = hi {
                collect_static_callees_expr(ck, fn_by_key, h, out);
            }
        }
        Expr::Call { callee, args, .. } => {
            record_callee_expr_edge(ck, fn_by_key, callee, out);
            collect_static_callees_expr(ck, fn_by_key, callee, out);
            for a in args {
                collect_static_callees_expr(ck, fn_by_key, &a.value, out);
            }
        }
        Expr::StructLit { fields, .. } => {
            for f in fields {
                collect_static_callees_expr(ck, fn_by_key, &f.value, out);
            }
        }
        Expr::SliceLit { items, .. } => {
            for it in items {
                collect_static_callees_expr(ck, fn_by_key, it, out);
            }
        }
        Expr::MapLit { entries, .. } => {
            for (k, v) in entries {
                collect_static_callees_expr(ck, fn_by_key, k, out);
                collect_static_callees_expr(ck, fn_by_key, v, out);
            }
        }
        Expr::FnLit { body, .. } => collect_static_callees_block(ck, fn_by_key, body, out),
    }
}

fn record_callee_expr_edge(
    ck: &Checker<'_>,
    fn_by_key: &HashMap<String, &FnDecl>,
    raw_callee: &Expr,
    out: &mut BTreeMap<String, ()>,
) {
    let callee = match unwrap_parens(raw_callee) {
        Expr::Index { base, .. } => unwrap_parens(base),
        other => other,
    };
    match callee {
        Expr::Ident { name, .. } => {
            let base = Checker::base_named_type_str(name);
            if fn_by_key.contains_key(base) {
                out.insert(base.to_string(), ());
            }
        }
        Expr::Field { base, name, .. } => {
            let head = Checker::base_named_type_str(name);
            if let Expr::Ident { name: root, .. } = unwrap_parens(base) {
                let assoc = format!("{root}.{head}");
                if fn_by_key.contains_key(&assoc) {
                    out.insert(assoc, ());
                    return;
                }
                if foreign::is_curated_package_root(root) || ck.packages.contains(root) {
                    return;
                }
            }
            // Conservatively link any declared method matching `.head` so mutual
            // recursion through methods is always placed in one SCC.
            for k in fn_by_key.keys() {
                if k.rsplit_once('.').map(|(_, m)| m) == Some(head) {
                    out.insert(k.clone(), ());
                }
            }
        }
        _ => {}
    }
}

/// Tarjan's strongly connected components algorithm.
/// Returns SCCs in reverse topological order (callees before callers).
fn tarjan_scc(nodes: &[String], edges: &HashMap<String, BTreeMap<String, ()>>) -> Vec<Vec<String>> {
    struct State<'a> {
        edges: &'a HashMap<String, BTreeMap<String, ()>>,
        index: usize,
        indices: HashMap<String, usize>,
        lowlink: HashMap<String, usize>,
        stack: Vec<String>,
        on_stack: HashSet<String>,
        sccs: Vec<Vec<String>>,
    }

    fn strongconnect(v: &str, st: &mut State<'_>) {
        let v_idx = st.index;
        st.index += 1;
        st.indices.insert(v.to_string(), v_idx);
        st.lowlink.insert(v.to_string(), v_idx);
        st.stack.push(v.to_string());
        st.on_stack.insert(v.to_string());

        if let Some(nbrs) = st.edges.get(v) {
            for w in nbrs.keys() {
                if !st.indices.contains_key(w) {
                    strongconnect(w, st);
                    let w_low = st.lowlink[w];
                    let v_low = st.lowlink.get_mut(v).unwrap();
                    *v_low = (*v_low).min(w_low);
                } else if st.on_stack.contains(w) {
                    let w_idx = st.indices[w];
                    let v_low = st.lowlink.get_mut(v).unwrap();
                    *v_low = (*v_low).min(w_idx);
                }
            }
        }

        if st.lowlink[v] == st.indices[v] {
            let mut scc = Vec::new();
            while let Some(w) = st.stack.pop() {
                st.on_stack.remove(&w);
                let done = w == v;
                scc.push(w);
                if done {
                    break;
                }
            }
            st.sccs.push(scc);
        }
    }

    let mut st = State {
        edges,
        index: 0,
        indices: HashMap::new(),
        lowlink: HashMap::new(),
        stack: Vec::new(),
        on_stack: HashSet::new(),
        sccs: Vec::new(),
    };

    for n in nodes {
        if !st.indices.contains_key(n) {
            strongconnect(n, &mut st);
        }
    }
    st.sccs
}

fn dedup_preserving_order(items: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for it in items {
        if seen.insert(it.clone()) {
            out.push(it);
        }
    }
    out
}

/// Extract `// @heap-budget: <spec>` or `// hyb:heap-budget: <spec>` pragmas immediately
/// preceding `fn` declarations in `src`.
pub fn parse_source_pragmas(src: &str, file: &File) -> HashMap<String, HeapBudget> {
    let lines: Vec<&str> = src.lines().collect();
    let mut out = HashMap::new();
    for d in &file.decls {
        let Decl::Fn(f) = d else { continue };
        let fn_line = f.span.start.line as usize;
        if fn_line <= 1 || fn_line > lines.len() + 1 {
            continue;
        }
        let mut cursor = fn_line - 1;
        while cursor > 0 {
            let trimmed = lines[cursor - 1].trim();
            if trimmed.is_empty() {
                cursor -= 1;
                continue;
            }
            let Some(comment) = trimmed.strip_prefix("//") else {
                break;
            };
            let body = comment.trim();
            let spec_opt = body
                .strip_prefix("@heap-budget:")
                .or_else(|| body.strip_prefix("@heap_budget:"))
                .or_else(|| body.strip_prefix("hyb:heap-budget:"));
            if let Some(raw_spec) = spec_opt {
                if let Ok(budget) = parse_budget_spec(raw_spec) {
                    out.insert(canonical_fn_key(f), budget);
                }
            }
            cursor -= 1;
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Corpus static summary & measurement helper (#4492 M1/M2 static half)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct CorpusProgramAllocRow {
    pub file: String,
    pub expected_verdict: String,
    pub program_checked_summary: AllocSummary,
    pub program_total_summary: AllocSummary,
    pub total_sites: usize,
    pub checked_sites: usize,
    pub managed_sites: usize,
    pub linear_functions: Vec<(String, AllocSummary)>,
    pub uses_data_dependent_surface: bool,
}

#[derive(Debug, Clone)]
pub struct CorpusAllocAudit {
    pub accept_programs_total: usize,
    pub accept_clean_count: usize,
    pub accept_managed_count: usize,
    pub programs_with_sites: usize,
    pub total_static_sites: usize,
    pub certified_const_zero_programs: usize,
    pub certified_const_programs: usize,
    pub certified_linear_programs: usize,
    pub unbounded_programs: usize,
    pub non_data_dependent_programs: usize,
    pub non_data_dependent_certified_const: usize,
    pub total_functions_analyzed: usize,
    pub const_functions: usize,
    pub linear_functions: usize,
    pub unbounded_functions: usize,
    pub by_kind_totals: BTreeMap<String, usize>,
    pub baseline_check_micros: u128,
    pub alloc_check_micros: u128,
    pub rows: Vec<CorpusProgramAllocRow>,
}

impl CorpusAllocAudit {
    /// Render a Markdown summary report suitable for M1/M2 review artifacts.
    pub fn render_markdown_summary(&self) -> String {
        let mut out = String::new();
        out.push_str("### Corpus Static Allocation Audit\n\n");
        out.push_str(&format!(
            "- Accept-side corpus programs: **{}** (`ACCEPT`: {}, `ACCEPT-MANAGED`: {})\n",
            self.accept_programs_total, self.accept_clean_count, self.accept_managed_count
        ));
        out.push_str(&format!(
            "- Programs emitting >= 1 allocation site: **{} / {}** ({} total static sites)\n",
            self.programs_with_sites, self.accept_programs_total, self.total_static_sites
        ));
        out.push_str(&format!(
            "- Program-level checked summaries (0 annotations): `const(0)` = **{}**, `const(k)` total = **{}**, `linear` = **{}**, `unbounded` = **{}**\n",
            self.certified_const_zero_programs,
            self.certified_const_programs,
            self.certified_linear_programs,
            self.unbounded_programs
        ));
        out.push_str(&format!(
            "- Non-data-dependent programs certified `const` with 0 annotations: **{} / {}**\n",
            self.non_data_dependent_certified_const, self.non_data_dependent_programs
        ));
        out.push_str(&format!(
            "- Function-level summaries across {} functions: `const` = **{}**, `linear` = **{}**, `unbounded` = **{}**\n",
            self.total_functions_analyzed,
            self.const_functions,
            self.linear_functions,
            self.unbounded_functions
        ));
        out.push_str(&format!(
            "- Static analysis cost over all {} programs: baseline check `{} us`, with alloc pass `{} us`\n\n",
            self.accept_programs_total, self.baseline_check_micros, self.alloc_check_micros
        ));
        out.push_str("| Program | Verdict | Checked Bound | Total Bound | Sites (Checked / Managed) | Linear Functions |\n");
        out.push_str("|---|---|---|---|---|---|\n");
        for r in &self.rows {
            let lin = if r.linear_functions.is_empty() {
                "none".to_string()
            } else {
                r.linear_functions
                    .iter()
                    .map(|(n, s)| format!("`{n}`: `{s}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            out.push_str(&format!(
                "| `{}` | `{}` | `{}` | `{}` | {} ({} / {}) | {} |\n",
                r.file,
                r.expected_verdict,
                r.program_checked_summary,
                r.program_total_summary,
                r.total_sites,
                r.checked_sites,
                r.managed_sites,
                lin
            ));
        }
        out
    }
}

/// Run the static allocation model across all `ACCEPT` and `ACCEPT-MANAGED` `.hy`
/// fixtures in `corpus_dir` and return a structured `CorpusAllocAudit`.
pub fn audit_corpus_dir(corpus_dir: &Path) -> Result<CorpusAllocAudit, String> {
    let mut entries: Vec<_> = std::fs::read_dir(corpus_dir)
        .map_err(|e| format!("reading {}: {e}", corpus_dir.display()))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "hy"))
        .collect();
    entries.sort();

    let mut accept_files: Vec<(String, String, String, File)> = Vec::new();
    for path in entries {
        let raw = std::fs::read_to_string(&path)
            .map_err(|e| format!("reading {}: {e}", path.display()))?;
        let Some(verdict) = extract_expected_accept_verdict(&raw) else {
            continue;
        };
        let fname = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown.hy")
            .to_string();
        let ast = hy_syntax::parse_file(&fname, &raw)
            .map_err(|ds| format!("parse failed on {fname}: {} error(s)", ds.len()))?;
        accept_files.push((fname, verdict, raw, ast));
    }

    let t0 = Instant::now();
    for (_, _, _, ast) in &accept_files {
        let _ = crate::check_file_without_alloc(ast);
    }
    let baseline_check_micros = t0.elapsed().as_micros();

    let t1 = Instant::now();
    let mut rows = Vec::new();
    let mut accept_clean_count = 0usize;
    let mut accept_managed_count = 0usize;
    let mut programs_with_sites = 0usize;
    let mut total_static_sites = 0usize;
    let mut certified_const_zero_programs = 0usize;
    let mut certified_const_programs = 0usize;
    let mut certified_linear_programs = 0usize;
    let mut unbounded_programs = 0usize;
    let mut non_data_dependent_programs = 0usize;
    let mut non_data_dependent_certified_const = 0usize;
    let mut total_functions_analyzed = 0usize;
    let mut const_functions = 0usize;
    let mut linear_functions = 0usize;
    let mut unbounded_functions = 0usize;
    let mut by_kind_totals: BTreeMap<String, usize> = BTreeMap::new();

    for (fname, verdict, raw, ast) in &accept_files {
        if verdict == "ACCEPT-MANAGED" {
            accept_managed_count += 1;
        } else {
            accept_clean_count += 1;
        }

        let res = crate::check_file_with_alloc(ast, Some(raw), &AllocCheckConfig::default());
        assert_eq!(
            res.verdict,
            crate::Verdict::Accept,
            "default un-budgeted check must keep {fname} accepted"
        );
        let rep = &res.alloc;
        if rep.total_sites > 0 {
            programs_with_sites += 1;
        }
        total_static_sites += rep.total_sites;
        for (k, v) in &rep.by_kind {
            *by_kind_totals.entry(k.clone()).or_insert(0usize) += v;
        }

        let uses_data_dependent = rep.sites.iter().any(|s| {
            matches!(
                s.kind,
                AllocKind::MapLit
                    | AllocKind::MapInsert
                    | AllocKind::ZeroMapInit
                    | AllocKind::SliceAppend
                    | AllocKind::MakeRuntime
                    | AllocKind::StringConcat
            )
        });
        if !uses_data_dependent {
            non_data_dependent_programs += 1;
            if rep.program_checked_summary.is_const() {
                non_data_dependent_certified_const += 1;
            }
        }

        match &rep.program_checked_summary {
            AllocSummary::Const(0) => {
                certified_const_zero_programs += 1;
                certified_const_programs += 1;
            }
            AllocSummary::Const(_) => {
                certified_const_programs += 1;
            }
            AllocSummary::Linear { .. } => {
                certified_linear_programs += 1;
            }
            AllocSummary::Unbounded => {
                unbounded_programs += 1;
            }
        }

        let mut fn_lin = Vec::new();
        for f in rep.functions.values() {
            total_functions_analyzed += 1;
            match &f.checked_summary {
                AllocSummary::Const(_) => const_functions += 1,
                AllocSummary::Linear { .. } => {
                    linear_functions += 1;
                    fn_lin.push((f.name.clone(), f.checked_summary.clone()));
                }
                AllocSummary::Unbounded => unbounded_functions += 1,
            }
        }

        rows.push(CorpusProgramAllocRow {
            file: fname.clone(),
            expected_verdict: verdict.clone(),
            program_checked_summary: rep.program_checked_summary.clone(),
            program_total_summary: rep.program_total_summary.clone(),
            total_sites: rep.total_sites,
            checked_sites: rep.checked_sites,
            managed_sites: rep.managed_sites,
            linear_functions: fn_lin,
            uses_data_dependent_surface: uses_data_dependent,
        });
    }
    let alloc_check_micros = t1.elapsed().as_micros();

    Ok(CorpusAllocAudit {
        accept_programs_total: accept_files.len(),
        accept_clean_count,
        accept_managed_count,
        programs_with_sites,
        total_static_sites,
        certified_const_zero_programs,
        certified_const_programs,
        certified_linear_programs,
        unbounded_programs,
        non_data_dependent_programs,
        non_data_dependent_certified_const,
        total_functions_analyzed,
        const_functions,
        linear_functions,
        unbounded_functions,
        by_kind_totals,
        baseline_check_micros,
        alloc_check_micros,
        rows,
    })
}

fn extract_expected_accept_verdict(src: &str) -> Option<String> {
    for line in src.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix("//") else {
            continue;
        };
        let rest = rest.trim_start();
        let Some(after) = rest.strip_prefix("EXPECTED-VERDICT:") else {
            continue;
        };
        let token = after.split_whitespace().next().unwrap_or("");
        if token == "ACCEPT" || token == "ACCEPT-MANAGED" {
            return Some(token.to_string());
        }
        return None;
    }
    None
}

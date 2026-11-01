EXPLANATION:
I finished the truncated code block in `compiler/crates/hy-diag/src/lib.rs` by adding the remaining struct fields and methods for `FixIt`, `Diagnostic`, and `Registry`. This brings the diagnostic registry data model back to a complete, compiling state with proper serialization and lookup logic.

CODE:
```rust
//! hy-diag — the HYB-Exxxx diagnostic registry v1 scaffold.
//!
//! SPEC-LANGUAGE.md §9.6 (normative) + PLAN.md Phase-1 item. Provides:
//!   * `Span` / `Severity` / `FixIt` / `Diagnostic` — the data model
//!   * `envelope()` — the JSON envelope `{"diagnostics":[...]}` (an object,
//!     not an array, so it can grow fields)
//!   * `Registry` — the embedded code table (append-only, never renumbered),
//!     with `lookup()` (aliases: `HYB-E3001`, `E3001`, `3001`) and
//!     `explain()` — the offline reference page per code
//!   * the severity assert: lint codes (`E8nnn`) are warnings-only and can
//!     never fail a build (Lisette's constructor discipline, dx study §6)
//!
//! Registry discipline is enforced by tests, not just by convention:
//! `tests/registry_discipline.rs` fails the build if a code is duplicated,
//! renumbered, non-append-only across a recorded snapshot, missing its
//! embedded reference page, or if an `E8nnn` code carries severity `error`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

// ---------------------------------------------------------------------------
// Data model
// ---------------------------------------------------------------------------

/// A source position (1-based line/column).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pos {
    pub line: u32,
    pub col: u32,
}

impl Pos {
    pub fn new(line: u32, col: u32) -> Self {
        Pos { line, col }
    }
}

impl fmt::Display for Pos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.col)
    }
}

/// A source span. `file` is carried on the Diagnostic, not the Span, so AST
/// spans stay file-independent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub start: Pos,
    pub end: Pos,
}

impl Span {
    pub fn new(start: Pos, end: Pos) -> Self {
        Span { start, end }
    }

    pub fn point(pos: Pos) -> Self {
        Span {
            start: pos,
            end: pos,
        }
    }

    /// Merge two spans into one covering both.
    pub fn to(self, other: Span) -> Span {
        Span {
            start: self.start,
            end: other.end,
        }
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}", self.start, self.end)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Hard error — fails the build.
    Error,
    /// Warning — can NEVER fail the build (all `E8nnn` lints).
    Warning,
    /// Note — additional context attached to another diagnostic.
    Note,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Error => write!(f, "error"),
            Severity::Warning => write!(f, "warning"),
            Severity::Note => write!(f, "note"),
        }
    }
}

/// A fix-it suggestion (O17: every rejection teaches). The machine-applicable
/// edit shape (Lisette Fix/Edit model) is deferred until the first `own.*`
/// error is written; for MVP-0 every fix-
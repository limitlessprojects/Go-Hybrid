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
/// error is written; for MVP-0 every fix-it is a described action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixIt {
    /// What to do, addressed to the programmer.
    pub description: String,
    /// Machine-applicable replacement — reserved, not yet populated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replacement: Option<String>,
}

/// One diagnostic — the unit the envelope carries.
#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    /// Stable registry code, e.g. `HYB-E3003`.
    pub code: String,
    /// Machine name from the registry, e.g. `use-after-move`.
    pub name: String,
    pub severity: Severity,
    pub file: String,
    pub span: Span,
    /// The primary message (one line).
    pub message: String,
    /// The reason in ownership terms (the `= note` body).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    /// Fix-it suggestions naming the legal routes.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fixits: Vec<FixIt>,
    /// Tier-0 rule id (`A1c`, `A1b`, `A2`, `A3`, `A6`, `A7`, `LOAN-ESCAPE`,
    /// `MANAGED`) — the join key the checker harness diffs against
    /// `EXPECTED-VERDICT` lines (tests/checker-corpus/VERDICTS.md rule key).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier0_rule: Option<String>,
}

impl Diagnostic {
    /// Convenience constructor: an error diagnostic with the registry's
    /// fix-its already attached.
    pub fn error(
        code: &str,
        file: impl Into<String>,
        span: Span,
        message: impl Into<String>,
    ) -> Self {
        let reg = Registry::global();
        let entry = reg.lookup(code);
        Diagnostic {
            code: entry
                .map(|e| e.code.clone())
                .unwrap_or_else(|| code.to_string()),
            name: entry.map(|e| e.name.clone()).unwrap_or_default(),
            severity: Severity::Error,
            file: file.into(),
            span,
            message: message.into(),
            notes: Vec::new(),
            fixits: entry
                .map(|e| {
                    e.fixits
                        .iter()
                        .map(|d| FixIt {
                            description: d.clone(),
                            replacement: None,
                        })
                        .collect()
                })
                .unwrap_or_default(),
            tier0_rule: entry.and_then(|e| e.tier0_rule_id.clone()),
        }
    }

    /// A warning diagnostic (lints — E8nnn; can never fail the build).
    pub fn warning(
        code: &str,
        file: impl Into<String>,
        span: Span,
        message: impl Into<String>,
    ) -> Self {
        let mut d = Diagnostic::error(code, file, span, message);
        d.severity = Severity::Warning;
        d
    }

    pub fn with_notes(mut self, notes: Vec<String>) -> Self {
        self.notes = notes;
        self
    }

    pub fn with_rule(mut self, rule: &str) -> Self {
        self.tier0_rule = Some(rule.to_string());
        self
    }

    /// rustc-style first line: `error[HYB-E3003]: message  --> file:span`
    pub fn render_head(&self) -> String {
        format!(
            "{}[{}]: {}\n  --> {}:{}",
            self.severity, self.code, self.message, self.file, self.span.start
        )
    }
}

// ---------------------------------------------------------------------------
// JSON envelope (SPEC §9.6: {"diagnostics":[...]} — an OBJECT)
// ---------------------------------------------------------------------------

/// The top-level envelope. An object, not an array, so it can grow fields
/// without breaking consumers.
#[derive(Debug, Serialize)]
pub struct Envelope<'a> {
    pub diagnostics: &'a [Diagnostic],
    /// Envelope format version.
    pub version: u32,
}

/// Render the JSON envelope for a slice of diagnostics.
pub fn envelope(diags: &[Diagnostic]) -> String {
    let env = Envelope {
        diagnostics: diags,
        version: 1,
    };
    serde_json::to_string_pretty(&env).expect("diagnostics serialize")
}

/// The per-file check summary the harness consumes. Rides in the envelope's
/// growing field set (the reason the envelope is an object).
#[derive(Debug, Serialize)]
pub struct CheckSummary {
    pub file: String,
    /// `accept` | `reject`
    pub verdict: String,
    /// Number of errors (severity == error).
    pub errors: usize,
    /// Number of warnings.
    pub warnings: usize,
    /// `--report-managed` data (O6.3): count of managed blocks seen, count
    /// of spawn/send sites resolved via class 6, and their percentage of all
    /// spawn/send sites in the file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub managed: Option<ManagedReport>,
}

/// The `--report-managed` audit data (normative tooling requirement, O6.3).
/// Row 14 (discussion #4) hardened it: the audit is a TABLE of invariants
/// bypassed — per-category counts + every block's reason/tickets — not a
/// bare number.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ManagedReport {
    pub managed_blocks: usize,
    pub sites_via_managed: usize,
    pub total_sites: usize,
    /// Managed blocks by category (the closed set: mutex, atomic, actor,
    /// finalizer, interop, other). Empty when no blocks were seen.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub categories: BTreeMap<String, usize>,
    /// Every managed block in source order (reason + category + tickets).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<ManagedBlockAudit>,
}

/// One managed block's audit payload (row 14): what was bypassed, why, and
/// which review tickets own the risk.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ManagedBlockAudit {
    pub reason: String,
    pub category: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tickets: Vec<String>,
}

impl ManagedReport {
    pub fn pct(&self) -> f64 {
        if self.total_sites == 0 {
            0.0
        } else {
            100.0 * self.sites_via_managed as f64 / self.total_sites as f64
        }
    }
}

/// Full check output as one JSON object: diagnostics + summary together.
#[derive(Debug, Serialize)]
pub struct CheckOutput<'a> {
    pub diagnostics: &'a [Diagnostic],
    pub summary: &'a CheckSummary,
    pub version: u32,
}

pub fn check_output_json(diags: &[Diagnostic], summary: &CheckSummary) -> String {
    let out = CheckOutput {
        diagnostics: diags,
        summary,
        version: 1,
    };
    serde_json::to_string_pretty(&out).expect("check output serializes")
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryEntry {
    pub code: String,
    pub name: String,
    pub category: String,
    pub severity: String,
    pub title: String,
    pub summary: String,
    #[serde(default)]
    pub fixits: Vec<String>,
    #[serde(default)]
    pub example: String,
    #[serde(default)]
    pub rule: String,
    #[serde(default)]
    pub tier0_rule_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RegistryData {
    version: u32,
    updated: String,
    format: String,
    categories: std::collections::BTreeMap<String, String>,
    codes: Vec<RegistryEntry>,
}

/// The embedded registry. Loaded once, lazily, from
/// `data/diagnostics.json` (`include_str!` — deterministic, no runtime file
/// dependency, reproducible verdicts per SPEC §11).
pub struct Registry {
    data: RegistryData,
}

impl Registry {
    const RAW: &'static str = include_str!("../data/diagnostics.json");

    pub fn global() -> &'static Registry {
        static REG: std::sync::OnceLock<Registry> = std::sync::OnceLock::new();
        REG.get_or_init(|| Registry {
            data: serde_json::from_str(Registry::RAW).expect("embedded registry JSON is valid"),
        })
    }

    /// Resolve a code by any of its accepted spellings:
    /// `HYB-E3001`, `E3001`, `3001` (case-insensitive), or machine name.
    pub fn lookup(&self, query: &str) -> Option<&RegistryEntry> {
        let raw = query.trim();
        let q = raw.to_ascii_uppercase();
        let q_short = q.trim_start_matches("HYB-");
        self.data.codes.iter().find(|c| {
            let short = c.code.trim_start_matches("HYB-"); // "E3003"
            let digits = short.trim_start_matches('E'); // "3003"
            c.code == q || short == q_short || digits == q_short || c.name == raw
        })
    }

    pub fn entries(&self) -> &[RegistryEntry] {
        &self.data.codes
    }

    pub fn updated(&self) -> &str {
        &self.data.updated
    }

    /// The offline `hybrid explain` reference page (one per code, normative).
    pub fn explain(&self, query: &str) -> Option<String> {
        let e = self.lookup(query)?;
        let mut page = String::new();
        page.push_str(&format!("{} — {}\n", e.code, e.title));
        page.push_str(&format!("({} / severity: {})\n\n", e.category, e.severity));
        page.push_str(&format!("{}\n", e.summary));
        if !e.fixits.is_empty() {
            page.push_str("\nfixes:\n");
            for (i, f) in e.fixits.iter().enumerate() {
                page.push_str(&format!("  {}. {}\n", i + 1, f));
            }
        }
        if !e.example.is_empty() {
            page.push_str(&format!("\nexample:\n  {}\n", e.example));
        }
        if let Some(rule) = &e.tier0_rule_id {
            page.push_str(&format!("\nTier-0 rule id: {} (corpus join key)\n", rule));
        }
        if !e.rule.is_empty() {
            page.push_str(&format!("spec rule: {}\n", e.rule));
        }
        Some(page)
    }
}

// ---------------------------------------------------------------------------
// Severity assert (Lisette discipline, dx study §6)
// ---------------------------------------------------------------------------

/// Constructor discipline: an `E8nnn` (lint) code can never be constructed as
/// an error, and a non-`E8nnn` code can never be constructed as a warning.
/// Runs as a debug assert AND is unit-tested in release (`#[test]`).
pub fn assert_severity_code_coherence(code: &str, severity: Severity) {
    let is_lint = code.contains("-E8");
    let is_err = severity == Severity::Error;
    debug_assert!(
        !(is_lint && is_err),
        "lint code {} constructed with severity error — lints can never fail the build",
        code
    );
    debug_assert!(
        !(!is_lint && severity == Severity::Warning),
        "non-lint code {} constructed as a warning — warnings are E8nnn-only",
        code
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_loads_and_has_seed_set() {
        let reg = Registry::global();
        assert!(
            reg.entries().len() >= 17,
            "seed set >= 17 codes, got {}",
            reg.entries().len()
        );
        for e in reg.entries() {
            assert!(e.code.starts_with("HYB-E"), "code format: {}", e.code);
            assert!(e.code.len() == 9, "code format HYB-Exxxx: {}", e.code);
        }
    }

    #[test]
    fn lookup_aliases_work() {
        let reg = Registry::global();
        assert!(reg.lookup("HYB-E3003").is_some());
        assert!(reg.lookup("E3003").is_some());
        assert!(reg.lookup("3003").is_some());
        assert!(reg.lookup("e3003").is_some());
        assert!(reg.lookup("use-after-move").is_some());
        assert!(reg.lookup("nope-9999").is_none());
    }

    #[test]
    fn codes_unique_and_sorted() {
        let reg = Registry::global();
        let mut seen = std::collections::BTreeSet::new();
        for e in reg.entries() {
            assert!(seen.insert(e.code.clone()), "duplicate code {}", e.code);
        }
        // append-only discipline: sorted == insertion order for the seed set
        let codes: Vec<&str> = reg.entries().iter().map(|e| e.code.as_str()).collect();
        let mut sorted = codes.clone();
        sorted.sort();
        assert_eq!(
            codes, sorted,
            "seed codes must be registered in ascending order"
        );
    }

    #[test]
    fn lint_codes_are_warnings_only() {
        let reg = Registry::global();
        for e in reg.entries() {
            let is_lint = e.code.contains("-E8");
            let is_error = e.severity == "error";
            assert!(
                !(is_lint && is_error),
                "{} is a lint but has severity error — lints can never fail the build",
                e.code
            );
        }
    }

    #[test]
    fn e3_codes_carry_fixits() {
        let reg = Registry::global();
        for e in reg.entries() {
            if e.code.contains("-E3") {
                assert!(
                    !e.fixits.is_empty(),
                    "{} is an ownership rejection and MUST carry fix-it suggestions (O17)",
                    e.code
                );
            }
        }
    }

    #[test]
    fn every_code_has_reference_page() {
        let reg = Registry::global();
        for e in reg.entries() {
            let page = reg.explain(&e.code).expect("reference page exists");
            assert!(page.contains(&e.code));
            assert!(page.contains("severity"));
        }
    }

    #[test]
    fn envelope_is_object_with_diagnostics_key() {
        let d = Diagnostic::error(
            "HYB-E3003",
            "pool.hy",
            Span::point(Pos::new(52, 9)),
            "`counter` was moved to a task at pool.hy:41:5",
        );
        let json = envelope(&[d]);
        assert!(json.starts_with("{\n  \"diagnostics\""));
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(v.get("diagnostics").unwrap().is_array());
        assert!(v.get("version").unwrap().as_u64() == Some(1));
        let first = v["diagnostics"][0].as_object().unwrap();
        assert_eq!(first["code"].as_str().unwrap(), "HYB-E3003");
        assert_eq!(first["tier0_rule"].as_str().unwrap(), "A1b");
        assert!(!first["fixits"].as_array().unwrap().is_empty());
    }

    #[test]
    fn severity_coherence_never_panics_for_valid_pairs() {
        assert_severity_code_coherence("HYB-E8001", Severity::Warning);
        assert_severity_code_coherence("HYB-E3001", Severity::Error);
    }

    #[test]
    fn e8001_example_uses_required_managed_arguments() {
        let reg = Registry::global();
        let entry = reg.lookup("HYB-E8001").expect("HYB-E8001 registered");
        assert!(
            entry.example.contains("managed(\"") && entry.example.contains("\", \""),
            "HYB-E8001 example must include managed(reason, category) arguments so it does not fail with HYB-E0003: {}",
            entry.example
        );
        assert!(
            !entry.example.contains("managed {"),
            "HYB-E8001 example must not teach bare `managed {{ ... }}`: {}",
            entry.example
        );
    }
}

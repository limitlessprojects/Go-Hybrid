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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
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
        Span {
            start: start.min(end),
            end: start.max(end),
        }
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
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}", self.start, self.end)
    }
}

/// A zero-based, half-open UTF-8 byte interval `[start, end)` in a checked source snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ByteSpan {
    pub start: usize,
    pub end: usize,
}

/// 1-based display coordinates derived from a checked UTF-8 byte offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplayPos {
    pub line: u32,
    pub scalar_col: u32,
    pub utf16_col: u32,
    pub utf8_col: u32,
}

/// Start and end display coordinates for a source span.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplaySpan {
    pub start: DisplayPos,
    pub end: DisplayPos,
}

/// Failure when resolving a previously emitted diagnostic against current source state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JumpError {
    /// Source content digest or document version changed since the diagnostic was produced.
    StaleSnapshot,
    /// Span coordinates do not resolve cleanly to valid UTF-8 boundaries in the snapshot.
    InvalidSpan,
}

/// Verified half-open UTF-8 byte interval plus multi-encoding display coordinates for jump targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedJump {
    pub byte_span: ByteSpan,
    pub display: DisplaySpan,
}

/// Compute lowercase FIPS 180-4 SHA-256 hex digest of a source snapshot using stdlib only.
pub fn snapshot_sha256(src: &str) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
        0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];

    let bytes = src.as_bytes();
    let bit_len = (bytes.len() as u64).wrapping_mul(8);
    let mut padded = Vec::with_capacity(bytes.len() + 72);
    padded.extend_from_slice(bytes);
    padded.push(0x80);
    while (padded.len() % 64) != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for block in padded.chunks_exact(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            let j = i * 4;
            w[i] = u32::from_be_bytes([block[j], block[j + 1], block[j + 2], block[j + 3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        for (slot, delta) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *slot = slot.wrapping_add(delta);
        }
    }

    let mut hex = String::with_capacity(64);
    for word in h {
        use fmt::Write;
        let _ = write!(hex, "{word:08x}");
    }
    hex
}

/// Convert a 1-based `(line, scalar_col)` lexer position into a zero-based UTF-8 byte offset.
/// Matches `hy_syntax::Lexer::bump` line/column accounting (1-based line split on `\n`,
/// 1-based column incremented per Unicode scalar), including end-of-line and EOF positions.
pub fn pos_to_byte_offset(src: &str, pos: Pos) -> Option<usize> {
    if pos.line == 0 || pos.col == 0 {
        return None;
    }
    let mut line: u32 = 1;
    let mut col: u32 = 1;
    for (byte_idx, ch) in src.char_indices() {
        if line == pos.line && col == pos.col {
            return Some(byte_idx);
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    if line == pos.line && col == pos.col {
        Some(src.len())
    } else {
        None
    }
}

/// Convert a scalar `Span` into a zero-based half-open UTF-8 [`ByteSpan`].
pub fn span_to_byte_span(src: &str, span: Span) -> Option<ByteSpan> {
    let start = pos_to_byte_offset(src, span.start)?;
    let end = pos_to_byte_offset(src, span.end)?;
    if start <= end {
        Some(ByteSpan { start, end })
    } else {
        None
    }
}

/// Derive 1-based scalar, UTF-16, and UTF-8 column coordinates from a zero-based UTF-8 byte offset.
pub fn byte_offset_to_display_pos(src: &str, byte_offset: usize) -> Option<DisplayPos> {
    if byte_offset > src.len() || !src.is_char_boundary(byte_offset) {
        return None;
    }
    let mut line: u32 = 1;
    let mut scalar_col: u32 = 1;
    let mut utf16_col: u32 = 1;
    let mut utf8_col: u32 = 1;
    for ch in src[..byte_offset].chars() {
        if ch == '\n' {
            line += 1;
            scalar_col = 1;
            utf16_col = 1;
            utf8_col = 1;
        } else {
            scalar_col += 1;
            utf16_col += ch.len_utf16() as u32;
            utf8_col += ch.len_utf8() as u32;
        }
    }
    Some(DisplayPos {
        line,
        scalar_col,
        utf16_col,
        utf8_col,
    })
}

/// Derive start/end display coordinates from a [`ByteSpan`].
pub fn byte_span_to_display_span(src: &str, byte_span: ByteSpan) -> Option<DisplaySpan> {
    if byte_span.start > byte_span.end {
        return None;
    }
    let start = byte_offset_to_display_pos(src, byte_span.start)?;
    let end = byte_offset_to_display_pos(src, byte_span.end)?;
    Some(DisplaySpan { start, end })
}

/// Attach the checked snapshot SHA-256 digest and half-open UTF-8 [`ByteSpan`] to every diagnostic.
pub fn stamp_diagnostics_for_source(src: &str, diags: &mut [Diagnostic]) {
    if diags.is_empty() {
        return;
    }
    let digest = snapshot_sha256(src);
    for diag in diags {
        diag.snapshot_sha256 = Some(digest.clone());
        diag.byte_span = span_to_byte_span(src, diag.span);
    }
}

/// Verify snapshot identity and resolve a diagnostic to exact UTF-8 byte offsets and display columns.
/// Returns [`JumpError::StaleSnapshot`] if the document version or source digest has changed.
pub fn resolve_diagnostic_jump(
    diag: &Diagnostic,
    current_src: &str,
    expected_doc_version: Option<u64>,
    current_doc_version: Option<u64>,
) -> Result<ResolvedJump, JumpError> {
    if expected_doc_version != current_doc_version {
        return Err(JumpError::StaleSnapshot);
    }
    if let Some(expected_digest) = diag.snapshot_sha256.as_deref() {
        if expected_digest != snapshot_sha256(current_src) {
            return Err(JumpError::StaleSnapshot);
        }
    }

    let derived = span_to_byte_span(current_src, diag.span);
    let byte_span = match (diag.byte_span, derived) {
        (Some(stamped), Some(from_span)) => {
            if stamped != from_span {
                return Err(JumpError::InvalidSpan);
            }
            stamped
        }
        (Some(stamped), None) => {
            if stamped.start > stamped.end
                || stamped.end > current_src.len()
                || !current_src.is_char_boundary(stamped.start)
                || !current_src.is_char_boundary(stamped.end)
            {
                return Err(JumpError::InvalidSpan);
            }
            stamped
        }
        (None, Some(from_span)) => from_span,
        (None, None) => return Err(JumpError::InvalidSpan),
    };

    let display = byte_span_to_display_span(current_src, byte_span).ok_or(JumpError::InvalidSpan)?;
    Ok(ResolvedJump { byte_span, display })
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
    /// Zero-based half-open UTF-8 byte interval `[start, end)` in the checked snapshot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub byte_span: Option<ByteSpan>,
    /// SHA-256 digest of the source snapshot against which `span` and `byte_span` were checked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot_sha256: Option<String>,
}

/// Trim surrounding whitespace, brackets, quotes, angle brackets, parentheses,
/// and trailing punctuation (colon, comma, period, semicolon) from a code or alias query.
pub(crate) fn trim_code_query(query: &str) -> &str {
    query.trim_matches(|c: char| {
        c.is_whitespace()
            || matches!(
                c,
                '[' | ']' | '(' | ')' | '<' | '>' | '{' | '}' | ':' | ',' | '.' | ';' | '\'' | '"' | '`'
            )
    })
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
        Self::build(code, Severity::Error, file.into(), span, message.into())
    }

    fn build(code: &str, severity: Severity, file: String, span: Span, message: String) -> Self {
        assert_severity_code_coherence(code, severity);
        let entry = Registry::global().lookup(code);
        Diagnostic {
            code: entry
                .map(|e| e.code.clone())
                .unwrap_or_else(|| trim_code_query(code).replace('_', "-").to_ascii_uppercase()),
            name: entry.map(|e| e.name.clone()).unwrap_or_default(),
            severity,
            file,
            span,
            message,
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
            byte_span: None,
            snapshot_sha256: None,
        }
    }

    /// A warning diagnostic (lints — E8nnn; can never fail the build).
    pub fn warning(
        code: &str,
        file: impl Into<String>,
        span: Span,
        message: impl Into<String>,
    ) -> Self {
        Self::build(code, Severity::Warning, file.into(), span, message.into())
    }

    pub fn with_notes(mut self, notes: Vec<String>) -> Self {
        self.notes = notes.into_iter().map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).collect();
        self
    }

    pub fn with_rule(mut self, rule: &str) -> Self {
        let rule = rule.trim();
        self.tier0_rule = (!rule.is_empty()).then(|| rule.to_string());
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
    /// SHA-256 digest of the checked `.hy` source file for consumer staleness checks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot_sha256: Option<String>,
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
            (100.0 * self.sites_via_managed as f64 / self.total_sites as f64).min(100.0)
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
    /// `HYB-E3001`, `E3001`, `E-3001`, `3001` (case-insensitive), or machine name.
    pub fn lookup(&self, query: &str) -> Option<&RegistryEntry> {
        let raw = trim_code_query(query).replace('_', "-");
        let q = raw.to_ascii_uppercase();
        self.data.codes.iter().find(|c| {
            let short = c.code.strip_prefix("HYB-").unwrap_or(&c.code);
            let digits = short.strip_prefix('E').unwrap_or(short);
            c.code == q
                || short == q
                || q.strip_prefix("E-") == Some(digits)
                || digits == q
                || q.strip_prefix("HYB-") == Some(digits)
                || q.strip_prefix("HYB-E-") == Some(digits)
                || c.name.eq_ignore_ascii_case(&raw)
                || q
                    .strip_prefix("HYB-")
                    .map(|m| c.name.eq_ignore_ascii_case(m))
                    .unwrap_or(false)
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
    let canonical = Registry::global().lookup(code).map(|e| e.code.clone()).unwrap_or_else(|| {
        trim_code_query(code).replace('_', "-").to_ascii_uppercase()
    });
    let is_lint = canonical.contains("-E8")
        || canonical.contains("-8")
        || canonical.starts_with("E8")
        || canonical.starts_with("8");
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
        assert_severity_code_coherence("E8001", Severity::Warning);
        assert_severity_code_coherence("HYB-E3001", Severity::Error);
        assert!(Registry::global().lookup("HYB-HYB-E3003").is_none());
        assert!(Registry::global().lookup("USE-AFTER-MOVE").is_some());
        let s = Span::new(Pos::new(5, 1), Pos::new(5, 8)).to(Span::new(Pos::new(2, 4), Pos::new(2, 10)));
        assert_eq!((s.start, s.end), (Pos::new(2, 4), Pos::new(5, 8)));
    }
}

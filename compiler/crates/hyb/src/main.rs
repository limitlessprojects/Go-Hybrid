//! hyb — the Hybrid CLI (MVP-0 shape).
//!
//! Subcommands:
//!   check   — parse + Tier-0 check a .hy file (human or --json output)
//!   explain — offline reference page for a HYB-Exxxx code
//!   build   — the walking skeleton: parse → check → emit Go → go build

use std::process::{Command, ExitCode};

use hy_diag::{Diagnostic, Pos, Span};

/// Version pins enforced by `hyb build` (HYB-E9001). Overridable at COMPILE
/// time via env (tools/env.sh exports them); the defaults match PLAN.md's
/// environment row so a plain `cargo build` still enforces the right pins.
const PINNED_GO: &str = match option_env!("HYB_PINNED_GO") {
    Some(v) => v,
    None => "go1.26.9",
};
const PINNED_RUST: &str = match option_env!("HYB_PINNED_RUST") {
    Some(v) => v,
    None => "1.99.0",
};

/// The Go binary: HYB_GO_BIN override, else PATH (tools/env.sh puts the
/// pinned toolchain first; CI prepends /usr/local/go/bin). The version-pin
/// check below still enforces go1.26.9, so PATH lookup cannot drift silently.
fn go_bin() -> &'static str {
    option_env!("HYB_GO_BIN").unwrap_or("go")
}

/// The cargo binary for the bridge's hyrt build: HYB_CARGO_BIN override,
/// else PATH. (No absolute fallback: the rustless install contract — the
/// release smoke strips cargo from PATH — would be silently defeated by a
/// ~/.cargo/bin fallback rebuilding a fresh lib the shipped tarball already
/// carries. If freshness ever fails without cargo, fail LOUDLY instead.)
fn cargo_bin() -> String {
    if let Ok(c) = std::env::var("HYB_CARGO_BIN") {
        return c;
    }
    "cargo".to_string()
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("explain") => cmd_explain(&args[1..]),
        Some("check") => cmd_check(&args[1..]),
        Some("build") => cmd_build(&args[1..]),
        Some("generate") => cmd_generate(&args[1..]),
        Some("vendor-runtime") => cmd_vendor_runtime(&args[1..]),
        Some("bench-overhead") => cmd_bench_overhead(),
        _ => {
            eprintln!("hyb — the Hybrid compiler CLI (MVP-0)");
            eprintln!();
            eprintln!("usage:");
            eprintln!("  hyb check <file.hy> [--json] [--report-managed] [--deny-managed]");
            eprintln!("  hyb generate [--check] [--binding-prefix <path>] <paths...>  (existing go.mod projects)");
            eprintln!("  hyb vendor-runtime <dir>  (vendored runtime for a go.mod project)");
            eprintln!("  hyb explain <HYB-Exxxx | Exxxx | xxxx | machine-name>");
            eprintln!("  hyb build <file.hy> -o <bin-path> [--emit-go] [--deny-managed]");
            eprintln!("  hyb bench-overhead");
            eprintln!();
            eprintln!(
                "build pins: {} / rust {} (HYB-E9001 on drift)",
                PINNED_GO, PINNED_RUST
            );
            ExitCode::from(2)
        }
    }
}

fn cmd_explain(args: &[String]) -> ExitCode {
    let Some(code) = args.first() else {
        eprintln!("usage: hyb explain <code>");
        return ExitCode::from(2);
    };
    match hy_diag::Registry::global().explain(code) {
        Some(page) => {
            print!("{}", page);
            ExitCode::SUCCESS
        }
        None => {
            eprintln!(
                "unknown diagnostic code: {} (registry has {} codes)",
                code,
                hy_diag::Registry::global().entries().len()
            );
            ExitCode::from(2)
        }
    }
}

fn cmd_check(args: &[String]) -> ExitCode {
    const USAGE: &str =
        "usage: hyb check [--json] [--report-managed] [--deny-managed] [--] <file.hy>";
    let mut file = None;
    let mut json = false;
    let mut report_managed = false;
    let mut deny_managed = false;
    let mut end_of_options = false;
    // Validate the entire invocation before reading or checking any source.
    // Otherwise a second file or a mistyped option can silently go unchecked.
    for arg in args {
        if !end_of_options {
            match arg.as_str() {
                "--" => {
                    end_of_options = true;
                    continue;
                }
                "--json" => {
                    json = true;
                    continue;
                }
                "--report-managed" => {
                    report_managed = true;
                    continue;
                }
                "--deny-managed" => {
                    deny_managed = true;
                    continue;
                }
                value if value.starts_with('-') => {
                    eprintln!("unknown option: {value}\n{USAGE}");
                    return ExitCode::from(2);
                }
                _ => {}
            }
        }
        if file.is_some() {
            eprintln!("check expects exactly one input file; unexpected argument: {arg}\n{USAGE}");
            return ExitCode::from(2);
        }
        file = Some(arg.clone());
    }
    let Some(file) = file else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };

    let src = match std::fs::read_to_string(&file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error reading {}: {}", file, e);
            return ExitCode::from(2);
        }
    };

    // parse
    let ast = match hy_syntax::parse_file(&file, &src) {
        Ok(ast) => ast,
        Err(diags) => {
            if json {
                let summary = hy_diag::CheckSummary {
                    file: file.clone(),
                    verdict: "reject".into(),
                    errors: diags.len(),
                    warnings: 0,
                    managed: None,
                };
                println!("{}", hy_diag::check_output_json(&diags, &summary));
            } else {
                for d in &diags {
                    println!("{}", d.render_head());
                }
            }
            return ExitCode::FAILURE;
        }
    };

    // check
    let result = hy_check::check_file(&ast);
    let errors = result
        .diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .count();
    let warnings = result
        .diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Warning)
        .count();
    let summary = hy_diag::CheckSummary {
        file: file.clone(),
        verdict: result.verdict.as_str().to_string(),
        errors,
        warnings,
        managed: if report_managed {
            Some(result.managed.clone())
        } else {
            None
        },
    };

    if json {
        println!(
            "{}",
            hy_diag::check_output_json(&result.diagnostics, &summary)
        );
    } else {
        for d in &result.diagnostics {
            println!("{}", d.render_head());
            for n in &d.notes {
                println!("  = {}", n);
            }
            if !d.fixits.is_empty() {
                println!("  = help: choose one —");
                for (i, f) in d.fixits.iter().enumerate() {
                    println!("    {}. {}", i + 1, f.description);
                }
            }
        }
        if report_managed {
            println!(
                "managed: {} block(s), {} of {} spawn/send sites resolved via managed ({:.1}%)",
                result.managed.managed_blocks,
                result.managed.sites_via_managed,
                result.managed.total_sites,
                result.managed.pct()
            );
            // Row 14 (discussion #4): the audit is a table of invariants
            // bypassed, not a bare number.
            if !result.managed.categories.is_empty() {
                let cats: Vec<String> = result
                    .managed
                    .categories
                    .iter()
                    .map(|(c, n)| format!("{}: {}", c, n))
                    .collect();
                println!("  categories: {}", cats.join(", "));
            }
            for b in &result.managed.blocks {
                let tickets = if b.tickets.is_empty() {
                    String::new()
                } else {
                    format!(" [{}]", b.tickets.join(", "))
                };
                println!("  block: [{}] \"{}\"{}", b.category, b.reason, tickets);
            }
        }
        if result.diagnostics.is_empty() {
            println!("{}: OK (0 diagnostics)", file);
        }
    }

    // CI policy gate (--deny-managed): any managed block fails the check.
    // Pairs with --report-managed (count) the way `unsafe` pairs with audit
    // tooling — the hatch stays greppable, auditable, countable, AND now
    // optionally forbidden. Discussion #1 (ssmurfgg04-gif) ask.
    if deny_managed && result.managed.managed_blocks > 0 {
        let msg = format!(
            "deny-managed: {} managed block(s) present — check failed by policy (--deny-managed); \
             audit the block(s) or remove them (HYB-E8001 counts them under --report-managed)",
            result.managed.managed_blocks
        );
        if json {
            eprintln!("{}", msg);
        } else {
            println!("{}", msg);
        }
        return ExitCode::FAILURE;
    }

    if errors > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// The generated-file header (the DO-NOT-EDIT contract `generate --check`
/// and the overwrite guard both key on).
const GENERATED_HEADER_PREFIX: &str = "// Code generated by hyb from ";

/// Row 13 (discussion #3, design B): transpile .hy files to checked-in Go
/// BESIDE an existing go.mod project — `go:generate hyb generate ./...` in
/// one Go file, then ordinary `go build` / `go test`. Deleting a .hy file
/// yields plain reviewable Go (the pitch is literal). The checker still runs
/// fail-closed: a rejected .hy never emits.
///
///   hyb generate [--check] [--binding-prefix <path>] <paths...>
///
/// --check (the CI mode): re-emit in memory and compare — a missing or
/// drifted <stem>.hy.go exits 1 with the offending list (staleness is a
/// build failure, never a silent skip).
fn cmd_generate(args: &[String]) -> ExitCode {
    // Validate the complete invocation BEFORE discovering or writing files.
    // Ignoring an unknown option makes a typo such as --chekc run in write
    // mode. Consume option values by position: a path may have the same
    // spelling as the binding prefix without being that option's value.
    let mut check_only = false;
    let mut binding_prefix = "hybrid.program".to_string();
    let mut prefix_seen = false;
    let mut paths = Vec::new();
    let mut arguments = args.iter();
    while let Some(arg) = arguments.next() {
        match arg.as_str() {
            "--check" => check_only = true,
            "--binding-prefix" => {
                if prefix_seen {
                    eprintln!("error: --binding-prefix may only be supplied once");
                    return ExitCode::from(2);
                }
                let Some(value) = arguments.next() else {
                    eprintln!("error: --binding-prefix requires a value");
                    return ExitCode::from(2);
                };
                if value.is_empty() || value.starts_with('-') {
                    eprintln!("error: --binding-prefix requires a value, not an option");
                    return ExitCode::from(2);
                }
                binding_prefix = value.clone();
                prefix_seen = true;
            }
            "--" => {
                paths.extend(arguments.filter(|p| !p.ends_with(".go")).cloned());
                break;
            }
            _ if arg.starts_with('-') => {
                eprintln!("error: unknown generate option: {}", arg);
                return ExitCode::from(2);
            }
            _ if !arg.ends_with(".go") => paths.push(arg.clone()),
            _ => {}
        }
    }
    if paths.is_empty() {
        eprintln!("usage: hyb generate [--check] [--binding-prefix <path>] <file.hy | dir> ...");
        return ExitCode::from(2);
    }

    // Collect .hy files (dirs walk recursively — go:generate ./... shape).
    let mut hy_files: Vec<std::path::PathBuf> = Vec::new();
    for p in &paths {
        let path = std::path::Path::new(p);
        if path.is_file() && path.extension().map(|e| e == "hy").unwrap_or(false) {
            hy_files.push(path.to_path_buf());
        } else if path.is_dir() {
            let mut stack = vec![path.to_path_buf()];
            while let Some(d) = stack.pop() {
                let mut entries: Vec<_> = match std::fs::read_dir(&d) {
                    Ok(rd) => rd.flatten().collect(),
                    Err(e) => {
                        eprintln!("error reading {}: {}", d.display(), e);
                        return ExitCode::from(2);
                    }
                };
                entries.sort_by_key(|e| e.file_name());
                for e in entries {
                    let p = e.path();
                    if p.is_dir() {
                        // Do not recursively follow directory links: a project
                        // may link back to itself or to another source tree.
                        // Explicit directory roots and .hy file links still work.
                        if e.file_type().map(|t| t.is_symlink()).unwrap_or(false) {
                            continue;
                        }
                        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                        if name.starts_with('.') || name == "hybridrt" || name == "vendor" {
                            continue; // no tools' dirs, no the vendored runtime
                        }
                        stack.push(p);
                    } else if p.extension().map(|x| x == "hy").unwrap_or(false) {
                        hy_files.push(p);
                    }
                }
            }
        } else {
            eprintln!("error: {} is not a .hy file or a directory", p);
            return ExitCode::from(2);
        }
    }
    if hy_files.is_empty() {
        eprintln!("error: no .hy files under {:?}", paths);
        return ExitCode::from(2);
    }
    hy_files.sort();

    let mut drifted: Vec<String> = Vec::new();
    let mut written: usize = 0;
    let mut formatters = None;
    for hy in &hy_files {
        match generate_one(hy, check_only, &binding_prefix, &mut formatters) {
            Ok(GenOutcome::Wrote(f)) => {
                written += 1;
                println!("emitted {}", f);
            }
            Ok(GenOutcome::Fresh(f)) => {
                println!("fresh {}", f);
            }
            Ok(GenOutcome::Drift(f)) => drifted.push(f),
            Err(msg) => {
                eprintln!("error: {}", msg);
                return ExitCode::FAILURE;
            }
        }
    }
    if check_only {
        if drifted.is_empty() {
            println!("generate --check: {} file(s) fresh", hy_files.len());
            ExitCode::SUCCESS
        } else {
            eprintln!("generate --check: {} file(s) STALE or missing — the generated Go must match the .hy sources (run `hyb generate` and commit):", drifted.len());
            for f in &drifted {
                eprintln!("  {}", f);
            }
            ExitCode::FAILURE
        }
    } else {
        println!("generate: {} file(s) written, {} total", written, hy_files.len());
        ExitCode::SUCCESS
    }
}

enum GenOutcome {
    Wrote(String),
    Fresh(String),
    Drift(String),
}

/// gofmt over stdin→stdout — the one formatter that decides what a
/// checked-in Go file looks like. Required (not optional): a non-canonical
/// checked-in file would make --check compare against a moving target.
fn run_gofmt(src: &str) -> Result<String, String> {
    run_gofmt_candidates(src, &gofmt_candidates())
}

fn gofmt_candidates() -> Vec<std::path::PathBuf> {
    // Candidate order: GOROOT/bin/gofmt (works whenever go works — the
    // canonical location), then PATH. `hyb generate` runs inside a Go
    // project, so the toolchain is present by definition.
    let goroot_bin_gofmt = std::process::Command::new(go_bin())
        .args(["env", "GOROOT"])
        .output()
        .ok()
        .and_then(|o| {
            let root = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if root.is_empty() {
                None
            } else {
                Some(std::path::PathBuf::from(root).join("bin/gofmt"))
            }
        });
    goroot_bin_gofmt
        .into_iter()
        .chain(std::iter::once(std::path::PathBuf::from("gofmt")))
        .collect()
}

fn run_gofmt_candidates(src: &str, candidates: &[std::path::PathBuf]) -> Result<String, String> {
    let mut last_err: Option<String> = None;
    for gofmt in candidates {
        match run_gofmt_with(gofmt, src) {
            Ok(out) => return Ok(out),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| {
        "gofmt not found — hyb generate needs the Go toolchain to emit canonical files".to_string()
    }))
}

fn run_gofmt_with(gofmt: &std::path::Path, src: &str) -> Result<String, String> {
    use std::io::Write;
    let mut child = std::process::Command::new(gofmt)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| {
            format!(
                "running gofmt ({}): {} — hyb generate needs the Go toolchain to emit canonical files",
                gofmt.display(),
                e
            )
        })?;
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(src.as_bytes())
        .map_err(|e| format!("piping to gofmt: {}", e))?;
    let out = child
        .wait_with_output()
        .map_err(|e| format!("waiting for gofmt: {}", e))?;
    if !out.status.success() {
        return Err(format!(
            "gofmt rejected the emitted Go (an emitter bug — please report):\n{}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    String::from_utf8(out.stdout).map_err(|e| format!("gofmt output: {}", e))
}

/// Generate one .hy file: parse → check (fail-closed) → infer the package
/// from sibling .go files → emit <stem>.hy.go beside it (or --check it).
fn generate_one(
    hy: &std::path::Path,
    check_only: bool,
    binding_prefix: &str,
    formatters: &mut Option<Vec<std::path::PathBuf>>,
) -> Result<GenOutcome, String> {
    let src = std::fs::read_to_string(hy)
        .map_err(|e| format!("reading {}: {}", hy.display(), e))?;
    let ast = hy_syntax::parse_file(&hy.to_string_lossy(), &src)
        .map_err(|diags| {
            let mut m = format!("{} failed the checker (nothing emitted):", hy.display());
            for d in &diags {
                m.push_str(&format!("\n  {}", d.render_head()));
            }
            m
        })?;
    let result = hy_check::check_file(&ast);
    if result.verdict == hy_check::Verdict::Reject {
        let mut m = format!("{} failed the checker (nothing emitted):", hy.display());
        for d in &result.diagnostics {
            if d.severity == hy_diag::Severity::Error {
                m.push_str(&format!("\n  {}", d.render_head()));
            }
        }
        return Err(m);
    }

    // Package inference: the sibling .go files decide (an existing go.mod
    // project has its own package clauses; .hy has none BY DESIGN — the
    // emitted file joins the directory's package). Siblings must agree.
    // A bare filename's parent() is Some("") — an EMPTY relative path,
    // which read_dir rejects; normalize it to "." so `hyb generate
    // --check calc.hy` works beside the `./calc.hy` and `.` forms.
    let dir = hy
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let mut pkg: Option<String> = None;
    let mut siblings = std::fs::read_dir(&dir)
        .map_err(|e| format!("reading {}: {}", dir.display(), e))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "go").unwrap_or(false))
        // Generated .hy.go files join the production package. Go test files
        // may declare a separate <package>_test package and cannot choose it,
        // and pre-existing .hy.go outputs are generated artifacts (not
        // hand-written package authorities).
        .filter(|p| {
            let name = p.file_name().unwrap_or_default().to_string_lossy();
            !name.ends_with("_test.go") && !name.ends_with(".hy.go")
        })
        .collect::<Vec<_>>();
    siblings.sort();
    if siblings.is_empty() {
        return Err(format!(
            "{}: no non-test .go files beside it — the emitted file must join a Go package (HYB-E9002); add a non-test .go file with a `package <name>` clause first",
            hy.display()
        ));
    }
    for go in &siblings {
        let text = std::fs::read_to_string(go)
            .map_err(|e| format!("reading {}: {}", go.display(), e))?;
        if let Some(name) = extract_go_package_name(&text) {
            match &pkg {
                None => pkg = Some(name),
                Some(p) if p != &name => {
                    return Err(format!(
                        "{}: sibling .go files disagree on the package ({} vs {} in {})",
                        hy.display(),
                        p,
                        name,
                        go.display()
                    ));
                }
                _ => {}
            }
        }
    }
    let Some(pkg_name) = pkg else {
        return Err(format!(
            "{}: no `package <name>` clause found in the sibling .go files (HYB-E9002)",
            hy.display()
        ));
    };

    let go_src = hy_emit::emit_go_package(&ast, &pkg_name, binding_prefix)
        .map_err(|e| format!("[emit] {}: {}", hy.display(), e.message))?;
    let hy_name = hy.file_name().and_then(|n| n.to_str()).unwrap_or("?");
    let header = format!(
        "{}{}. DO NOT EDIT.\n\n",
        GENERATED_HEADER_PREFIX,
        hy_name
    );
    let unformatted = format!("{}{}", header, go_src);
    // The checked-in file must be gofmt-CANONICAL (row 13's constraint:
    // reviewable, diffable, go build-clean) — so generate pipes it through
    // the project's own gofmt. (The emitter now emits gofmt-canonical
    // indentation directly — the base-indent sweep landed 2026-10-14 — the
    // pass stays as defense in depth for anything gofmt normalizes beyond
    // indentation.)
    // Discover lazily after source validation, once per generate invocation.
    let out_text = run_gofmt_candidates(
        &unformatted,
        formatters.get_or_insert_with(gofmt_candidates),
    )?;

    let stem = hy
        .file_stem()
        .and_then(|n| n.to_str())
        .unwrap_or("program")
        .to_string();
    let out_path = dir.join(format!("{}.hy.go", stem));
    let out_disp = out_path.display().to_string();

    // Overwrite guard: never clobber a hand-written file.
    if let Ok(existing) = std::fs::read_to_string(&out_path) {
        if !existing.starts_with(GENERATED_HEADER_PREFIX) {
            return Err(format!(
                "{} exists and is NOT hyb-generated — refusing to overwrite a hand-written file",
                out_disp
            ));
        }
    }

    if check_only {
        match std::fs::read_to_string(&out_path) {
            Ok(existing) if existing == out_text => Ok(GenOutcome::Fresh(out_disp)),
            _ => Ok(GenOutcome::Drift(out_disp)),
        }
    } else {
        std::fs::write(&out_path, &out_text)
            .map_err(|e| format!("writing {}: {}", out_disp, e))?;
        Ok(GenOutcome::Wrote(out_disp))
    }
}

/// Row 13: vendor the runtime for a go.mod project — one boring command.
///
///   hyb vendor-runtime <dir>
///
/// Produces <dir>/hybridrt/ with the Go binding packages, a `module
/// hybrid.program` go.mod, and the fresh arch-matched staticlib at lib/ —
/// then the project's go.mod gets:
///   require hybrid.program v0.0.0
///   replace hybrid.program => ./hybridrt
/// and emitted imports resolve as ordinary local packages. Idempotent.
fn cmd_vendor_runtime(args: &[String]) -> ExitCode {
    const USAGE: &str = "usage: hyb vendor-runtime [--target-dir <dir> | <dir>]  (creates <dir>/hybridrt)";
    let mut target_dir: Option<String> = None;
    let mut target_flag_seen = false;
    let mut end_of_options = false;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if !end_of_options {
            match arg.as_str() {
                "--" => {
                    end_of_options = true;
                    continue;
                }
                "--target-dir" => {
                    if target_flag_seen || target_dir.is_some() {
                        eprintln!("error: target directory may only be supplied once\n{USAGE}");
                        return ExitCode::from(2);
                    }
                    let Some(val) = iter.next() else {
                        eprintln!("error: --target-dir requires a value\n{USAGE}");
                        return ExitCode::from(2);
                    };
                    if val.is_empty() || val.starts_with('-') {
                        eprintln!("error: --target-dir requires a value, not an option\n{USAGE}");
                        return ExitCode::from(2);
                    }
                    target_dir = Some(val.clone());
                    target_flag_seen = true;
                    continue;
                }
                _ if arg.starts_with('-') => {
                    eprintln!("error: unknown vendor-runtime option: {arg}\n{USAGE}");
                    return ExitCode::from(2);
                }
                _ => {}
            }
        }
        if target_dir.is_some() {
            eprintln!("error: vendor-runtime expects a single target directory; unexpected argument: {arg}\n{USAGE}");
            return ExitCode::from(2);
        }
        target_dir = Some(arg.clone());
    }
    let Some(d) = target_dir else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let dir = std::path::PathBuf::from(d);
    let rt = match runtime_dir() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {}", render_build_error(&e));
            return ExitCode::FAILURE;
        }
    };
    let lib = match ensure_staticlib(&rt) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("error: {}", render_build_error(&e));
            return ExitCode::FAILURE;
        }
    };
    let dst = dir.join("hybridrt");
    // Copy the binding packages.
    let go_src = rt.join("go");
    let entries = match std::fs::read_dir(&go_src) {
        Ok(e) => e,
        Err(err) => {
            eprintln!("error reading {}: {}", go_src.display(), err);
            return ExitCode::FAILURE;
        }
    };
    for e in entries.flatten() {
        let p = e.path();
        if !p.is_dir() {
            continue;
        }
        let name = e.file_name();
        let out_pkg = dst.join(&name);
        std::fs::create_dir_all(&out_pkg).ok();
        let sub_entries = match std::fs::read_dir(&p) {
            Ok(rd) => rd,
            Err(err) => {
                eprintln!("error reading {}: {}", p.display(), err);
                return ExitCode::FAILURE;
            }
        };
        for f in sub_entries.flatten() {
            let fp = f.path();
            if fp.is_file() {
                let dstf = out_pkg.join(f.file_name());
                if std::fs::copy(&fp, &dstf).is_err() {
                    eprintln!("error copying {} → {}", fp.display(), dstf.display());
                    return ExitCode::FAILURE;
                }
            }
        }
        println!("vendored {}/", out_pkg.display());
    }
    // The nested module go.mod + lib.
    std::fs::create_dir_all(dst.join("lib")).ok();
    if std::fs::write(dst.join("go.mod"), "module hybrid.program\n\ngo 1.26\n").is_err() {
        eprintln!("error writing {}/go.mod", dst.display());
        return ExitCode::FAILURE;
    }
    if let Err(m) = install_staticlib(&lib, &dst.join("lib")) {
        eprintln!("error installing the staticlib: {}", m);
        return ExitCode::FAILURE;
    }
    println!(
        "vendored {} (staticlib: {})",
        dst.join("go.mod").display(),
        lib.display()
    );
    println!("next: in your go.mod —");
    println!("  require hybrid.program v0.0.0");
    println!("  replace hybrid.program => ./hybridrt");
    ExitCode::SUCCESS
}

/// BuildError rendering without pulling the whole CLI plumbing around.
fn render_build_error(e: &BuildError) -> String {
    match e {
        BuildError::Tool(m) => m.clone(),
        BuildError::Go(m) => format!("go build failed:\n{}", m),
    }
}

fn cmd_build(args: &[String]) -> ExitCode {
    const USAGE: &str = "usage: hyb build <file.hy> [-o <bin-path>] [--emit-go] [--deny-managed] | --package <name> --out-dir <dir> [--binding-prefix <path>]";
    let mut file: Option<String> = None;
    let mut out_path: Option<String> = None;
    let mut emit_go_only = false;
    let mut deny_managed = false;
    let mut pkg_name: Option<String> = None;
    let mut out_dir: Option<String> = None;
    let mut binding_prefix: Option<String> = None;
    let mut end_of_options = false;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if !end_of_options {
            match arg.as_str() {
                "--" => {
                    end_of_options = true;
                    continue;
                }
                "--emit-go" => {
                    emit_go_only = true;
                    continue;
                }
                "--deny-managed" => {
                    deny_managed = true;
                    continue;
                }
                "-o" | "--package" | "--out-dir" | "--binding-prefix" => {
                    let slot = match arg.as_str() {
                        "-o" => &mut out_path,
                        "--package" => &mut pkg_name,
                        "--out-dir" => &mut out_dir,
                        "--binding-prefix" => &mut binding_prefix,
                        _ => unreachable!(),
                    };
                    if slot.is_some() {
                        eprintln!("error: {arg} may only be supplied once\n{USAGE}");
                        return ExitCode::from(2);
                    }
                    let Some(val) = iter.next() else {
                        eprintln!("error: {arg} requires a value\n{USAGE}");
                        return ExitCode::from(2);
                    };
                    if val.is_empty() || val.starts_with('-') {
                        eprintln!("error: {arg} requires a value, not an option\n{USAGE}");
                        return ExitCode::from(2);
                    }
                    *slot = Some(val.clone());
                    continue;
                }
                _ if arg.starts_with('-') => {
                    eprintln!("error: unknown build option: {arg}\n{USAGE}");
                    return ExitCode::from(2);
                }
                _ => {}
            }
        }
        if !arg.ends_with(".hy") {
            eprintln!("error: build input must be a .hy file: {arg}\n{USAGE}");
            return ExitCode::from(2);
        }
        if file.is_some() {
            eprintln!("error: build expects exactly one .hy input file; unexpected argument: {arg}\n{USAGE}");
            return ExitCode::from(2);
        }
        file = Some(arg.clone());
    }
    let Some(file) = file else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    if pkg_name.is_some() != out_dir.is_some() {
        eprintln!("error: --package and --out-dir must be supplied together\n{USAGE}");
        return ExitCode::from(2);
    }
    let out_path = out_path.unwrap_or_else(|| "a.out".to_string());
    let binding_prefix = binding_prefix.unwrap_or_else(|| "hybrid.program".to_string());

    // --- 1. Toolchain pin enforcement (HYB-E9001) ---
    // The walking skeleton's reproducibility guard (DX study: ~30 lines).
    // HYB_ALLOW_TOOLCHAIN_DRIFT=1 is the documented escape hatch for
    // experimentation; CI and the Makefile never set it.
    if std::env::var("HYB_ALLOW_TOOLCHAIN_DRIFT").ok().as_deref() != Some("1") {
        if let Some(diag) = check_toolchain_pins(&file) {
            println!("{}", diag.render_head());
            for f in &diag.fixits {
                println!("  = help: {}", f.description);
            }
            return ExitCode::from(2);
        }
    }

    // --- 2. Parse (HYB-E0002 envelope on failure) ---
    let src = match std::fs::read_to_string(&file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error reading {}: {}", file, e);
            return ExitCode::from(2);
        }
    };
    let ast = match hy_syntax::parse_file(&file, &src) {
        Ok(ast) => ast,
        Err(diags) => {
            let summary = hy_diag::CheckSummary {
                file: file.clone(),
                verdict: "reject".into(),
                errors: diags.len(),
                warnings: 0,
                managed: None,
            };
            println!("{}", hy_diag::check_output_json(&diags, &summary));
            for d in &diags {
                println!("{}", d.render_head());
            }
            return ExitCode::FAILURE;
        }
    };

    // --- 3. Tier-0 check (verdict gates the build) ---
    let result = hy_check::check_file(&ast);
    let errors = result
        .diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .count();
    if errors > 0 {
        let summary = hy_diag::CheckSummary {
            file: file.clone(),
            verdict: result.verdict.as_str().to_string(),
            errors,
            warnings: result
                .diagnostics
                .iter()
                .filter(|d| d.severity == hy_diag::Severity::Warning)
                .count(),
            managed: None,
        };
        println!(
            "{}",
            hy_diag::check_output_json(&result.diagnostics, &summary)
        );
        for d in &result.diagnostics {
            println!("{}", d.render_head());
        }
        return ExitCode::FAILURE;
    }
    if deny_managed && result.managed.managed_blocks > 0 {
        eprintln!(
            "hyb build: deny-managed: {} managed block(s) present — build failed by policy (--deny-managed); \
             audit the block(s) or remove them (HYB-E8001 counts them under --report-managed)",
            result.managed.managed_blocks
        );
        return ExitCode::FAILURE;
    }

    // --- 4. Emit Go ---
    let go_src = if let Some(pn) = &pkg_name {
        match hy_emit::emit_go_package(&ast, pn, &binding_prefix) {
            Ok(g) => g,
            Err(e) => {
                eprintln!("error [emit]: {}", e.message);
                return ExitCode::from(2);
            }
        }
    } else {
        match hy_emit::emit_go(&ast) {
            Ok(g) => g,
            Err(e) => {
                eprintln!("error [emit]: {}", e.message);
                return ExitCode::from(2);
            }
        }
    };
    // --- 3b. gofmt pass (cosmetic, semantics-preserving by definition) ---
    // The emitter emits gofmt-canonical indentation and struct alignment
    // directly (the 2026-10-14 base-indent sweep); go/printer's remaining
    // normalizations are deep-expression SPACING heuristics not worth
    // re-implementing, so build pipes through the pinned toolchain's own
    // gofmt. Build requires the pinned go anyway (E9001), so gofmt is
    // present by definition. On any gofmt failure the unformatted source
    // proceeds unchanged (gofmt is cosmetic; go build is the real gate).
    let go_src = match run_gofmt(&go_src) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("notice: gofmt pass skipped ({})", e);
            go_src
        }
    };

    // Which rust bindings does the emitted Go call? (M6: known crates only;
    // the emitter errors cleanly on unknown ones, so this scan is exact.)
    let bindings = hy_emit::used_binding_roots(&go_src);

    // --- 4b. Package mode: assemble the package dir + runtime pieces. ---
    if let (Some(pn), Some(od)) = (&pkg_name, &out_dir) {
        return cmd_build_package(od, pn, &bindings, &go_src, &binding_prefix);
    }

    let go_path = format!("{}.go", out_path);
    if let Some(parent) = std::path::Path::new(&go_path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(&go_path, &go_src) {
        eprintln!("error writing {}: {}", go_path, e);
        return ExitCode::from(2);
    }
    println!("emitted {}", go_path);
    if emit_go_only {
        return ExitCode::SUCCESS;
    }

    // --- 5. go build ---
    // With rust bindings the program is a proper MODULE (cgo broker +
    // copied runtime packages + the hyrt staticlib): assemble the workspace,
    // cargo-build hyrt, then `go build ./...` inside it. Plain programs keep
    // the single-file build.
    let go_bin = go_bin();
    if bindings.is_empty() {
        let build = Command::new(go_bin)
            .args(["build", "-o", &out_path, &go_path])
            .env("GOFLAGS", "-p=2")
            .output();
        match build {
            Ok(out) if out.status.success() => {
                println!("built {} (go {})", out_path, PINNED_GO);
                ExitCode::SUCCESS
            }
            Ok(out) => {
                eprintln!("go build failed:");
                eprint!("{}", String::from_utf8_lossy(&out.stderr));
                ExitCode::FAILURE
            }
            Err(e) => {
                eprintln!("error running go build: {} (looked for {})", e, go_bin);
                ExitCode::from(2)
            }
        }
    } else {
        match build_with_bridge(go_bin, &out_path, &go_src, &bindings) {
            Ok(()) => {
                println!("built {} (go {}, bridge: hyrt)", out_path, PINNED_GO);
                ExitCode::SUCCESS
            }
            Err(BuildError::Tool(msg)) => {
                eprintln!("{}", msg);
                ExitCode::from(2)
            }
            Err(BuildError::Go(stderr)) => {
                eprintln!("go build failed:");
                eprint!("{}", stderr);
                ExitCode::FAILURE
            }
        }
    }
}

enum BuildError {
    Tool(String),
    Go(String),
}

/// Package mode (example 09): assemble an importable Go package directory:
///
/// ```text
/// <out-dir>/<name>.go      the emitted package source
/// <out-dir>/hybridge/      the boundary core (copied)
/// <out-dir>/<binding>/     per-crate bindings (copied)
/// <out-dir>/lib/libhyrt.a  the staticlib (built fresh or reused)
/// ```
/// The consumer's module imports `<module>/<name>`; --binding-prefix told
/// the emitter what import path the bindings carry, and the binding
/// packages' hybridge import is rewritten to the same prefix.
fn cmd_build_package(
    out_dir: &str,
    pkg_name: &str,
    bindings: &[&str],
    go_src: &str,
    binding_prefix: &str,
) -> ExitCode {
    let fail = |m: String| {
        eprintln!("{}", m);
        ExitCode::from(2)
    };
    let rt = match runtime_dir() {
        Ok(p) => p,
        Err(BuildError::Tool(m)) => return fail(m),
        Err(_) => return fail("runtime dir error".to_string()),
    };
    let od = std::path::Path::new(out_dir);
    if let Err(e) = std::fs::create_dir_all(od) {
        return fail(format!("creating {}: {}", od.display(), e));
    }
    if let Err(e) = std::fs::write(od.join(format!("{}.go", pkg_name)), go_src) {
        return fail(format!("writing package source: {}", e));
    }
    let copy_pkg = |name: &str| -> Result<(), String> {
        let src = rt.join("go").join(name);
        let dst = od.join(name);
        std::fs::create_dir_all(&dst).map_err(|e| format!("mkdir {}: {}", dst.display(), e))?;
        for entry in
            std::fs::read_dir(&src).map_err(|e| format!("reading {}: {}", src.display(), e))?
        {
            let entry = entry.map_err(|e| format!("readdir: {}", e))?;
            let p = entry.path();
            if p.extension().map(|x| x == "go").unwrap_or(false) {
                let mut text = std::fs::read_to_string(&p)
                    .map_err(|e| format!("reading {}: {}", p.display(), e))?;
                // The runtime copies' hybridge import follows the consumer's
                // module (the prefix the emitter already used).
                text = text.replace("hybrid.program/", &format!("{}/", binding_prefix));
                std::fs::write(dst.join(entry.file_name()), text)
                    .map_err(|e| format!("writing {}: {}", dst.display(), e))?;
            } else if p.is_file() {
                std::fs::copy(&p, dst.join(entry.file_name()))
                    .map_err(|e| format!("copying {}: {}", p.display(), e))?;
            }
        }
        Ok(())
    };
    if let Err(m) = copy_pkg("hybridge") {
        return fail(m);
    }
    for b in bindings {
        if let Err(m) = copy_pkg(b) {
            return fail(m);
        }
    }
    // The staticlib (fresh or reused) at <out-dir>/lib/ — the bindings'
    // #cgo LDFLAGS point at ${SRCDIR}/../lib.
    match ensure_staticlib(&rt) {
        Ok(lib) => {
            if let Err(m) = install_staticlib(&lib, &od.join("lib")) {
                return fail(m);
            }
        }
        Err(BuildError::Tool(m)) => return fail(m),
        Err(_) => return fail("staticlib build error".to_string()),
    }
    println!(
        "package {} ready at {} (bindings: {})",
        pkg_name,
        od.display(),
        if bindings.is_empty() {
            "none".to_string()
        } else {
            bindings.join(", ")
        }
    );
    ExitCode::SUCCESS
}

/// Build (or reuse) the hyrt staticlib; returns its path.
fn ensure_staticlib(rt: &std::path::Path) -> Result<std::path::PathBuf, BuildError> {
    let crate_dir = rt.join("rust/hyrt");
    // Candidate staticlibs — the freshest ARCH-MATCHING fresh one wins.
    // (Row 16 lesson: the bridge smoke on the arm64 fleet leg caught hyb
    // happily linking a foreign-arch libhyrt.a. Candidates cover the dev
    // shared-target flow (CARGO_TARGET_DIR), cross --target subdirs, and
    // the crate-local layout the shipped tarball re-lays the prebuilt lib
    // into (target/release/libhyrt.a — the one end users hit).)
    // Community benchmark report (v0.1.0, Windows): cargo with the msvc
    // toolchain names the artifact hyrt.lib, so BOTH spellings are probed.
    let mut cands = staticlib_candidates(&crate_dir);
    cands.sort();
    cands.dedup();
    let src_mtime = newest_mtime(&crate_dir);
    if std::env::var("HYB_DEBUG_LIB").is_ok() {
        eprintln!(
            "[hyb-debug] runtime={} crate_dir={} newest_src_mtime={:?}",
            rt.display(),
            crate_dir.display(),
            src_mtime
        );
    }
    let src_lib = cands
        .into_iter()
        .filter(|p| lib_arch_matches(p))
        .filter(|p| match (&src_mtime, std::fs::metadata(p)) {
            (Some(src_t), Ok(md)) => md.modified().ok().map(|l| l >= *src_t).unwrap_or(false),
            _ => false,
        })
        .max_by_key(|p| {
            std::fs::metadata(p)
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
        });
    if std::env::var("HYB_DEBUG_LIB").is_ok() {
        eprintln!("[hyb-debug] chosen staticlib: {:?}", src_lib.as_ref().map(|p| p.display().to_string()));
    }
    let src_lib = match src_lib {
        Some(p) => p,
        None => {
            // No fresh arch-matching lib — rebuild native (the dev flow; the
            // release tarball never lands here because packaging touches the
            // shipped lib's mtime past every source's).
            let mut cargo = Command::new(cargo_bin());
            cargo
                .args(["build", "--release", "-j2"])
                .current_dir(&crate_dir)
                .env("CARGO_BUILD_JOBS", "2");
            if let Ok(t) = std::env::var("CARGO_TARGET_DIR") {
                cargo.env("CARGO_TARGET_DIR", t);
            }
            let out = cargo
                .output()
                .map_err(|e| BuildError::Tool(format!("running cargo build (hyrt): {}", e)))?;
            if !out.status.success() {
                return Err(BuildError::Tool(format!(
                    "cargo build (hyrt) failed:\n{}{}",
                    String::from_utf8_lossy(&out.stdout),
                    String::from_utf8_lossy(&out.stderr)
                )));
            }
            // Re-probe: cargo names the artifact by toolchain flavor
            // (libhyrt.a under gnu/linux, hyrt.lib under msvc).
            let rebuilt = staticlib_candidates(&crate_dir)
                .into_iter()
                .filter(|p| lib_arch_matches(p))
                .max_by_key(|p| {
                    std::fs::metadata(p)
                        .and_then(|m| m.modified())
                        .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
                });
            match rebuilt {
                Some(p) => p,
                None => {
                    return Err(BuildError::Tool(
                        "no arch-matching hyrt staticlib found after cargo build. \
                         expected one of libhyrt.a (gnu flavor), hyrt.lib or libhyrt.lib (msvc) \
                         under <hyrt>/target/release. on windows the bridge ALSO needs \
                         mingw-w64 gcc for cgo; the release tarballs ship a prebuilt \
                         mingw-flavored libhyrt.a."
                            .to_string(),
                    ))
                }
            }
        }
    };
    Ok(src_lib)
}

/// Every file name the hyrt staticlib can take, per toolchain flavor:
/// gnu/linux cargo emits libhyrt.a; msvc cargo emits hyrt.lib (and some
/// setups libhyrt.lib). The community benchmark report (v0.1.0) hit hyb
/// hardcoding the .a name on Windows.
const STATICLIB_NAMES: &[&str] = &["libhyrt.a", "hyrt.lib", "libhyrt.lib"];

/// All candidate paths for a built staticlib under the hyrt crate dir:
/// <target>/release/<name> for every flavor name, plus cargo's per-triple
/// subdirs when the crate was built with --target.
fn staticlib_candidates(crate_dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    fn push_flat(d: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for n in STATICLIB_NAMES {
            out.push(d.join(n));
        }
    }
    let mut out: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(t) = std::env::var("CARGO_TARGET_DIR") {
        let t = std::path::PathBuf::from(t);
        push_flat(&t.join("release"), &mut out);
        push_sibling_libs(&t, &mut out);
    }
    let local_target = crate_dir.join("target");
    push_flat(&local_target.join("release"), &mut out);
    push_sibling_libs(&local_target, &mut out);
    out
}

/// Push <dir>/<subdir>/release/<name> for every subdir (cargo's per-triple
/// layout: target/<triple>/release/... when the crate was built with --target)
/// and every flavor name.
fn push_sibling_libs(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            if e.path().is_dir() && e.file_name() != "release" {
                for n in STATICLIB_NAMES {
                    out.push(e.path().join("release").join(n));
                }
            }
        }
    }
}

/// Check the first object in a static archive (or a raw ELF/Mach-O object)
/// against the host architecture. GNU/COFF ar names and Darwin's BSD extended
/// names are supported; index members are skipped without scanning their data.
/// Every read stays inside its member so a truncated object cannot borrow bytes
/// from the following header. Unknown formats and architectures fail closed.
fn lib_arch_matches(p: &std::path::Path) -> bool {
    use std::io::{Read, Seek, SeekFrom};
    fn object_matches(mut r: impl Read, allow_coff: bool) -> bool {
        let mut hdr = [0u8; 20];
        if r.read_exact(&mut hdr).is_err() {
            return false;
        }
        if &hdr[..4] == b"\x7fELF" {
            let machine = u16::from_le_bytes([hdr[18], hdr[19]]);
            return matches!(
                (std::env::consts::ARCH, machine),
                ("x86_64", 62) | ("aarch64", 183)
            );
        }
        let big_endian = match &hdr[..4] {
            b"\xcf\xfa\xed\xfe" => false, // MH_MAGIC_64
            b"\xfe\xed\xfa\xcf" => true,  // byte-swapped MH_MAGIC_64
            _ => return allow_coff && coff_machine_matches(u16::from_le_bytes([hdr[0], hdr[1]])),
        };
        // A mach_header_64 is 32 bytes, not the 20-byte ELF/COFF probe.
        if r.read_exact(&mut [0u8; 12]).is_err() {
            return false;
        }
        let word = |offset| {
            let bytes: [u8; 4] = hdr[offset..offset + 4].try_into().unwrap();
            if big_endian {
                u32::from_be_bytes(bytes)
            } else {
                u32::from_le_bytes(bytes)
            }
        };
        word(12) == 1 // MH_OBJECT, not an executable/dylib
            && matches!((std::env::consts::ARCH, word(4)),
                ("x86_64", 0x01000007) | ("aarch64", 0x0100000c))
    }
    let Ok(mut f) = std::fs::File::open(p) else {
        return false;
    };
    let Ok(file_len) = f.metadata().map(|m| m.len()) else {
        return false;
    };
    let mut magic = [0u8; 8];
    if f.read_exact(&mut magic).is_err() {
        return false;
    }
    if &magic != b"!<arch>\n" {
        return f.seek(SeekFrom::Start(0)).is_ok() && object_matches(&mut f, false);
    }
    loop {
        let mut m = [0u8; 60];
        if f.read_exact(&mut m).is_err() || &m[58..60] != b"`\n" {
            return false;
        }
        let Ok(name) = std::str::from_utf8(&m[..16]) else {
            return false;
        };
        let name = name.trim_end();
        let Some(size) = std::str::from_utf8(&m[48..58])
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
        else {
            return false;
        };
        let Ok(start) = f.stream_position() else {
            return false;
        };
        let Some(end) = start
            .checked_add(size)
            .and_then(|n| n.checked_add(size & 1))
        else {
            return false;
        };
        if end > file_len {
            return false;
        }
        let mut payload_size = size;
        let mut is_index = name == "/" || name == "//" || name.starts_with("__.SYMDEF");
        if let Some(length) = name.strip_prefix("#1/") {
            // BSD: the size includes a possibly NUL-padded filename prefix.
            // Read only enough of it to identify an index; never allocate an
            // untrusted length or inspect the name as though it were an object.
            let Ok(length) = length.parse::<u64>() else {
                return false;
            };
            if length == 0 || length > size {
                return false;
            }
            let mut prefix = [0u8; 16];
            let n = length.min(prefix.len() as u64) as usize;
            if f.read_exact(&mut prefix[..n]).is_err() {
                return false;
            }
            is_index = prefix[..n].starts_with(b"__.SYMDEF");
            if f.seek(SeekFrom::Start(start + length)).is_err() {
                return false;
            }
            payload_size -= length;
        }
        if !is_index {
            return object_matches((&mut f).take(payload_size), true);
        }
        if f.seek(SeekFrom::Start(end)).is_err() {
            return false;
        }
    }
}

/// COFF machine-type counterpart of the ELF/Mach-O host-architecture probe.
fn coff_machine_matches(m: u16) -> bool {
    match std::env::consts::ARCH {
        "x86_64" => m == 0x8664,
        "aarch64" => m == 0xAA64,
        // Never guess the ABI for an unsupported host architecture.
        _ => false,
    }
}

/// Extract the Go `package <name>` identifier from a `.go` source file,
/// skipping leading whitespace, UTF-8 BOM, `//` line comments, and `/* ... */`
/// block comments.
pub(crate) fn extract_go_package_name(src: &str) -> Option<String> {
    fn skip_ws_and_comments(mut input: &str) -> Option<&str> {
        loop {
            input = input.trim_start();
            if let Some(rest) = input.strip_prefix("//") {
                input = match rest.find('\n') {
                    Some(idx) => &rest[idx + 1..],
                    None => "",
                };
                continue;
            }
            if let Some(rest) = input.strip_prefix("/*") {
                let end = rest.find("*/")?;
                input = &rest[end + 2..];
                continue;
            }
            return Some(input);
        }
    }
    let mut s = src.strip_prefix('\u{feff}').unwrap_or(src);
    s = skip_ws_and_comments(s)?;
    let after_kw = s.strip_prefix("package")?;
    let first_after = after_kw.chars().next()?;
    if !first_after.is_whitespace() && !after_kw.starts_with("/*") {
        return None;
    }
    let after_sep = skip_ws_and_comments(after_kw)?;
    let mut chars = after_sep.char_indices();
    let (_, first) = chars.next()?;
    if !(first == '_' || first.is_alphabetic()) {
        return None;
    }
    let mut end = first.len_utf8();
    for (idx, ch) in chars {
        if ch == '_' || ch.is_alphanumeric() {
            end = idx + ch.len_utf8();
        } else {
            break;
        }
    }
    Some(after_sep[..end].to_string())
}

/// Lay the chosen staticlib into a target lib/ directory under the name the
/// cgo contract links (`-lhyrt` → libhyrt.a) — and, when the source is an
/// msvc .lib, ALSO under the .lib spellings so MS-style consumers and any
/// mingw ld probing the no-prefix name find it. Content decides format; the
/// aliases are the same bytes.
pub(crate) fn install_staticlib(
    src: &std::path::Path,
    libdir: &std::path::Path,
) -> Result<(), String> {
    std::fs::create_dir_all(libdir)
        .map_err(|e| format!("creating {}: {}", libdir.display(), e))?;
    std::fs::copy(src, libdir.join("libhyrt.a"))
        .map_err(|e| format!("copying staticlib: {}", e))?;
    if src.extension().and_then(|e| e.to_str()) == Some("lib") {
        for alias in ["hyrt.lib", "libhyrt.lib"] {
            std::fs::copy(src, libdir.join(alias))
                .map_err(|e| format!("copying staticlib alias {}: {}", alias, e))?;
        }
    }
    Ok(())
}

/// The runtime directory: HYB_RUNTIME_DIR override, then the TARBALL layout
/// (runtime/ next to the running hyb — a release artifact is self-contained),
/// else the repo-relative dev default (the compiler crate sits at
/// <repo>/compiler/crates/hyb).
///
/// Order matters (row 16 lesson): the CARGO_MANIFEST_DIR fallback points at
/// the BUILD machine's repo — for a shipped binary it usually does not exist
/// (today's users need the install.sh launcher for exactly this reason), and
/// when it does exist (fleet smokes run on the checkout that built hyb) it
/// can hold a FOREIGN-ARCH staticlib. Resolving the executable's own
/// directory first makes the tarball the source of truth for its own binary.
fn runtime_dir() -> Result<std::path::PathBuf, BuildError> {
    if let Ok(d) = std::env::var("HYB_RUNTIME_DIR") {
        let p = std::path::PathBuf::from(&d);
        if p.is_dir() {
            return Ok(p);
        }
        return Err(BuildError::Tool(format!(
            "HYB_RUNTIME_DIR={} is not a directory",
            d
        )));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join("runtime");
            if p.is_dir() {
                return Ok(p);
            }
        }
    }
    let manifest = env!("CARGO_MANIFEST_DIR");
    // <repo>/compiler/crates/hyb → <repo>/runtime (three ups).
    let p = std::path::Path::new(manifest)
        .join("../../../runtime")
        .canonicalize()
        .map_err(|_| {
            BuildError::Tool(
                "runtime/ not found — set HYB_RUNTIME_DIR to the repo's runtime directory".into(),
            )
        })?;
    Ok(p)
}

/// Assemble the build workspace and build with the bridge:
///
/// ```text
/// <ws>/go.mod            module hybrid.program (go 1.26)
/// <ws>/main.go           the emitted Go
/// <ws>/hybridge/         runtime/go/hybridge (copied verbatim)
/// <ws>/<binding>/        runtime/go/<binding> (copied verbatim)
/// <ws>/lib/libhyrt.a     cargo-built staticlib
/// ```
/// The binding's #cgo LDFLAGS point at ../lib (workspace-relative), so the
/// layout is part of the contract. gofmt-style tabs; CGO_ENABLED=1.
fn build_with_bridge(
    go_bin: &str,
    out_path: &str,
    go_src: &str,
    bindings: &[&str],
) -> Result<(), BuildError> {
    let rt = runtime_dir()?;
    // The workspace build runs with cwd = <ws>; a RELATIVE -o would land
    // inside the workspace and vanish with it. Anchor to the caller's cwd.
    let out_abs = if std::path::Path::new(out_path).is_absolute() {
        std::path::PathBuf::from(out_path)
    } else {
        std::env::current_dir()
            .map_err(|e| BuildError::Tool(format!("resolving cwd: {}", e)))?
            .join(out_path)
    };
    let out_path = out_abs.to_string_lossy().to_string();
    let ws = std::env::temp_dir().join(format!(
        "hyb-ws-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(ws.join("lib"))
        .map_err(|e| BuildError::Tool(format!("creating workspace {}: {}", ws.display(), e)))?;

    // 1. The hyrt staticlib — via ensure_staticlib (fresh + ARCH-MATCHED).
    //    (Row 16 lesson: this path used to duplicate the logic naively — it
    //    cargo-rebuilt NATIVELY on every build (the ws lib never pre-exists)
    //    and copied whatever cargo produced: a foreign-arch lib under cross
    //    builds, found via cargo_bin()'s old ~/.cargo fallback even in the
    //    rustless smoke. The bridge smoke on the arm64 fleet leg caught it.)
    let src_lib = ensure_staticlib(&rt)?;
    install_staticlib(&src_lib, &ws.join("lib")).map_err(BuildError::Tool)?;

    // 2. Workspace assembly.
    std::fs::write(ws.join("go.mod"), "module hybrid.program\n\ngo 1.26\n")
        .map_err(|e| BuildError::Tool(format!("writing go.mod: {}", e)))?;
    std::fs::write(ws.join("main.go"), go_src)
        .map_err(|e| BuildError::Tool(format!("writing main.go: {}", e)))?;
    let copy_pkg = |name: &str| -> Result<(), BuildError> {
        let src = rt.join("go").join(name);
        let dst = ws.join(name);
        std::fs::create_dir_all(&dst)
            .map_err(|e| BuildError::Tool(format!("mkdir {}: {}", dst.display(), e)))?;
        for entry in std::fs::read_dir(&src)
            .map_err(|e| BuildError::Tool(format!("reading {}: {}", src.display(), e)))?
        {
            let entry = entry.map_err(|e| BuildError::Tool(format!("readdir: {}", e)))?;
            let p = entry.path();
            if p.is_file() {
                std::fs::copy(&p, dst.join(entry.file_name()))
                    .map_err(|e| BuildError::Tool(format!("copying {}: {}", p.display(), e)))?;
            }
        }
        Ok(())
    };
    copy_pkg("hybridge")?;
    for b in bindings {
        copy_pkg(b)?;
    }

    // 3. go build inside the module.
    let mut gb = Command::new(go_bin);
    gb.args(["build", "-o", &out_path, "."])
        .current_dir(&ws)
        .env("GOFLAGS", "-p=2")
        .env("CGO_ENABLED", "1");
    let out = gb
        .output()
        .map_err(|e| BuildError::Tool(format!("running go build (workspace): {}", e)))?;
    if out.status.success() {
        let _ = std::fs::remove_dir_all(&ws);
        Ok(())
    } else {
        // Keep the workspace for diagnosis; say where.
        eprintln!("(workspace kept at {} for diagnosis)", ws.display());
        Err(BuildError::Go(
            String::from_utf8_lossy(&out.stderr).to_string(),
        ))
    }
}

fn newest_mtime(dir: &std::path::Path) -> Option<std::time::SystemTime> {
    let mut newest = None;
    let stack = vec![dir.to_path_buf()];
    let mut todo = stack;
    while let Some(d) = todo.pop() {
        for entry in std::fs::read_dir(&d).ok()? {
            let p = entry.ok()?.path();
            if p.is_dir() {
                if p.file_name().map(|n| n == "target").unwrap_or(false) {
                    continue; // build output, not source
                }
                todo.push(p);
            } else {
                if let Ok(m) = p.metadata().and_then(|m| m.modified()) {
                    if newest.map(|n| m > n).unwrap_or(true) {
                        newest = Some(m);
                    }
                }
            }
        }
    }
    newest
}

/// Compare installed toolchains against the pins. Returns the E9001
/// diagnostic on the FIRST mismatch (go first — it gates codegen).
fn check_toolchain_pins(file: &str) -> Option<Diagnostic> {
    let span = Span::point(Pos::new(1, 1));

    let go_out = Command::new(go_bin())
        .arg("version")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    match go_out {
        None => {
            return Some(Diagnostic::error(
                "HYB-E9001",
                file,
                span,
                format!("go not found — pinned {} required", PINNED_GO),
            ));
        }
        Some(v) => {
            let found = v.split_whitespace().nth(2).unwrap_or("").to_string();
            if found != PINNED_GO {
                return Some(Diagnostic::error(
                    "HYB-E9001",
                    file,
                    span,
                    format!("toolchain mismatch: found {}, pinned {}", found, PINNED_GO),
                ));
            }
        }
    }

    let rust_out = Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    if let Some(v) = rust_out {
        // Empty/unparseable output is treated as ABSENT: the rust pin
        // governs the toolchain WHEN IT IS USED (bridge builds via
        // ensure_staticlib); a bridge-free build — the arm64 release smoke,
        // where rustc is legitimately not on PATH at all — must not die on
        // a rustc that exists but prints nothing. Absent (ENOENT) already
        // skips; this closes the same hole for the found-but-silent case.
        // Found during the arm64 release-matrix smoke (row 11/16).
        let found = v.split_whitespace().nth(1).unwrap_or("").to_string();
        if !found.is_empty() && found != PINNED_RUST {
            return Some(Diagnostic::error(
                "HYB-E9001",
                file,
                span,
                format!(
                    "toolchain mismatch: found rustc {}, pinned {}",
                    found, PINNED_RUST
                ),
            ));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// bench-overhead — the G2 gate's call-overhead evidence (Batch J)
// ---------------------------------------------------------------------------

/// The bench program assembled into the bridge workspace. It measures TWO
/// crossings on the LIVE bridge (the same hybridge assembly + staticlib
/// every `hyb build` links):
///
///   crossing-nop      hybridge.Cross1 → hyrt's hy_nop — the shipping
///                     fast-path trampoline with ZERO payload work. This is
///                     the quantity SPEC §6's ≤5 ns budget names.
///   binding-newobject jsonbinding.NewObject — a REAL binding call (handle
///                     allocation on the Rust side) for context.
///
/// Machine-parseable lines: `<name> <float> ns/call`. Best of 3 in-process.
const BENCH_MAIN: &str = r#"package main

import (
        "fmt"
        "runtime"
        "time"

        "hybrid.program/hybench"
        "hybrid.program/jsonbinding"
        "hybrid.program/overheadbench"
)

var sink uint64

//go:noinline
func goNop(x uint64) uint64 { return x }

// measure: one best-of-3 inline loop (the spike-1 shape — no closure
// indirection), run at the CURRENT stack depth. Modes: 0 = Go floor,
// 2 = the shipping guarded crossing, 3 = the raw fast trampoline.
func measure(n int, mode int) float64 {
        best := 0.0
        for rep := 0; rep < 3; rep++ {
                t0 := time.Now()
                switch mode {
                case 0:
                        for i := 0; i < n; i++ {
                                sink += goNop(uint64(i))
                        }
                case 2:
                        for i := 0; i < n; i++ {
                                sink += overheadbench.Nop(uint64(i))
                        }
                case 3:
                        for i := 0; i < n; i++ {
                                v, ok := overheadbench.NopRaw(uint64(i))
                                if !ok {
                                        panic("fast path short headroom in scenario B")
                                }
                                sink += v
                        }
                }
                d := float64(time.Since(t0).Nanoseconds()) / float64(n)
                if best == 0 || d < best {
                        best = d
                }
        }
        return best
}

// descend recurses to force contiguous stack growth (8 KB doubles on each
// morestack; 200 frames force several doublings), then measures at THAT
// depth — the grown-stack scenario where headroom exceeds the fast path's
// 4096-byte threshold.
func descend(n int, mode int, depth int) float64 {
        if depth > 0 {
                return descend(n, mode, depth-1)
        }
        return measure(n, mode)
}

// measurePct: the row-18 percentile measurement (PLAN row 18: the report's
// ms-clock harnesses were too coarse; the mean hides tails). Timer honesty
// at ns scale: one time.Now() per WINDOW, not per call — each sample is a
// WINDOW MEAN (window/K ns), the quantity a fixed-rate consumer sees. 100
// windows of K calls each; the HDR-style histogram carries p50/p90/p99.
func measurePct(mode int, windows int, per int) *hybench.Histogram {
        h := hybench.New()
        for w := 0; w < windows; w++ {
                t0 := time.Now()
                switch mode {
                case 0:
                        for i := 0; i < per; i++ {
                                sink += goNop(uint64(i))
                        }
                case 2:
                        for i := 0; i < per; i++ {
                                sink += overheadbench.Nop(uint64(i))
                        }
                case 3:
                        for i := 0; i < per; i++ {
                                v, ok := overheadbench.NopRaw(uint64(i))
                                if !ok {
                                        panic("fast path short headroom in pct scenario B")
                                }
                                sink += v
                        }
                }
                h.Record(time.Since(t0).Nanoseconds() / int64(per))
        }
        return h
}

// pctDeep is measurePct at the grown-stack depth.
func pctDeep(mode int, windows int, per int, depth int) *hybench.Histogram {
        if depth > 0 {
                return pctDeep(mode, windows, per, depth-1)
        }
        return measurePct(mode, windows, per)
}

func main() {
        // Warmup: trampolines, the deep-stack pool, allocator pages.
        for i := 0; i < 100000; i++ {
                sink += overheadbench.Nop(uint64(i))
        }
        const N = 5000000

        // Scenario A — SHALLOW goroutine (fresh main stack): the headroom probe
        // found ~2.9 KB here, BELOW the fast path's 4096-byte threshold, so the
        // shipping crossing engages the pooled deep path.
        aFloor := measure(N, 0)
        aNop := measure(N, 2)
        aDeep := overheadbench.DeepPathCount()
        fmt.Printf("a-floor %.2f ns/call\n", aFloor)
        fmt.Printf("a-nop %.2f ns/call\n", aNop)
        fmt.Printf("a-deepcount %d\n", aDeep)

        // Scenario B — GROWN stack: the same crossing after forced growth. The
        // fast path must engage (deepcount frozen) — the budget's steady state.
        bFloor := descend(N, 0, 200)
        bRaw := descend(N, 3, 200)
        bNop := descend(N, 2, 200)
        bDeep := overheadbench.DeepPathCount()
        fmt.Printf("b-floor %.2f ns/call\n", bFloor)
        fmt.Printf("b-raw %.2f ns/call\n", bRaw)
        fmt.Printf("b-nop %.2f ns/call\n", bNop)
        fmt.Printf("b-deepcount %d\n", bDeep)
        fmt.Printf("b-delta %.2f ns/call\n", bNop-bFloor)

        // Context: a REAL binding call — jsonbinding.NewObject allocates a
        // Rust-side handle through the shipping crossing. At ~µs scale a
        // PER-CALL timer is honest (~25 ns timer on ~900 ns call, <3%), so
        // the percentiles here are true per-call tails.
        for i := 0; i < 1000; i++ {
                _ = jsonbinding.NewObject()
        }
        const M = 200000
        best := 0.0
        for rep := 0; rep < 3; rep++ {
                runtime.GC()
                t1 := time.Now()
                for i := 0; i < M; i++ {
                        v := jsonbinding.NewObject()
                        _ = v
                }
                d := float64(time.Since(t1).Nanoseconds()) / float64(M)
                if best == 0 || d < best {
                        best = d
                }
        }
        runtime.GC()
        fmt.Printf("binding-newobject %.2f ns/call\n", best)

        // Row 18: percentile tails. Fast paths get WINDOWED percentiles
        // (100 windows × 10k calls — per-call timers would dominate a 3 ns
        // crossing); the binding call gets true per-call percentiles.
        const PW = 100
        const PK = 10000
        hAFloor := measurePct(0, PW, PK)
        hANop := measurePct(2, PW, PK)
        hBFloor := pctDeep(0, PW, PK, 200)
        hBNop := pctDeep(2, PW, PK, 200)
        hBRaw := pctDeep(3, PW, PK, 200)
        hObj := hybench.New()
        for i := 0; i < 20000; i++ {
                t2 := time.Now()
                v := jsonbinding.NewObject()
                _ = v
                hObj.Record(time.Since(t2).Nanoseconds())
        }
        fmt.Printf("a-floor-p50 %d\na-floor-p99 %d\n", hAFloor.P50(), hAFloor.P99())
        fmt.Printf("a-nop-p50 %d\na-nop-p99 %d\n", hANop.P50(), hANop.P99())
        fmt.Printf("b-floor-p50 %d\nb-floor-p99 %d\n", hBFloor.P50(), hBFloor.P99())
        fmt.Printf("b-raw-p50 %d\nb-raw-p99 %d\n", hBRaw.P50(), hBRaw.P99())
        fmt.Printf("b-nop-p50 %d\nb-nop-p99 %d\n", hBNop.P50(), hBNop.P99())
        fmt.Printf("binding-newobject-p50 %d\nbinding-newobject-p99 %d\n", hObj.P50(), hObj.P99())
        fmt.Printf("sink %d\n", sink)
}
"#;

/// One bench process run's parsed numbers (median-of-3 reported).
#[derive(Default, Clone, Copy)]
struct BenchRun {
    a_floor: f64,
    a_nop: f64,
    a_deep: u64,
    b_floor: f64,
    b_raw: f64,
    b_nop: f64,
    b_deep: u64,
    b_delta: f64,
    obj: f64,
    // Row 18: percentile tails (windowed means for the fast paths,
    // per-call for the binding). Zero = absent (parse failure keeps 0
    // and the report line is skipped).
    a_floor_p50: i64,
    a_floor_p99: i64,
    a_nop_p50: i64,
    a_nop_p99: i64,
    b_floor_p50: i64,
    b_floor_p99: i64,
    b_raw_p50: i64,
    b_raw_p99: i64,
    b_nop_p50: i64,
    b_nop_p99: i64,
    obj_p50: i64,
    obj_p99: i64,
}

fn cmd_bench_overhead() -> ExitCode {
    // Same pins discipline as build (HYB-E9001): the bench measures the
    // pinned toolchains, nothing else.
    if let Some(diag) = check_toolchain_pins("bench-overhead") {
        eprintln!("{}", diag.render_head());
        for n in &diag.notes {
            eprintln!("  = {}", n);
        }
        return ExitCode::FAILURE;
    }
    println!(
        "assembling the bench on the live bridge (pins: {} / rust {})...",
        PINNED_GO, PINNED_RUST
    );
    let go_bin = go_bin();
    let out = std::env::temp_dir()
        .join(format!(
            "hyb-bench-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0)
        ))
        .to_string_lossy()
        .to_string();
    if let Err(e) = build_with_bridge(
        go_bin,
        &out,
        BENCH_MAIN,
        &["overheadbench", "jsonbinding", "hybench"],
    ) {
        match e {
            BuildError::Tool(m) => eprintln!("error: {}", m),
            BuildError::Go(stderr) => eprintln!("error: go build failed:\n{}", stderr),
        }
        return ExitCode::FAILURE;
    }

    // Run 3× (separate processes; report the median run's numbers — wall
    // clock is supporting evidence per SPEC §3.4, and the number the gate
    // cites is the process-median of the in-process best-of-3).
    let mut runs: Vec<BenchRun> = Vec::new();
    for _ in 0..3 {
        let proc_out = Command::new(&out).output();
        let out = match proc_out {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
            Ok(o) => {
                eprintln!(
                    "bench run failed:\n{}{}",
                    String::from_utf8_lossy(&o.stdout),
                    String::from_utf8_lossy(&o.stderr)
                );
                return ExitCode::FAILURE;
            }
            Err(e) => {
                eprintln!("error running the bench: {}", e);
                return ExitCode::FAILURE;
            }
        };
        let mut r = BenchRun::default();
        for line in out.lines() {
            let mut it = line.split_whitespace();
            match (it.next(), it.next()) {
                (Some("a-floor"), Some(v)) => r.a_floor = v.parse().unwrap_or(0.0),
                (Some("a-nop"), Some(v)) => r.a_nop = v.parse().unwrap_or(0.0),
                (Some("a-deepcount"), Some(v)) => r.a_deep = v.parse().unwrap_or(0),
                (Some("b-floor"), Some(v)) => r.b_floor = v.parse().unwrap_or(0.0),
                (Some("b-raw"), Some(v)) => r.b_raw = v.parse().unwrap_or(0.0),
                (Some("b-nop"), Some(v)) => r.b_nop = v.parse().unwrap_or(0.0),
                (Some("b-deepcount"), Some(v)) => r.b_deep = v.parse().unwrap_or(0),
                (Some("b-delta"), Some(v)) => r.b_delta = v.parse().unwrap_or(0.0),
                (Some("binding-newobject"), Some(v)) => r.obj = v.parse().unwrap_or(0.0),
                (Some("a-floor-p50"), Some(v)) => r.a_floor_p50 = v.parse().unwrap_or(0),
                (Some("a-floor-p99"), Some(v)) => r.a_floor_p99 = v.parse().unwrap_or(0),
                (Some("a-nop-p50"), Some(v)) => r.a_nop_p50 = v.parse().unwrap_or(0),
                (Some("a-nop-p99"), Some(v)) => r.a_nop_p99 = v.parse().unwrap_or(0),
                (Some("b-floor-p50"), Some(v)) => r.b_floor_p50 = v.parse().unwrap_or(0),
                (Some("b-floor-p99"), Some(v)) => r.b_floor_p99 = v.parse().unwrap_or(0),
                (Some("b-raw-p50"), Some(v)) => r.b_raw_p50 = v.parse().unwrap_or(0),
                (Some("b-raw-p99"), Some(v)) => r.b_raw_p99 = v.parse().unwrap_or(0),
                (Some("b-nop-p50"), Some(v)) => r.b_nop_p50 = v.parse().unwrap_or(0),
                (Some("b-nop-p99"), Some(v)) => r.b_nop_p99 = v.parse().unwrap_or(0),
                (Some("binding-newobject-p50"), Some(v)) => r.obj_p50 = v.parse().unwrap_or(0),
                (Some("binding-newobject-p99"), Some(v)) => r.obj_p99 = v.parse().unwrap_or(0),
                _ => {}
            }
        }
        runs.push(r);
    }
    runs.sort_by(|a, b| a.b_nop.partial_cmp(&b.b_nop).unwrap());
    let r = runs[runs.len() / 2];
    let budget = 5.0f64;
    println!();
    println!("call overhead — the live bridge (hybridge fast path + hyrt staticlib):");
    println!("  scenario A — SHALLOW goroutine (fresh 8 KB main stack, ~2.9 KB headroom):");
    println!("    go-call floor     {:>7.2} ns/call", r.a_floor);
    println!("    shipping crossing {:>7.2} ns/call  (deep path: {} crossings — the pooled guarded stack)", r.a_nop, r.a_deep);
    println!("  scenario B — GROWN stack (headroom > 4096 B: the fast path's threshold):");
    println!("    go-call floor     {:>7.2} ns/call", r.b_floor);
    println!(
        "    raw fast tramp    {:>7.2} ns/call  (in-trampoline guard, no Go-wrapper retry)",
        r.b_raw
    );
    println!("    shipping crossing {:>7.2} ns/call  (Cross1; deepcount frozen at {} — fast path proven)", r.b_nop, r.b_deep);
    println!(
        "    marginal delta    {:>7.2} ns/call  (the boundary's cost over a Go call)",
        r.b_delta
    );
    println!("  binding-newobject   {:>7.2} ns/call  (context: real binding call, Rust-side handle alloc)", r.obj);
    println!("  budget              {:>7.2} ns/call  (SPEC §6 hypothesis; spike-1: 2.82 ns sandbox / 4.36 ns worker on the FIXED-stack prototype)", budget);
    println!();
    println!("  percentile tails (row 18 — ns resolution; fast paths are windowed means: 100 windows × 10k calls, per-call timers would dominate a 3 ns crossing; the binding is per-call):", );
    let pct = |name: &str, p50: i64, p99: i64| {
        if p50 > 0 {
            println!("    {:<19} p50 {:>6} ns   p99 {:>6} ns", name, p50, p99);
        }
    };
    pct("a-floor", r.a_floor_p50, r.a_floor_p99);
    pct("a-nop", r.a_nop_p50, r.a_nop_p99);
    pct("b-floor", r.b_floor_p50, r.b_floor_p99);
    pct("b-raw", r.b_raw_p50, r.b_raw_p99);
    pct("b-nop", r.b_nop_p50, r.b_nop_p99);
    pct("binding-newobject", r.obj_p50, r.obj_p99);
    println!();
    println!("  reading: the ≤5 ns hypothesis holds on the FAST path (grown stacks) —");
    if r.b_nop <= budget {
        println!(
            "  VERDICT: PASS — fast path {:.2} ns ≤ {:.2} ns ({:.0}% of budget). Shallow goroutines take the pooled deep path at {:.2} ns until their stacks grow (the collision-safe pool costs ~{:.0} ns over the spike's fixed-stack prototype).",
            r.b_nop, budget, 100.0 * r.b_nop / budget, r.a_nop, r.a_nop - r.b_nop
        );
        ExitCode::SUCCESS
    } else {
        println!(
            "  VERDICT: MISS — fast path {:.2} ns > {:.2} ns (see gates/G2.md for the interpretation + the documented fallback)",
            r.b_nop, budget
        );
        ExitCode::FAILURE
    }
}

#[cfg(test)]
mod staticlib_tests;

#[cfg(test)]
mod tests {
    use super::*;

    /// lib_arch_matches must accept a matching-arch ELF and reject a
    /// foreign-arch one, garbage, and short files (row 16: the bridge smoke
    /// caught hyb linking a foreign-arch libhyrt.a — the matcher is the
    /// regression guard).
    #[test]
    fn lib_arch_matches_rejects_foreign_elf() {
        let dir = std::env::temp_dir().join(format!("hyb-archtest-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // Minimal 20-byte ELF headers: magic + e_machine (LE u16 at 0x12).
        let mut x86 = vec![0x7fu8, b'E', b'L', b'F'];
        x86.extend(std::iter::repeat_n(0u8, 14));
        x86.extend_from_slice(&62u16.to_le_bytes()); // EM_X86_64
        let mut arm = x86.clone();
        arm[18] = 183u8; // EM_AARCH64
        let mut arm_hi = arm.clone();
        arm_hi[19] = 0u8; // wrong high byte -> machine 183 != 0x00b7? 183 = 0xb7, so hi must be 0; arm above already has 0? no: to_le_bytes set [18]=0xb7,[19]=0 — fix arm_hi to a WRONG machine (999)
        arm_hi[18] = 0xe7u8;
        arm_hi[19] = 0x03u8; // e_machine = 999 (unknown arch) — host arch never matches
        let garbage: Vec<u8> = vec![0xde, 0xad, 0xbe, 0xef, 0, 0, 0, 0, 0, 0];
        for (name, bytes, want_x86, want_arm) in [
            ("x86.a", &x86, true, false),
            ("arm.a", &arm, false, true),
            ("junk.a", &arm_hi, false, false),
            ("garbage.a", &garbage, false, false),
        ] {
            let p = dir.join(name);
            std::fs::write(&p, bytes).unwrap();
            let got = lib_arch_matches(&p);
            let expected = if std::env::consts::ARCH == "x86_64" { want_x86 } else { want_arm };
            assert_eq!(got, expected, "case {} (host {})", name, std::env::consts::ARCH);
        }
        // ar-archive shape: real libhyrt.a starts with "!<arch>" and carries
        // a big symbol-index member ("/") first — the walker must skip it and
        // decide on the first DATA member (row 16 regression).
        let member = |name: &[u8], size: usize, data: &[u8]| -> Vec<u8> {
            // ar pads every field with SPACES (a NUL-padded name is not
            // parseable — trim_end only strips whitespace, matching real ar)
            let mut h = vec![b' '; 16];
            h[..name.len()].copy_from_slice(name);
            h.extend_from_slice(b"0           "); // mtime
            h.extend_from_slice(b"0     "); // uid
            h.extend_from_slice(b"0     "); // gid
            h.extend_from_slice(b"0       "); // mode
            h.extend_from_slice(format!("{:010}", size).as_bytes());
            h.extend_from_slice(b"`\n");
            let mut v = h;
            v.extend_from_slice(data);
            if size % 2 == 1 {
                v.push(b'\n'); // ar pads data to even offsets
            }
            v
        };
        let mut ar_x86 = b"!<arch>\n".to_vec();
        ar_x86.extend(member(b"/", 312132, &vec![0u8; 312132])); // symbol index (skipped)
        ar_x86.extend(member(b"x86.o/", x86.len(), &x86)); // first data member
        let p = dir.join("ar-x86.a");
        std::fs::write(&p, &ar_x86).unwrap();
        assert_eq!(
            lib_arch_matches(&p),
            std::env::consts::ARCH == "x86_64",
            "ar-wrapped x86 member (behind a symbol index)"
        );
        // ar archive whose first data member is foreign-arch must be rejected
        let mut ar_arm = b"!<arch>\n".to_vec();
        ar_arm.extend(member(b"arm.o/", arm.len(), &arm));
        let p = dir.join("ar-arm.a");
        std::fs::write(&p, &ar_arm).unwrap();
        assert_eq!(
            lib_arch_matches(&p),
            std::env::consts::ARCH == "aarch64",
            "ar-wrapped arm member"
        );
        // short file (< 20 bytes) must be rejected, not panic
        let p = dir.join("short.a");
        std::fs::write(&p, b"\x7fELF").unwrap();
        assert!(!lib_arch_matches(&p));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The real shipped staticlib (if present) must match the host arch hyb
    /// was built for — the tarball-contract sanity check.
    #[test]
    fn real_staticlib_matches_host_arch_when_present() {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../runtime/rust/hyrt/target/release/libhyrt.a");
        if !p.exists() {
            return; // no lib built in this checkout — nothing to assert
        }
        assert!(lib_arch_matches(&p), "shipped libhyrt.a arch != host arch");
    }

    /// COFF members (msvc-built hyrt.lib) must be accepted for the host arch
    /// and rejected for a foreign one — the community benchmark report
    /// (v0.1.0) hit hyb rejecting every msvc lib (ELF-only matcher).
    #[test]
    fn lib_arch_matches_accepts_coff_msvc_lib() {
        let dir = std::env::temp_dir().join(format!("hyb-cofftest-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let member = |name: &[u8], size: usize, data: &[u8]| -> Vec<u8> {
            let mut h = vec![b' '; 16];
            h[..name.len()].copy_from_slice(name);
            h.extend_from_slice(b"0           ");
            h.extend_from_slice(b"0     ");
            h.extend_from_slice(b"0     ");
            h.extend_from_slice(b"0       ");
            h.extend_from_slice(format!("{:010}", size).as_bytes());
            h.extend_from_slice(b"`\n");
            let mut v = h;
            v.extend_from_slice(data);
            if size % 2 == 1 {
                v.push(b'\n');
            }
            v
        };
        // An MSVC object: machine type LE u16 at offset 0, no ELF magic.
        // x86-64 = 0x8664, arm64 = 0xAA64 (little-endian bytes below).
        let coff_x86 = [0x64u8, 0x86, 0x01, 0x00, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let coff_arm = [0x64u8, 0xAA, 0x01, 0x00, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        for (name, obj) in [("msvc-x86.lib", &coff_x86), ("msvc-arm.lib", &coff_arm)] {
            let mut ar = b"!<arch>\n".to_vec();
            ar.extend(member(b"/", 4, &[0u8; 4])); // MS linker symbol index (skipped)
            ar.extend(member(b"hyrt.obj/", obj.len(), obj));
            let p = dir.join(name);
            std::fs::write(&p, &ar).unwrap();
            let want = matches!(
                (std::env::consts::ARCH, name),
                ("x86_64", "msvc-x86.lib") | ("aarch64", "msvc-arm.lib")
            );
            assert_eq!(lib_arch_matches(&p), want, "case {}", name);
        }
        // Garbage first data member (neither ELF nor a known COFF machine)
        // must still be rejected.
        let mut junk = b"!<arch>\n".to_vec();
        junk.extend(member(b"j.obj/", 4, &[0x39, 0x05, 0, 0])); // 0x0539 = unknown
        let p = dir.join("msvc-junk.lib");
        std::fs::write(&p, &junk).unwrap();
        assert!(!lib_arch_matches(&p), "unknown COFF machine must be rejected");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The msvc install path must lay BOTH the cgo-contract name and the
    /// .lib aliases; the gnu path stays single-name (no alias churn).
    #[test]
    fn install_staticlib_aliases_the_msvc_flavor() {
        let dir = std::env::temp_dir().join(format!("hyb-installtest-{}", std::process::id()));
        let libdir = dir.join("lib");
        std::fs::create_dir_all(&dir).unwrap();
        // MSVC flavor: source named hyrt.lib
        let src = dir.join("hyrt.lib");
        std::fs::write(&src, b"!<arch>\nCOFF").unwrap();
        install_staticlib(&src, &libdir).unwrap();
        for n in ["libhyrt.a", "hyrt.lib", "libhyrt.lib"] {
            assert!(libdir.join(n).is_file(), "msvc flavor must lay {}", n);
        }
        // GNU flavor: source named libhyrt.a — no .lib aliases.
        let libdir2 = dir.join("lib2");
        let src2 = dir.join("libhyrt.a");
        std::fs::write(&src2, b"!<arch>\nELF").unwrap();
        install_staticlib(&src2, &libdir2).unwrap();
        assert!(libdir2.join("libhyrt.a").is_file());
        assert!(!libdir2.join("hyrt.lib").exists(), "gnu flavor must not alias");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn extract_go_package_name_handles_block_comments_tabs_semicolons_and_trailing_comments() {
        assert_eq!(
            extract_go_package_name("/*\npackage fake_in_comment\n*/\npackage realpkg\n"),
            Some("realpkg".to_string())
        );
        assert_eq!(
            extract_go_package_name("// header\npackage\tcalc//inline comment\n"),
            Some("calc".to_string())
        );
        assert_eq!(
            extract_go_package_name("package/*c*/calc_2;\n"),
            Some("calc_2".to_string())
        );
        assert_eq!(
            extract_go_package_name("package_not_keyword\n"),
            None
        );
    }

    #[test]
    fn generate_one_ignores_stale_generated_hy_go_siblings_on_package_rename() {
        let dir = std::env::temp_dir().join(format!("hyb-gentest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let main_go = dir.join("main.go");
        let stale_hy_go = dir.join("calc.hy.go");
        let calc_hy = dir.join("calc.hy");
        std::fs::write(&main_go, "package oldpkg\n").unwrap();
        std::fs::write(&calc_hy, "fn add(a int, b int) int {\n    return a + b\n}\n").unwrap();

        generate_one(&calc_hy, false, "hybrid.program", &mut None)
            .expect("initial generate_one succeeds");
        assert!(
            std::fs::read_to_string(&stale_hy_go)
                .unwrap()
                .contains("package oldpkg")
        );

        std::fs::write(
            &main_go,
            "/*\npackage commentpkg\n*/\npackage\tnewpkg; // active\n",
        )
        .unwrap();
        generate_one(&calc_hy, false, "hybrid.program", &mut None)
            .expect("generate_one succeeds despite stale .hy.go");
        let generated = std::fs::read_to_string(&stale_hy_go).unwrap();
        assert!(
            generated.contains("package newpkg"),
            "expected regenerated calc.hy.go to adopt `package newpkg`, got:\n{generated}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

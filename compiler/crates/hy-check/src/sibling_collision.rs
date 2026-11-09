//! sibling_collision.rs — Issue #4222: a `.hy` file that collides with a sibling
//! `.go` or `.hy` top-level declaration passes `hyb check` and breaks the package
//! build after `hyb generate` (`redeclared in this block`).
//!
//! Under GUARANTEES.md Row 13 (BUILDABLE), every accepted program must build.
//! When a `.hy` file declares a top-level name (function, type, constant, enum)
//! that collides with a sibling non-test `.go` file or sibling `.hy` file in the
//! same package directory, reject fail-closed as `HYB-E1002`.

use std::collections::HashMap;
use std::path::Path;

use hy_diag::{Diagnostic, Span};
use hy_syntax::Decl;

use crate::model::Checker;

struct DeclTarget {
    name: String,
    emitted_name: Option<String>,
    span: Span,
}

impl<'a> Checker<'a> {
    pub(crate) fn check_sibling_collisions(&mut self) {
        if self.path.is_empty() {
            return;
        }

        let path = Path::new(&self.path);
        // Only run check if the file actually exists on the filesystem (guards in-memory test snippets).
        if !path.is_file() {
            return;
        }

        let dir = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));

        if !dir.is_dir() {
            return;
        }

        // Skip test corpora where each file is an independent test fixture (e.g. tests/checker-corpus).
        if dir.join("VERDICTS.md").exists()
            || path
                .components()
                .any(|c| c.as_os_str() == "checker-corpus" || c.as_os_str() == "testdata")
        {
            return;
        }

        // Collect top-level declarations from the current file
        let mut decls: Vec<DeclTarget> = Vec::new();
        for d in &self.file.decls {
            match d {
                Decl::Fn(f) if f.receiver.is_none() => {
                    let emitted_name = if f.is_pub {
                        Some(to_go_name(&f.name))
                    } else if f.name.contains('.') {
                        Some(f.name.replace('.', "_"))
                    } else {
                        None
                    };
                    decls.push(DeclTarget {
                        name: f.name.clone(),
                        emitted_name,
                        span: f.span,
                    });
                }
                Decl::Const(c) => {
                    decls.push(DeclTarget {
                        name: c.name.clone(),
                        emitted_name: None,
                        span: c.span,
                    });
                }
                Decl::Struct(s) => {
                    decls.push(DeclTarget {
                        name: s.name.clone(),
                        emitted_name: None,
                        span: s.span,
                    });
                }
                Decl::Enum(e) => {
                    decls.push(DeclTarget {
                        name: e.name.clone(),
                        emitted_name: None,
                        span: e.span,
                    });
                }
                Decl::TypeAlias(ta) => {
                    decls.push(DeclTarget {
                        name: ta.name.clone(),
                        emitted_name: None,
                        span: ta.span,
                    });
                }
                _ => {}
            }
        }

        if decls.is_empty() {
            return;
        }

        let current_filename = path.file_name().unwrap_or_default().to_string_lossy();

        // Read sibling directory entries
        let entries = match std::fs::read_dir(dir) {
            Ok(rd) => rd.flatten().map(|e| e.path()).collect::<Vec<_>>(),
            Err(_) => return,
        };

        // Map: declaration name -> (sibling file name, file kind)
        let mut sibling_decls: HashMap<String, (String, &'static str)> = HashMap::new();

        let canon_path = std::fs::canonicalize(path).ok();

        for entry in entries {
            if !entry.is_file() {
                continue;
            }
            let filename = entry.file_name().unwrap_or_default().to_string_lossy();
            if filename == current_filename {
                continue;
            }
            // If entry is a symlink pointing to the current file (or vice versa), skip it.
            if let (Some(ref cp), Ok(canon_entry)) = (&canon_path, std::fs::canonicalize(&entry)) {
                if *cp == canon_entry {
                    continue;
                }
            }

            if filename.ends_with(".go") {
                // Generated .hy.go files and Go test files do not define package authorities.
                if filename.ends_with("_test.go") || filename.ends_with(".hy.go") {
                    continue;
                }
                if let Ok(src) = std::fs::read_to_string(&entry) {
                    // Previously generated hyb outputs carry the generated header and are not package authorities.
                    if src.starts_with("// Code generated by hyb from ") {
                        continue;
                    }
                    for (name, _kind) in extract_go_top_level_decls(&src) {
                        sibling_decls.entry(name).or_insert_with(|| (filename.to_string(), "Go"));
                    }
                }
            } else if filename.ends_with(".hy") {
                if let Ok(src) = std::fs::read_to_string(&entry) {
                    if let Ok(ast) = hy_syntax::parse_file(&entry.to_string_lossy(), &src) {
                        for d in &ast.decls {
                            match d {
                                Decl::Fn(f) if f.receiver.is_none() => {
                                    sibling_decls
                                        .entry(f.name.clone())
                                        .or_insert_with(|| (filename.to_string(), "Hybrid"));
                                    if f.is_pub {
                                        sibling_decls
                                            .entry(to_go_name(&f.name))
                                            .or_insert_with(|| (filename.to_string(), "Hybrid"));
                                    } else if f.name.contains('.') {
                                        sibling_decls
                                            .entry(f.name.replace('.', "_"))
                                            .or_insert_with(|| (filename.to_string(), "Hybrid"));
                                    }
                                }
                                Decl::Const(c) => {
                                    sibling_decls
                                        .entry(c.name.clone())
                                        .or_insert_with(|| (filename.to_string(), "Hybrid"));
                                }
                                Decl::Struct(s) => {
                                    sibling_decls
                                        .entry(s.name.clone())
                                        .or_insert_with(|| (filename.to_string(), "Hybrid"));
                                }
                                Decl::Enum(e) => {
                                    sibling_decls
                                        .entry(e.name.clone())
                                        .or_insert_with(|| (filename.to_string(), "Hybrid"));
                                }
                                Decl::TypeAlias(ta) => {
                                    sibling_decls
                                        .entry(ta.name.clone())
                                        .or_insert_with(|| (filename.to_string(), "Hybrid"));
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }

        // Check if there are any non-test .go sibling files (defines a package directory context)
        let has_sibling_go = sibling_decls.values().any(|(_, source)| *source == "Go");

        // Compare current declarations against sibling declarations
        for decl in &decls {
            let candidates = if let Some(ref em) = decl.emitted_name {
                vec![decl.name.as_str(), em.as_str()]
            } else {
                vec![decl.name.as_str()]
            };

            for cand in candidates {
                if let Some((sibling_file, source_kind)) = sibling_decls.get(cand) {
                    // Standalone executable scripts each declaring `fn main` in a directory
                    // without sibling .go package files are built individually via `hyb build`.
                    if cand == "main" && *source_kind == "Hybrid" && !has_sibling_go {
                        continue;
                    }

                    let msg = format!(
                        "`{cand}` redeclared in this block — sibling `{sibling_file}` already declares `{cand}` (HYB-E1002)"
                    );
                    let why = if *source_kind == "Go" {
                        format!(
                            "why: Go keeps a single package block across all files in a package directory; `{cand}` is already declared in sibling `{sibling_file}` (`{cand} redeclared in this block`), so the package cannot build after `hyb generate` (issue #4222)"
                        )
                    } else {
                        format!(
                            "why: all `.hy` files in a package directory join the same Go package; sibling `{sibling_file}` already declares `{cand}`, which causes `{cand} redeclared in this block` when both files are emitted (issue #4222)"
                        )
                    };
                    self.diags.push(
                        Diagnostic::error("HYB-E1002", self.path.clone(), decl.span, msg).with_notes(vec![
                            why,
                            format!(
                                "fix: rename `{cand}` in `{current_filename}` or `{sibling_file}` — top-level declarations within a package must have unique names"
                            ),
                        ]),
                    );
                    break;
                }
            }
        }
    }
}

fn to_go_name(name: &str) -> String {
    name.split('_')
        .filter(|p| !p.is_empty())
        .map(|p| {
            if let Some(init) = go_initialism(p) {
                return init.to_string();
            }
            let mut cs = p.chars();
            match cs.next() {
                Some(c) => c.to_uppercase().collect::<String>() + cs.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

fn go_initialism(seg: &str) -> Option<&'static str> {
    Some(match seg {
        "acl" => "ACL",
        "api" => "API",
        "ascii" => "ASCII",
        "cpu" => "CPU",
        "crlf" => "CRLF",
        "css" => "CSS",
        "dns" => "DNS",
        "eof" => "EOF",
        "guid" => "GUID",
        "html" => "HTML",
        "http" => "HTTP",
        "https" => "HTTPS",
        "id" => "ID",
        "ip" => "IP",
        "ipv4" => "IPv4",
        "ipv6" => "IPv6",
        "json" => "JSON",
        "lhs" => "LHS",
        "ok" => "OK",
        "qps" => "QPS",
        "ram" => "RAM",
        "rfc822" => "RFC822",
        "rfc822z" => "RFC822Z",
        "rfc850" => "RFC850",
        "rfc1123" => "RFC1123",
        "rfc1123z" => "RFC1123Z",
        "rfc3339" => "RFC3339",
        "rhs" => "RHS",
        "rlock" => "RLock",
        "rpc" => "RPC",
        "runlock" => "RUnlock",
        "rw" => "RW",
        "rwmutex" => "RWMutex",
        "sha1" => "SHA1",
        "sha256" => "SHA256",
        "sha384" => "SHA384",
        "sha512" => "SHA512",
        "sla" => "SLA",
        "smtp" => "SMTP",
        "sql" => "SQL",
        "ssh" => "SSH",
        "tcp" => "TCP",
        "tls" => "TLS",
        "ttl" => "TTL",
        "udp" => "UDP",
        "ui" => "UI",
        "uid" => "UID",
        "uri" => "URI",
        "url" => "URL",
        "utf8" => "UTF8",
        "utf16" => "UTF16",
        "utf32" => "UTF32",
        _ => return None,
    })
}

fn extract_go_top_level_decls(src: &str) -> Vec<(String, &'static str)> {
    let mut decls = Vec::new();
    let mut clean = String::with_capacity(src.len());
    let mut chars = src.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '/' && chars.peek() == Some(&'*') {
            chars.next();
            while let Some(inner) = chars.next() {
                if inner == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    break;
                }
            }
        } else {
            clean.push(c);
        }
    }

    let mut in_paren_block: Option<&'static str> = None;
    for raw_line in clean.lines() {
        let line = if let Some(idx) = raw_line.find("//") {
            &raw_line[..idx]
        } else {
            raw_line
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(kind) = in_paren_block {
            if line.starts_with(')') {
                in_paren_block = None;
            } else if let Some(ident) = leading_go_ident(line) {
                decls.push((ident, kind));
            }
            continue;
        }

        // func <ident> (must NOT start with '(' which indicates a method receiver)
        if let Some(rest) = line.strip_prefix("func") {
            let rest = rest.trim_start();
            if !rest.starts_with('(') {
                if let Some(ident) = leading_go_ident(rest) {
                    decls.push((ident, "function"));
                    continue;
                }
            }
        }

        // Block starters: type (, var (, const (
        if line.starts_with("type") && line.strip_prefix("type").unwrap_or("").trim() == "(" {
            in_paren_block = Some("type");
            continue;
        }
        if line.starts_with("var") && line.strip_prefix("var").unwrap_or("").trim() == "(" {
            in_paren_block = Some("variable");
            continue;
        }
        if line.starts_with("const") && line.strip_prefix("const").unwrap_or("").trim() == "(" {
            in_paren_block = Some("constant");
            continue;
        }

        // Single declarations: type <ident>, var <ident>, const <ident>
        if let Some(rest) = line.strip_prefix("type") {
            let rest = rest.trim_start();
            if let Some(ident) = leading_go_ident(rest) {
                decls.push((ident, "type"));
                continue;
            }
        }
        if let Some(rest) = line.strip_prefix("var") {
            let rest = rest.trim_start();
            if let Some(ident) = leading_go_ident(rest) {
                decls.push((ident, "variable"));
                continue;
            }
        }
        if let Some(rest) = line.strip_prefix("const") {
            let rest = rest.trim_start();
            if let Some(ident) = leading_go_ident(rest) {
                decls.push((ident, "constant"));
                continue;
            }
        }
    }
    decls
}

fn leading_go_ident(s: &str) -> Option<String> {
    let mut chars = s.char_indices();
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
    Some(s[..end].to_string())
}

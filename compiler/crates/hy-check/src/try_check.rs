//! try_check.rs — Legality pass for the `?` try operator.
//!
//! SPEC-LANGUAGE.md §6.1 E3, E3b, E4, T20:
//!   * E4: `?` applies only to error-returning calls; on `option` it is a type error (T20);
//!   * E4: `?` on any call that does not return error is a type error (HYB-E2003);
//!   * E3b: `?` in a function or closure whose results do not end in error is a compile error
//!     because propagation has nowhere to land (HYB-E2004);
//!   * Defer: `?` cannot appear inside a `defer` statement (HYB-E2001).

use crate::model::{unwrap_parens, Checker, FnState, TypeShape};
use hy_diag::Diagnostic;
use hy_syntax::{Block, Expr, FnDecl, ForKind, InterpPart, SelectArmKind, Stmt, TypeExpr};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CallResultKind {
    /// Returns error as its last result (e.g. `(T, error)` or `error`).
    Error,
    /// Returns `option[T]`.
    Option,
    /// Returns a non-error result (or no result).
    NotError,
    /// Unresolvable call.
    Unknown,
}

pub(crate) fn is_error_type(ty: &TypeExpr) -> bool {
    matches!(ty, TypeExpr::Named { name, .. } if name == "error")
}

pub(crate) fn is_option_type(ty: &TypeExpr) -> bool {
    match ty {
        TypeExpr::Option { .. } => true,
        TypeExpr::Named { name, .. } => name == "option",
        _ => false,
    }
}

/// Determine whether a call expression returns an error, an option, or a non-error value.
pub(crate) fn call_result_kind(ck: &Checker<'_>, st: &FnState, expr: &Expr) -> CallResultKind {
    let unwrapped = unwrap_parens(expr);
    let Expr::Call { callee, .. } = unwrapped else {
        // E3: `?` is for calls only. Non-calls under `?` are never error-returning.
        return CallResultKind::NotError;
    };

    match unwrap_parens(callee) {
        Expr::Ident { name, .. } => {
            // 1. Local binding in scope.
            if let Some(b) = ck.lookup_binding(st, name) {
                if let Some(TypeExpr::Fn { results, .. }) = &b.ty {
                    if results.is_empty() {
                        return CallResultKind::NotError;
                    }
                    if is_error_type(results.last().unwrap()) {
                        return CallResultKind::Error;
                    }
                    if is_option_type(results.last().unwrap()) {
                        return CallResultKind::Option;
                    }
                    return CallResultKind::NotError;
                }
                if !matches!(b.shape, TypeShape::Unknown) {
                    return CallResultKind::NotError;
                }
                return CallResultKind::Unknown;
            }

            // 2. Builtin constructors / functions.
            match name.as_str() {
                "some" | "none" => return CallResultKind::Option,
                "make" | "new" | "len" | "cap" | "min" | "max" | "append" | "close" | "panic" => {
                    return CallResultKind::NotError;
                }
                _ => {}
            }

            // 3. User top-level functions in the file.
            if let Some(f) = ck.fns.get(name.as_str()) {
                if f.results.is_empty() {
                    return CallResultKind::NotError;
                }
                let last = f.results.last().unwrap();
                if is_error_type(&last.ty) {
                    return CallResultKind::Error;
                }
                if is_option_type(&last.ty) {
                    return CallResultKind::Option;
                }
                return CallResultKind::NotError;
            }

            CallResultKind::Unknown
        }

        Expr::Field { base, name, .. } => {
            // Package-level calls: `os.open`, `serde_json.parse`, etc.
            if let Expr::Ident { name: pkg, .. } = unwrap_parens(base) {
                if ck.lookup_binding(st, pkg).is_none() && ck.known_package(pkg) {
                    return package_call_result_kind(pkg, name);
                }
            }

            // Method calls on typed receiver.
            let recv_shape = ck.shape_of_expr(st, base);
            match &recv_shape {
                TypeShape::OwnHandle(h) | TypeShape::BorrowHandle(h) => {
                    if h == "serde_json.value" || h == "serde_json.view" {
                        if matches!(
                            name.as_str(),
                            "array" | "int" | "string" | "set" | "push_string" | "get"
                        ) {
                            return CallResultKind::Error;
                        }
                        return CallResultKind::NotError;
                    }
                    if h == "flate2.writer" {
                        if matches!(name.as_str(), "write" | "finish" | "new_writer") {
                            return CallResultKind::Error;
                        }
                        return CallResultKind::NotError;
                    }
                }
                TypeShape::Atomic => return CallResultKind::NotError,
                TypeShape::Struct(Some(sname)) => {
                    // Check declared methods on this struct in the file
                    for f in ck.file.fns() {
                        if f.name == *name {
                            if let Some(r) = &f.receiver {
                                if receiver_type_matches(&r.ty, sname) {
                                    if f.results.is_empty() {
                                        return CallResultKind::NotError;
                                    }
                                    let last = f.results.last().unwrap();
                                    if is_error_type(&last.ty) {
                                        return CallResultKind::Error;
                                    }
                                    if is_option_type(&last.ty) {
                                        return CallResultKind::Option;
                                    }
                                    return CallResultKind::NotError;
                                }
                            }
                        }
                    }
                }
                _ => {}
            }

            CallResultKind::Unknown
        }

        Expr::FnLit { results, .. } => {
            if results.is_empty() {
                return CallResultKind::NotError;
            }
            let last = results.last().unwrap();
            if is_error_type(&last.ty) {
                return CallResultKind::Error;
            }
            if is_option_type(&last.ty) {
                return CallResultKind::Option;
            }
            CallResultKind::NotError
        }

        _ => CallResultKind::Unknown,
    }
}

fn receiver_type_matches(ty: &TypeExpr, struct_name: &str) -> bool {
    match ty {
        TypeExpr::Named { name, .. } => name == struct_name,
        TypeExpr::Pointer { to, .. } => receiver_type_matches(to, struct_name),
        _ => false,
    }
}

fn package_call_result_kind(pkg: &str, method: &str) -> CallResultKind {
    match pkg {
        "os" => {
            if matches!(
                method,
                "open"
                    | "create"
                    | "read_file"
                    | "write_file"
                    | "remove"
                    | "remove_all"
                    | "mkdir"
                    | "mkdir_all"
                    | "stat"
                    | "lstat"
                    | "rename"
                    | "chmod"
                    | "chown"
                    | "truncate"
                    | "readlink"
            ) {
                CallResultKind::Error
            } else if matches!(
                method,
                "exit"
                    | "getenv"
                    | "getpid"
                    | "getppid"
                    | "getuid"
                    | "getgid"
                    | "getpagesize"
                    | "temp_dir"
            ) {
                CallResultKind::NotError
            } else {
                CallResultKind::Unknown
            }
        }
        "io" => {
            if matches!(
                method,
                "read_all"
                    | "read_full"
                    | "copy"
                    | "copy_n"
                    | "copy_buffer"
                    | "read_at_least"
                    | "write_string"
            ) {
                CallResultKind::Error
            } else if matches!(
                method,
                "pipe" | "limit_reader" | "multi_reader" | "multi_writer" | "tee_reader"
            ) {
                CallResultKind::NotError
            } else {
                CallResultKind::Unknown
            }
        }
        "serde_json" => {
            if matches!(method, "parse" | "to_string_pretty") {
                CallResultKind::Error
            } else if matches!(method, "new_object" | "new_array") {
                CallResultKind::NotError
            } else {
                CallResultKind::Unknown
            }
        }
        "strconv" => {
            if matches!(
                method,
                "atoi" | "parse_int" | "parse_float" | "parse_bool" | "parse_uint"
            ) {
                CallResultKind::Error
            } else if matches!(
                method,
                "itoa"
                    | "format_int"
                    | "format_float"
                    | "format_bool"
                    | "format_uint"
            ) {
                CallResultKind::NotError
            } else {
                CallResultKind::Unknown
            }
        }
        "httptest" => {
            if matches!(method, "get" | "post") {
                CallResultKind::Error
            } else if matches!(method, "new_server" | "new_recorder") {
                CallResultKind::NotError
            } else {
                CallResultKind::Unknown
            }
        }
        "regex" => {
            if matches!(method, "compile" | "is_match") {
                CallResultKind::Error
            } else if matches!(method, "is_valid") {
                CallResultKind::NotError
            } else {
                CallResultKind::Unknown
            }
        }
        "sha3" => {
            if matches!(method, "digest256" | "stream_checksum") {
                CallResultKind::Error
            } else {
                CallResultKind::Unknown
            }
        }
        "fmt" | "strings" | "atomic" | "sort" => CallResultKind::NotError,
        "time" => {
            if matches!(method, "now" | "since" | "sleep" | "duration") {
                CallResultKind::NotError
            } else if matches!(method, "parse_duration" | "parse") {
                CallResultKind::Error
            } else {
                CallResultKind::NotError
            }
        }
        _ => CallResultKind::Unknown,
    }
}

/// Check all `Expr::Try` usages within a function declaration against E3, E3b, E4, and T20.
pub(crate) fn check_tries_in_fn(ck: &mut Checker<'_>, f: &FnDecl, st: &FnState) {
    let Some(body) = &f.body else { return };
    let is_main = f.name == "main" && f.receiver.is_none();
    let fn_returns_error = matches!(f.results.last(), Some(r) if is_error_type(&r.ty));
    let top_can_propagate = is_main || fn_returns_error;

    check_block(ck, st, body, top_can_propagate, false);
}

fn check_block(
    ck: &mut Checker<'_>,
    st: &FnState,
    block: &Block,
    can_propagate: bool,
    in_defer: bool,
) {
    for stmt in &block.stmts {
        check_stmt(ck, st, stmt, can_propagate, in_defer);
    }
}

fn check_stmt(
    ck: &mut Checker<'_>,
    st: &FnState,
    stmt: &Stmt,
    can_propagate: bool,
    in_defer: bool,
) {
    match stmt {
        Stmt::Local(l) => {
            for v in &l.values {
                check_expr(ck, st, v, can_propagate, in_defer);
            }
        }
        Stmt::Assign { values, .. } => {
            for v in values {
                check_expr(ck, st, v, can_propagate, in_defer);
            }
        }
        Stmt::Expr { expr, .. } => {
            check_expr(ck, st, expr, can_propagate, in_defer);
        }
        Stmt::If {
            init,
            cond,
            then,
            otherwise,
            ..
        } => {
            if let Some(init) = init {
                check_stmt(ck, st, init, can_propagate, in_defer);
            }
            check_expr(ck, st, cond, can_propagate, in_defer);
            check_block(ck, st, then, can_propagate, in_defer);
            if let Some(otherwise) = otherwise {
                check_stmt(ck, st, otherwise, can_propagate, in_defer);
            }
        }
        Stmt::For { kind, body, .. } => {
            match kind {
                ForKind::Iterate { expr, .. } => check_expr(ck, st, expr, can_propagate, in_defer),
                ForKind::Range { lo, hi, .. } => {
                    check_expr(ck, st, lo, can_propagate, in_defer);
                    check_expr(ck, st, hi, can_propagate, in_defer);
                }
                ForKind::While { cond } => check_expr(ck, st, cond, can_propagate, in_defer),
                ForKind::Infinite => {}
            }
            check_block(ck, st, body, can_propagate, in_defer);
        }
        Stmt::Return { values, .. } => {
            for v in values {
                check_expr(ck, st, v, can_propagate, in_defer);
            }
        }
        Stmt::Defer { call, span } => {
            if !matches!(call, Expr::Call { .. }) {
                ck.diags.push(
                    Diagnostic::error(
                        "HYB-E2001",
                        ck.path.clone(),
                        *span,
                        "defer requires a call expression (HYB-E2001)",
                    )
                    .with_notes(vec![
                        "`?` cannot be used in a defer statement — defer executes at function return and cannot propagate errors (E3)".to_string(),
                    ])
                    .with_rule("P"),
                );
            }
            check_expr(ck, st, call, can_propagate, true);
        }
        Stmt::Task { callee, args, .. } => {
            check_expr(ck, st, callee, can_propagate, in_defer);
            for a in args {
                check_expr(ck, st, &a.value, can_propagate, in_defer);
            }
        }
        Stmt::Send { chan, value, .. } => {
            check_expr(ck, st, chan, can_propagate, in_defer);
            check_expr(ck, st, value, can_propagate, in_defer);
        }
        Stmt::Match { subject, arms, .. } => {
            check_expr(ck, st, subject, can_propagate, in_defer);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    check_expr(ck, st, g, can_propagate, in_defer);
                }
                check_block(ck, st, &arm.body, can_propagate, in_defer);
            }
        }
        Stmt::Select { arms, .. } => {
            for arm in arms {
                match &arm.kind {
                    SelectArmKind::Send { chan, value } => {
                        check_expr(ck, st, chan, can_propagate, in_defer);
                        check_expr(ck, st, value, can_propagate, in_defer);
                    }
                    SelectArmKind::Recv { chan, .. } => {
                        check_expr(ck, st, chan, can_propagate, in_defer);
                    }
                    SelectArmKind::Default => {}
                }
                check_block(ck, st, &arm.body, can_propagate, in_defer);
            }
        }
        Stmt::Managed { body, .. } => {
            check_block(ck, st, body, can_propagate, in_defer);
        }
        Stmt::Block { block, .. } => {
            check_block(ck, st, block, can_propagate, in_defer);
        }
        Stmt::Break { .. } | Stmt::Continue { .. } => {}
    }
}

fn check_expr(
    ck: &mut Checker<'_>,
    st: &FnState,
    expr: &Expr,
    can_propagate: bool,
    in_defer: bool,
) {
    match expr {
        Expr::Try { operand, span } => {
            if in_defer {
                // Defer cannot propagate errors
                ck.diags.push(
                    Diagnostic::error(
                        "HYB-E2001",
                        ck.path.clone(),
                        *span,
                        "`?` cannot be used in a defer statement — defer executes at function return and cannot propagate errors (E3)",
                    )
                    .with_notes(vec![
                        "handle the error inside the deferred function or remove `?`".to_string(),
                    ])
                    .with_rule("P"),
                );
                check_expr(ck, st, operand, can_propagate, in_defer);
                return;
            }

            if !can_propagate {
                ck.diags.push(
                    Diagnostic::error(
                        "HYB-E2004",
                        ck.path.clone(),
                        *span,
                        "`?` in a function or closure whose results do not end in error — the propagation has nowhere to land (E3b)",
                    )
                    .with_notes(vec![
                        "add `error` to the enclosing function or closure results (E4)".to_string(),
                    ])
                    .with_rule("E3b"),
                );
                check_expr(ck, st, operand, can_propagate, in_defer);
                return;
            }

            let kind = call_result_kind(ck, st, operand);
            match kind {
                CallResultKind::Option => {
                    ck.diags.push(
                        Diagnostic::error(
                            "HYB-E2003",
                            ck.path.clone(),
                            *span,
                            "`?` on an option result — absence and failure are distinct (T20); use match/if-ok or an error-returning call",
                        )
                        .with_notes(vec![
                            "`option[T]` represents absence, not an error — use match or `if v, ok := opt; ok` (T20)".to_string(),
                        ])
                        .with_rule("T20"),
                    );
                }
                CallResultKind::NotError => {
                    ck.diags.push(
                        Diagnostic::error(
                            "HYB-E2003",
                            ck.path.clone(),
                            *span,
                            "`?` on a call that does not return error (E4)",
                        )
                        .with_notes(vec![
                            "`?` is only legal on calls whose return type ends in `error` (E4)".to_string(),
                        ])
                        .with_rule("E4"),
                    );
                }
                CallResultKind::Error | CallResultKind::Unknown => {}
            }

            check_expr(ck, st, operand, can_propagate, in_defer);
        }

        Expr::Call { callee, args, .. } => {
            // E11 handler contract: `http.handler(fn(w, r) { ... })`
            let is_e11_target = match unwrap_parens(callee) {
                Expr::Field { base, name, .. } => {
                    if let Expr::Ident { name: pkg, .. } = unwrap_parens(base) {
                        pkg == "http" && name == "handler"
                    } else {
                        false
                    }
                }
                _ => false,
            };

            check_expr(ck, st, callee, can_propagate, in_defer);
            for (i, a) in args.iter().enumerate() {
                if is_e11_target && i == 0 {
                    // This closure is an E11 handler contract: can propagate!
                    if let Expr::FnLit { body, .. } = unwrap_parens(&a.value) {
                        check_block(ck, st, body, true, in_defer);
                        continue;
                    }
                }
                check_expr(ck, st, &a.value, can_propagate, in_defer);
            }
        }

        Expr::FnLit { results, body, .. } => {
            let last_is_err = matches!(results.last(), Some(r) if is_error_type(&r.ty));
            check_block(ck, st, body, last_is_err, false);
        }

        Expr::Binary { lhs, rhs, .. } => {
            check_expr(ck, st, lhs, can_propagate, in_defer);
            check_expr(ck, st, rhs, can_propagate, in_defer);
        }
        Expr::Unary { operand, .. }
        | Expr::AddrOf { operand, .. }
        | Expr::Move { operand, .. }
        | Expr::Paren { inner: operand, .. } => {
            check_expr(ck, st, operand, can_propagate, in_defer);
        }
        Expr::Field { base, .. } => {
            check_expr(ck, st, base, can_propagate, in_defer);
        }
        Expr::Index { base, index, .. } => {
            check_expr(ck, st, base, can_propagate, in_defer);
            check_expr(ck, st, index, can_propagate, in_defer);
        }
        Expr::Slice { base, lo, hi, .. } => {
            check_expr(ck, st, base, can_propagate, in_defer);
            if let Some(l) = lo {
                check_expr(ck, st, l, can_propagate, in_defer);
            }
            if let Some(h) = hi {
                check_expr(ck, st, h, can_propagate, in_defer);
            }
        }
        Expr::Receive { chan, .. } => {
            check_expr(ck, st, chan, can_propagate, in_defer);
        }
        Expr::StructLit { fields, .. } => {
            for f in fields {
                check_expr(ck, st, &f.value, can_propagate, in_defer);
            }
        }
        Expr::SliceLit { items, .. } => {
            for item in items {
                check_expr(ck, st, item, can_propagate, in_defer);
            }
        }
        Expr::MapLit { entries, .. } => {
            for (k, v) in entries {
                check_expr(ck, st, k, can_propagate, in_defer);
                check_expr(ck, st, v, can_propagate, in_defer);
            }
        }
        Expr::Interp { parts, .. } => {
            for p in parts {
                if let InterpPart::Expr(e) = p {
                    check_expr(ck, st, e, can_propagate, in_defer);
                }
            }
        }
        Expr::Ident { .. } | Expr::Lit { .. } => {}
    }
}

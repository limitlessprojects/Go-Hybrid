//! Expression emission — `Emitter` impl part 3.
//!
//! Precedence-aware minimal parenthesization (users read the emitted Go),
//! the special-member table (`os.args()` → `os.Args`, `fmt.eprintln` →
//! `fmt.Fprintln(os.Stderr, …)`, duration constructors), and the bug-#2
//! fix: `make`'s first argument renders as a BARE TYPE — the parser encodes
//! type-as-value as an empty `StructLit{ty: Some(t)}` / `SliceLit` /
//! `MapLit`, and rendering those as value literals inside `make(...)` is
//! invalid Go.

use hy_syntax::*;

use super::{
    go_quote, sanitize_ident, special_member, to_go_name, BindingKind, EmitError, Emitter,
    MemberSpecial,
};

use super::shapes::{
    lookup_conversion, lookup_member, lookup_rust_crate, lookup_unbound_crate, MemberKind,
    RustCrateDesc,
};

/// Operator precedence (Go tiers; higher binds tighter).
fn bin_prec(op: BinOp) -> u8 {
    match op {
        BinOp::Or => 1,
        BinOp::And => 2,
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => 3,
        BinOp::Add | BinOp::Sub => 4,
        BinOp::Mul | BinOp::Div | BinOp::Mod => 5,
        // SPEC-LANGUAGE 1.4: `|` `^` are AddOp level, `&` `<<` `>>` are MulOp level.
        BinOp::BitOr | BinOp::BitXor => 4,
        BinOp::BitAnd | BinOp::Shl | BinOp::Shr => 5,
    }
}

pub(crate) const ATOM: u8 = 100;

impl Emitter {
    /// The full expression emitter. `min_prec` = the precedence the CALLER
    /// requires; lower-precedence children parenthesize themselves.
    pub fn emit_expr(&mut self, e: &Expr, min_prec: u8) -> Result<String, EmitError> {
        if let Some(t) = self.expr_temps.get(&Self::expr_key(e)) {
            return Ok(t.clone());
        }
        let text = self.emit_expr_inner(e)?;
        let my = self.top_prec(e);
        if my < min_prec {
            Ok(format!("({})", text))
        } else {
            Ok(text)
        }
    }

    /// Expression emission with an EXPECTED type (Batch I, corpus 09): a
    /// type-elided struct literal inherits the context's type — a map
    /// literal's VALUE type, a slice literal's ELEM type. The elision is
    /// Go's own composite-literal sugar; the emitter fills in the head.
    /// Everything else emits exactly like emit_expr.
    pub fn emit_expr_exp(
        &mut self,
        e: &Expr,
        min_prec: u8,
        expected: Option<&TypeExpr>,
    ) -> Result<String, EmitError> {
        if let Some(exp) = expected {
            // `Paren` is semantically transparent: `({replicas: 2})` in a
            // map-value or slice-elem slot still has to inherit the expected
            // type's head, or the elided literal falls through to the
            // "spell the type" refusal. Peeling here is safe — the parens are
            // redundant in a Go element position and are dropped from output.
            let mut lit = e;
            while let Expr::Paren { inner, .. } = lit {
                lit = inner;
            }
            match lit {
                Expr::StructLit {
                    ty: None, fields, ..
                } => {
                    let head = self.type_text(exp);
                    let body = self.struct_lit_body(exp, fields)?;
                    return Ok(format!("{}{{{}}}", head, body)); // atomic head
                }
                Expr::SliceLit {
                    elem: None, items, ..
                } => match exp {
                    TypeExpr::Slice { elem, .. } | TypeExpr::Array { elem, .. } => {
                        let head = self.type_text(exp);
                        let mut parts = Vec::new();
                        for i in items {
                            parts.push(self.emit_expr_exp(i, 0, Some(elem))?);
                        }
                        return Ok(format!("{}{{{}}}", head, parts.join(", ")));
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        self.emit_expr(e, min_prec)
    }

    fn top_prec(&self, e: &Expr) -> u8 {
        match e {
            Expr::Binary { op, .. } => bin_prec(*op),
            Expr::Call { callee, args, .. } if args.len() == 1 => {
                if let Expr::Field { base, name, .. } = callee.as_ref() {
                    if let Expr::Ident { name: pkg, .. } = base.as_ref() {
                        if self.is_unshadowed_pkg_root(pkg)
                            && matches!(special_member(pkg, name), Some(MemberSpecial::Duration(_)))
                        {
                            return bin_prec(BinOp::Mul);
                        }
                    }
                }
                ATOM
            }
            _ => ATOM,
        }
    }

    fn emit_expr_inner(&mut self, e: &Expr) -> Result<String, EmitError> {
        match e {
            Expr::Ident { name, .. } => {
                if name == "none" {
                    return Err(EmitError::unsupported(
                        "`none` outside a return (MVP-0 lowers `return none` against the declared option result)",
                    ));
                }
                let (head, type_args) = split_type_args(name);
                if let Some(ta) = type_args {
                    let translated = self.translate_type_args_suffix(ta);
                    return Ok(format!("{}{}", sanitize_ident(head), translated));
                }
                Ok(sanitize_ident(name).to_string())
            }

            Expr::Lit { kind, .. } => Ok(self.lit_text(kind)),

            Expr::Interp { parts, .. } => self.emit_interp(parts),

            Expr::Binary { op, lhs, rhs, .. } => {
                let p = bin_prec(*op);
                let l = self.emit_expr(lhs, p)?;
                let r = self.emit_expr(rhs, p + 1)?;
                Ok(format!("{} {} {}", l, op.render(), r))
            }

            Expr::Unary { op, operand, .. } => {
                let o = self.emit_expr(operand, ATOM)?;
                if matches!(op, UnOp::Neg) && o.starts_with('-') {
                    Ok(format!("-({})", o))
                } else {
                    Ok(format!("{}{}", op.render(), o))
                }
            }

            Expr::AddrOf { operand, .. } => {
                let o = self.emit_expr(operand, ATOM)?;
                Ok(format!("&{}", o))
            }

            // The move marker is ownership bookkeeping (O5) — the checker
            // verified it; Go receives the plain value.
            Expr::Move { operand, .. } => self.emit_expr_inner(operand),

            // E3: a hoisted try substitutes its temp (the expression-form
            // desugar); a try the hoister never saw is a clean error.
            Expr::Try { span, .. } => {
                if let Some(t) = self.try_temps.get(&(span.start.line, span.start.col)) {
                    return Ok(t.clone());
                }
                Err(EmitError::unsupported(
                    "`?` outside a binding position — E3 desugars binding-position and \
                     hoisted-expression `?` only"
                        .to_string(),
                ))
            }

            Expr::Receive { chan, .. } => {
                let c = self.emit_expr(chan, ATOM)?;
                Ok(format!("<-{}", c))
            }

            Expr::Index { base, index, .. } => {
                let b = self.emit_expr(base, ATOM)?;
                let i = self.emit_expr(index, 0)?;
                Ok(format!("{}[{}]", b, i))
            }

            Expr::Slice { base, lo, hi, .. } => {
                let b = self.emit_expr(base, ATOM)?;
                let lo_t = match lo {
                    Some(x) => self.emit_expr(x, 0)?,
                    None => String::new(),
                };
                let hi_t = match hi {
                    Some(x) => self.emit_expr(x, 0)?,
                    None => String::new(),
                };
                Ok(format!("{}[{}:{}]", b, lo_t, hi_t))
            }

            Expr::Field { base, name, .. } => {
                // `(x).f` is `x.f` in Go: peel a redundant receiver paren so
                // the member spelling is resolved and the emitted Go is the
                // same access (issue #618). A non-primary base keeps its
                // parentheses — `(*p).f` and `(a + b).f` need them.
                let base = Self::selector_base(base);
                let b = self.emit_expr(base, ATOM)?;
                // Package-qualified members register the import.
                if let Expr::Ident { name: root, .. } = base {
                    if self.is_unshadowed_pkg_root(root) {
                        self.use_package(root);
                    }
                }
                Ok(format!("{}.{}", b, self.member_of_typed(base, name)))
            }

            Expr::Call { callee, args, .. } => self.emit_call(callee, args),

            Expr::StructLit { ty, fields, .. } => {
                if let Some(t) = ty {
                    if matches!(t, TypeExpr::Chan { .. }) {
                        return Err(EmitError::unsupported(
                            "channel type is not a value expression — construct a channel with `make(chan T)`",
                        ));
                    }
                    let head = self.type_text(t);
                    let body = self.struct_lit_body(t, fields)?;
                    Ok(format!("{}{{{}}}", head, body))
                } else {
                    Err(EmitError::unsupported(
                        "type-elided struct literal — spell the type (MVP-0 emission surface)",
                    ))
                }
            }

            Expr::SliceLit { elem, items, .. } => {
                if let Some((_, inner_elem, arr_ty)) = e.as_array_lit() {
                    let head = self.type_text(arr_ty);
                    let mut parts = Vec::new();
                    for i in items {
                        parts.push(self.emit_expr_exp(i, 0, Some(inner_elem))?);
                    }
                    return Ok(format!("{}{{{}}}", head, parts.join(", ")));
                }
                let head = match elem {
                    Some(t) => format!("[]{}", self.type_text(t)),
                    None => self.infer_slice_head(items)?,
                };
                let mut parts = Vec::new();
                for i in items {
                    // Elided struct elements inherit the slice's element
                    // type (Batch I, corpus 09).
                    parts.push(self.emit_expr_exp(i, 0, elem.as_deref())?);
                }
                Ok(format!("{}{{{}}}", head, parts.join(", ")))
            }

            Expr::MapLit {
                key,
                value,
                entries,
                ..
            } => {
                let head = format!("map[{}]{}", self.type_text(key), self.type_text(value));
                let mut parts = Vec::new();
                for (k, v) in entries {
                    let kt = self.emit_expr(k, 0)?;
                    // Elided struct values inherit the map's VALUE type
                    // (Batch I, corpus 09: `map[string]service{"k": {…}}`).
                    let vt = self.emit_expr_exp(v, 0, Some(value))?;
                    parts.push(format!("{}: {}", kt, vt));
                }
                Ok(format!("{}{{{}}}", head, parts.join(", ")))
            }

            Expr::FnLit {
                params,
                results,
                body,
                ..
            } => {
                // Conversion target (Batch E): untyped params take the
                // TARGET's types (`http.handler(fn(w, r) {…})` — the
                // literal's `w, r` are typed by HandlerFunc's signature).
                // Typed params in the source win; a count mismatch errors.
                let conv = self.pending_fnlit_conv.take(); // only the params consume it
                let mut untyped_params: Vec<usize> = Vec::new();
                let params: Vec<Param> =
                    if let Some(conv) = conv {
                        if params.len() != conv.params.len() {
                            return Err(EmitError::unsupported(format!(
                            "function literal takes {} params; the {} target's signature has {}",
                            params.len(),
                            conv.go,
                            conv.params.len()
                        )));
                        }
                        params
                        .iter()
                        .zip(conv.params)
                        .enumerate()
                        .map(|(i, (p, d))| {
                            let untyped =
                                matches!(&p.ty, TypeExpr::Named { name, .. } if name.is_empty());
                            if untyped {
                                untyped_params.push(i);
                                // The Go spelling the target's signature uses
                                // (`w ResponseWriter` interface vs `r *Request`
                                // pointer — ParamDesc carries the difference).
                                let base = TypeExpr::Named {
                                    name: d.go.to_string(),
                                    span: p.ty.span(),
                                };
                                let ty = if d.pointer {
                                    TypeExpr::Pointer { to: Box::new(base), span: p.ty.span() }
                                } else {
                                    base
                                };
                                Param { name: p.name.clone(), ty, span: p.span }
                            } else {
                                p.clone()
                            }
                        })
                        .collect()
                    } else {
                        params.clone()
                    };
                let rendered_params = params
                    .iter()
                    .map(|p| format!("{} {}", p.name, self.type_text(&p.ty)))
                    .collect::<Vec<_>>()
                    .join(", ");
                // Hints from closure param types (example-06's w, r) —
                // scoped: removed again when the literal ends. Conv-typed
                // params bind through the DESCRIPTOR's shape key (the Go
                // spelling in the signature is not the table's key).
                let saved_scope = self.save_scope();
                for r in results {
                    if let Some(n) = &r.name {
                        if n != "_" {
                            self.shadow_local_name(n);
                            if matches!(r.ty, TypeExpr::Pointer { .. }) {
                                self.pointer_vars.insert(n.clone());
                            }
                            if let Some(kind) = self.binding_kind_of_ty(&r.ty) {
                                self.bindings.insert(n.clone(), kind);
                            }
                            self.register_var_ty(n, &r.ty);
                        }
                    }
                }
                for (i, p) in params.iter().enumerate() {
                    if p.name != "_" {
                        self.shadow_local_name(&p.name);
                        if matches!(p.ty, TypeExpr::Pointer { .. }) {
                            self.pointer_vars.insert(p.name.clone());
                        }
                    }
                    self.register_var_ty(&p.name, &p.ty);
                    let desc_kind = conv
                        .and_then(|c| c.params.get(i))
                        .filter(|_| untyped_params.contains(&i))
                        .map(|d| BindingKind::ForeignTy(d.ty_key));
                    let kind = desc_kind.or_else(|| self.binding_kind_of_ty(&p.ty));
                    if let Some(kind) = kind {
                        self.bindings.insert(p.name.clone(), kind);
                    } else if self.type_is_foreign(&p.ty) {
                        self.bindings.insert(p.name.clone(), BindingKind::Foreign);
                    }
                }
                // E11: inside a propagation-carrying conversion the `?`
                // contract names the writer param.
                let saved_writer = self.handler_writer.take();
                if let Some(w) = conv.and_then(|c| c.propagate) {
                    // The writer param's NAME as the literal spells it
                    // (falls back to the descriptor's conventional one).
                    let name = params
                        .first()
                        .map(|p| p.name.clone())
                        .unwrap_or_else(|| w.to_string());
                    self.handler_writer = Some(name);
                }
                self.fnlit_depth += 1;
                // Labels do not cross func boundaries: a labeled break/continue
                // must find its loop INSIDE the literal — reset the depth so
                // an outer loop's count cannot bless a closure-nested label.
                let saved_for_depth = self.for_depth;
                self.for_depth = 0;
                let saved_loop_labels = std::mem::take(&mut self.loop_labels);
                let mut out = String::from("func(");
                out.push_str(&rendered_params);
                out.push_str(") ");
                // Declared results (SPEC EBNF FuncLit = "fn" [Signature]
                // Block): emitted verbatim so `fn() int {…}` lowers to
                // `func() int {…}`.
                if !results.is_empty() {
                    let rs = self.result_list_text(results);
                    out.push_str(&format!("({}) ", rs));
                }
                out.push_str("{\n");
                // Redirect statement output into a scratch buffer: the
                // literal renders inline, its body keeps normal indents.
                let main_body = std::mem::take(&mut self.body);
                let saved_loans = std::mem::take(&mut self.loan_frames);
                let saved_inherit = self.branch_inherit.take();
                let body_res = self.emit_block_body(&body.stmts);
                self.branch_inherit = saved_inherit;
                self.loan_frames = saved_loans;
                self.loop_labels = saved_loop_labels;
                let closure_body = std::mem::take(&mut self.body);
                self.body = main_body;
                self.fnlit_depth -= 1;
                self.for_depth = saved_for_depth;
                self.handler_writer = saved_writer;
                self.restore_scope(saved_scope);
                body_res?;
                out.push_str(&closure_body);
                for _ in 0..self.indent {
                    out.push('\t');
                }
                out.push('}');
                Ok(out)
            }

            Expr::Paren { inner, .. } => {
                if let Some(t) = self.expr_temps.get(&Self::expr_key(inner)) {
                    return Ok(t.clone());
                }
                let i = self.emit_expr(inner, 0)?;
                Ok(format!("({})", i))
            }
        }
    }

    fn lit_text(&self, k: &LitKind) -> String {
        match k {
            LitKind::Int(s) | LitKind::Float(s) => s.clone(),
            LitKind::Bool(b) => b.to_string(),
            LitKind::Str(s) => go_quote(s),
            LitKind::RawStr(s) => format!("`{}`", s),
            LitKind::Char(c) => format!("'\\U{:08x}'", *c as u32),
            LitKind::Nil => "nil".to_string(),
        }
    }

    /// `"total={total}"` → `fmt.Sprintf("total=%v", total)`.
    /// All-literal interps stay plain strings (no Sprintf, no allocation).
    pub fn emit_interp(&mut self, parts: &[InterpPart]) -> Result<String, EmitError> {
        let has_expr = parts.iter().any(|p| matches!(p, InterpPart::Expr(_)));
        if !has_expr {
            let mut s = String::new();
            for p in parts {
                if let InterpPart::Lit(l) = p {
                    s.push_str(l);
                }
            }
            return Ok(go_quote(&s));
        }
        let (format_str, args) = self.interp_parts(parts)?;
        self.use_package("fmt");
        Ok(format!(
            "fmt.Sprintf({}, {})",
            go_quote(&format_str),
            args.join(", ")
        ))
    }

    /// The interpolation's (format, args) pieces — shared by `emit_interp`
    /// (Sprintf wrap) and the fprintf/printf splice (Batch E), which need
    /// the pieces SEPARATELY so Fprintf's format param consumes them.
    fn interp_parts(&mut self, parts: &[InterpPart]) -> Result<(String, Vec<String>), EmitError> {
        let mut format_str = String::new();
        let mut args = Vec::new();
        for p in parts {
            match p {
                InterpPart::Lit(l) => {
                    format_str.push_str(&l.replace('%', "%%"));
                }
                InterpPart::Expr(x) => {
                    format_str.push_str("%v");
                    args.push(self.emit_expr(x, 0)?);
                }
            }
        }
        Ok((format_str, args))
    }

    /// A struct literal's keyed fields plus, for a user struct, the N5
    /// allocation of every map-carrying field the literal leaves at its
    /// zero value (`bag{}` → `bag{counts: map[string]int{}}`, issue #645).
    fn struct_lit_body(
        &mut self,
        ty: &TypeExpr,
        fields: &[FieldInit],
    ) -> Result<String, EmitError> {
        let body = self.field_inits(fields)?;
        let extra = self.n5_missing_fields(ty, fields);
        if extra.is_empty() {
            return Ok(body);
        }
        let mut parts = Vec::new();
        if !body.is_empty() {
            parts.push(body);
        }
        parts.extend(extra);
        Ok(parts.join(", "))
    }

    fn field_inits(&mut self, fields: &[FieldInit]) -> Result<String, EmitError> {
        let mut parts = Vec::new();
        for f in fields {
            let v = self.emit_expr(&f.value, 0)?;
            parts.push(format!("{}: {}", sanitize_ident(&f.name), v));
        }
        Ok(parts.join(", "))
    }

    /// `["a","b"]` with no element type: infer from the first item. Unknown
    /// shapes error cleanly — never a wrong emission.
    fn infer_slice_head(&mut self, items: &[Expr]) -> Result<String, EmitError> {
        let Some(first) = items.first() else {
            return Err(EmitError::unsupported(
                "empty `[]` literal — write `[]T{}` with the element type",
            ));
        };
        let elem = match first {
            Expr::Lit { kind, .. } => match kind {
                LitKind::Int(_) => "int",
                LitKind::Float(_) => "float64",
                LitKind::Str(_) | LitKind::RawStr(_) => "string",
                LitKind::Bool(_) => "bool",
                LitKind::Char(_) => "rune",
                LitKind::Nil => {
                    return Err(EmitError::unsupported("nil in an untyped slice literal"))
                }
            },
            Expr::StructLit { ty: Some(t), .. } => {
                return Ok(format!("[]{}", self.type_text(t)));
            }
            _ => {
                return Err(EmitError::unsupported(format!(
                    "untyped slice literal of `{}` — spell the element type",
                    first.render()
                )));
            }
        };
        Ok(format!("[]{}", elem))
    }

    // -----------------------------------------------------------------------
    // Calls — builtins, package specials, foreign methods
    // -----------------------------------------------------------------------

    /// Descriptor type of an expression, when the shape table knows it
    /// (Batch E chain resolution):
    ///
    /// ```text
    /// r              → r's binding: ForeignTy("http.request")
    /// r.url          → http.request.url's result: "url.url"
    /// r.url.query    → url.url.query's result: "url.values"
    /// r.url.query.get("a")  — the CALL resolves its callee's base likewise
    /// ```
    /// Returns None the moment a link is unknown — the caller falls back to
    /// the M4 heuristic. Never guesses.
    pub fn type_of_expr(&self, e: &Expr) -> Option<&'static str> {
        // Parentheses are semantically transparent to a type question:
        // `(hits).add(1)` must resolve exactly like `hits.add(1)` (#618).
        match Self::unwrap_parens(e) {
            Expr::Ident { name, .. } => {
                if let Some(BindingKind::ForeignTy(t)) = self.bindings.get(name) {
                    return Some(t);
                }
                // Batch I: a var/param whose declared type is shape-known
                // (`var hits atomic.int64`) resolves through var_ty.
                if let Some((ty, _)) = self.var_ty.get(name) {
                    if let Some((k, _)) = super::shapes::SHAPES.iter().find(|(k, _)| *k == ty) {
                        return Some(k);
                    }
                }
                None
            }
            Expr::Field { base, name, .. } => {
                if let Some(bt) = self.type_of_expr(base) {
                    if let Some(m) = lookup_member(bt, name) {
                        return m.result;
                    }
                }
                // Batch I: user-struct field typing — `stats.mu` resolves
                // through var_struct + struct_fields when the field's
                // declared type is shape-known (18: sync.mutex).
                if let Expr::Ident { name: vn, .. } = Self::unwrap_parens(base.as_ref()) {
                    if let Some(sn) = self.var_struct.get(vn) {
                        if let Some(fields) = self.struct_fields.get(sn) {
                            if let Some((_, fty)) = fields.iter().find(|(f, _)| f == name) {
                                if let Some((k, _)) =
                                    super::shapes::SHAPES.iter().find(|(k, _)| *k == fty)
                                {
                                    return Some(k);
                                }
                            }
                        }
                    }
                }
                None
            }
            // A zero-arg/method call's result type is the member's result.
            Expr::Call { callee, .. } => self.type_of_expr(callee),
            _ => None,
        }
    }

    /// Member spelling for a FIELD-position access, shape-table first.
    /// A MethodZero member in field position emits WITH parens — the hy
    /// surface spells `r.url.query`; Go requires `r.URL.Query()`.
    pub fn member_of_typed(&self, base: &Expr, name: &str) -> String {
        if let Some(t) = self.type_of_expr(base) {
            if let Some(m) = lookup_member(t, name) {
                return match m.kind {
                    MemberKind::Field => m.go.to_string(),
                    MemberKind::MethodZero => format!("{}()", m.go),
                    // A Method member in FIELD position (never called) is
                    // not expressible in Go; emit the bare name and let Go
                    // reject — a clean failure, never a wrong emission.
                    MemberKind::Method => m.go.to_string(),
                };
            }
        }
        self.member_name(base, name)
    }

    fn unwrap_parens(mut e: &Expr) -> &Expr {
        while let Expr::Paren { inner, .. } = e {
            e = inner;
        }
        e
    }

    /// A selector's base never needs parentheses when its inner expression is
    /// itself a primary expression: `(hits).add(1)` and `(mux).handle(...)`
    /// are the same accesses as `hits.add(1)` / `mux.Handle(...)`. For
    /// anything else (`(*p).f`, `(a + b).f`) the parentheses are load-bearing,
    /// so they are kept. Same whitelist the `.pop()` lowering already uses.
    fn selector_base(e: &Expr) -> &Expr {
        let peeled = Self::unwrap_parens(e);
        if matches!(
            peeled,
            Expr::Ident { .. } | Expr::Field { .. } | Expr::Index { .. }
        ) {
            peeled
        } else {
            e
        }
    }

    fn is_user_struct_expr(&self, e: &Expr) -> bool {
        match Self::unwrap_parens(e) {
            Expr::Ident { name, .. } => self.var_struct.contains_key(name),
            Expr::StructLit { .. } => true,
            Expr::AddrOf { operand, .. } => self.is_user_struct_expr(operand),
            Expr::Field { base, name, .. } => {
                if let Expr::Ident { name: vn, .. } = Self::unwrap_parens(base) {
                    if let Some(sn) = self.var_struct.get(vn) {
                        if let Some(fields) = self.struct_fields.get(sn) {
                            if let Some((_, fty)) = fields.iter().find(|(f, _)| f == name) {
                                return self.struct_fields.contains_key(fty.as_str());
                            }
                        }
                    }
                }
                false
            }
            _ => false,
        }
    }

    pub(crate) fn emit_call(
        &mut self,
        callee: &Expr,
        args: &[CallArg],
    ) -> Result<String, EmitError> {
        // --- make(...): the first argument is a TYPE (bug-#2 fix) ---
        if matches!(callee, Expr::Ident { name, .. } if name == "make") {
            return self.emit_make(args);
        }
        // --- new(T) whose zero value allocates maps (N5, issue #645) ---
        let is_new = matches!(callee, Expr::Ident { name, .. } if name == "new");
        if is_new && !self.local_vars.contains("new") && !self.fn_shapes.contains_key("new") {
            if let Some(alloc) = self.n5_new(args) {
                return Ok(alloc);
            }
        }

        // --- package-qualified calls ---
        if let Expr::Field { base, name, .. } = callee {
            if let Expr::Ident { name: pkg, .. } = Self::unwrap_parens(base.as_ref()) {
                // Known to the checker, unshipped by the runtime (the
                // audit's flate2 row): the honest refusal must come BEFORE
                // the package-root gate — an unbound crate is not a Go
                // package root, so its call used to fall through this whole
                // block, emit verbatim, and die in the toolchain with
                // `undefined: flate2`.
                if !self.local_vars.contains(pkg) && lookup_unbound_crate(pkg).is_some() {
                    return Err(EmitError::unbound_crate(pkg));
                }
                if self.is_unshadowed_pkg_root(pkg) {
                    return self.emit_pkg_call(pkg, name, args);
                }
            }
        }

        // --- method calls on foreign bindings normalize (M4 + shapes) ---
        // Shape-table first (Batch E): a typed chain resolves
        // field-vs-method exactly (`r.url.query.get("a")` →
        // `r.URL.Query().Get("a")`); generic Foreign falls back to M4.
        if let Expr::Field { base, name, .. } = callee {
            let base = Self::selector_base(base);
            if let Some(bt) = self.type_of_expr(base) {
                if let Some(m) = lookup_member(bt, name) {
                    if m.kind == MemberKind::Method {
                        let b = self.emit_expr(base, ATOM)?;
                        let mut parts = Vec::new();
                        for a in args {
                            parts.push(self.emit_call_arg(a)?);
                        }
                        return Ok(format!("{}.{}({})", b, m.go, parts.join(", ")));
                    }
                    // MethodZero called WITH args (or a Field called) is a
                    // surface error — fall through; Go rejects Field(args)
                    // — a clean failure, never a wrong emission.
                }
            }
            if let Expr::Ident { name: root, .. } = base {
                if let Some(BindingKind::Foreign) = self.bindings.get(root) {
                    let b = self.emit_expr(base, ATOM)?;
                    let mut parts = Vec::new();
                    for a in args {
                        parts.push(self.emit_call_arg(a)?);
                    }
                    return Ok(format!("{}.{}({})", b, to_go_name(name), parts.join(", ")));
                }
            }
        }

        // --- slice `.pop()` → prelude helper (example 08's stack shape) ---
        if let Expr::Field { base, name, .. } = callee {
            if name == "pop" && args.is_empty() && !self.is_user_struct_expr(base) {
                let peeled = Self::unwrap_parens(base);
                if matches!(
                    peeled,
                    Expr::Ident { .. } | Expr::Field { .. } | Expr::Index { .. }
                ) {
                    let b = self.emit_expr(peeled, ATOM)?;
                    return Ok(format!("hy_pop(&{})", b));
                }
                return Err(EmitError::unsupported(
                    "`.pop()` on a non-addressable base (requires a slice variable, field, or element)",
                ));
            }
        }

        // --- associated-fn calls: `lru_cache.new[string, int](2)` →
        //     `lru_cache_new[string, int](2)` (Go has no dotted idents) ---
        if let Expr::Field { base, name, .. } = callee {
            if let Expr::Ident { name: root, .. } = Self::unwrap_parens(base.as_ref()) {
                if !self.is_unshadowed_pkg_root(root) && !self.bindings.contains_key(root) {
                    let (head, type_args) = split_type_args(name);
                    let full = format!("{}.{}", root, head);
                    if self.assoc_fns.contains(&full) {
                        let translated_ta = type_args
                            .map(|ta| self.translate_type_args_suffix(ta))
                            .unwrap_or_default();
                        let mut parts = Vec::new();
                        for a in args {
                            parts.push(self.emit_call_arg(a)?);
                        }
                        return Ok(format!(
                            "{}_{}{}({})",
                            root,
                            head,
                            translated_ta,
                            parts.join(", ")
                        ));
                    }
                }
            }
        }

        let mut bare_callee = callee;
        while let Expr::Paren { inner, .. } = bare_callee {
            bare_callee = inner;
        }
        if matches!(bare_callee, Expr::Ident { name, .. } if name == "make") {
            return self.emit_make(args);
        }
        if matches!(bare_callee, Expr::Ident { name, .. } if name == "new") && args.len() == 1 {
            return self.emit_new(&args[0]);
        }

        // --- a SPEC primitive conversion: `u8(300)` -> `uint8(300)`.
        //     SPEC-LANGUAGE line 1991 states `b := u8(300) // expect: 44`
        //     as an executable test-claim (§ "Examples are tests"), but the
        //     name used to fall through to the verbatim emission below and
        //     die in Go as `undefined: u8`. Issue #147. ---
        if let Expr::Ident { name, .. } = callee {
            if let Some(go) = super::shapes::go_primitive_name(name) {
                if args.len() != 1 {
                    return Err(EmitError::unsupported(format!(
                        "`{}` conversion takes exactly one argument",
                        name
                    )));
                }
                // A CONSTANT operand that does not fit the target has to
                // be folded here: Go rejects `uint8(300)` outright (the
                // truncation Go does is only for a variable operand), yet
                // SPEC-LANGUAGE line 1991 asserts `u8(300) == 44` as an
                // executable test-claim. The fold computes that same
                // two's-complement truncation at emit time.
                if let Some(folded) = super::constconv::fold_constant(&args[0].value, name) {
                    return Ok(format!("{}({})", go, folded));
                }
                let a = self.emit_call_arg(&args[0])?;
                return Ok(format!("{}({})", go, a));
            }
            if name == "len" && args.len() == 1 && self.is_set_expr(&args[0].value) {
                let a = self.emit_call_arg(&args[0])?;
                return Ok(format!("{}.len()", a));
            }
            if name == "delete" && args.len() == 2 && self.is_set_expr(&args[0].value) {
                let s = self.emit_call_arg(&args[0])?;
                let v = self.emit_call_arg(&args[1])?;
                return Ok(format!("{}.remove({})", s, v));
            }
        }

        // --- a slice type-as-value CALLED = a conversion: `[]byte("abc")`
        //     parses as SliceLit{elem, items: []} + a postfix call (the
        //     standing type-as-value encoding) and used to fall into the
        //     generic path below as `[]byte{}("abc")` — go:
        //     `invalid operation: cannot call []byte{}` — so even the
        //     legal Go conversion `[]byte(s)` never built (issue #228
        //     family C). Emit the conversion form directly. ---
        if let Expr::SliceLit {
            elem: Some(t),
            items,
            ..
        } = callee
        {
            if items.is_empty() && callee.as_array_lit().is_none() {
                let mut parts = Vec::new();
                for a in args {
                    parts.push(self.emit_call_arg(a)?);
                }
                return Ok(format!(
                    "[]{}({})",
                    self.type_text(t),
                    parts.join(", ")
                ));
            }
        }

        // --- everything else: callee(args...) ---
        // An addressable argument auto-addresses at a pointer param (Batch
        // I, corpus 09: `scale_then_register(e, …)` with `e *env` — Go's
        // own method-receiver auto-address precedent, pinned by the
        // pre-registered corpus).
        let mut ptr_params: Vec<bool> = Vec::new();
        if let Expr::Ident { name, .. } = callee {
            let (fn_head, _) = split_type_args(name);
            if let Some((_, _, _, params)) = self.fn_shapes.get(fn_head) {
                ptr_params = params.iter().map(|p| p.is_some()).collect();
            }
        }
        let c = self.emit_expr(callee, ATOM)?;
        let mut parts = Vec::new();
        for (i, a) in args.iter().enumerate() {
            let t = self.emit_call_arg(a)?;
            let wants_ptr = ptr_params.get(i).copied().unwrap_or(false);
            let already_ptr = self.expr_is_pointer(&a.value);
            let addressable = matches!(
                a.value,
                Expr::Ident { .. } | Expr::Field { .. } | Expr::Index { .. }
            );
            if wants_ptr && !already_ptr && addressable {
                parts.push(format!("&{}", t));
            } else {
                parts.push(t);
            }
        }
        Ok(format!("{}({})", c, parts.join(", ")))
    }

    fn emit_call_arg(&mut self, a: &CallArg) -> Result<String, EmitError> {
        let t = self.emit_expr(&a.value, 0)?;
        if a.spread {
            Ok(format!("{}...", t))
        } else {
            Ok(t)
        }
    }
    /// `make(chan job, 16)` / `make([]byte, 64*1024)` / `make(map[string]int)`.
    /// The parser encodes the type argument as an EMPTY composite literal of
    /// that type — rendering it as a value (`job{}`) was MVP-0 bug #2.
    fn emit_make(&mut self, args: &[CallArg]) -> Result<String, EmitError> {
        let Some(first) = args.first() else {
            return Err(EmitError::unsupported("`make` with no type argument"));
        };
        let mut first_val = &first.value;
        while let Expr::Paren { inner, .. } = first_val {
            first_val = inner;
        }
        let ty_text = match first_val {
            Expr::StructLit {
                ty: Some(t),
                fields,
                ..
            } if fields.is_empty() => self.type_text(t),
            Expr::SliceLit {
                elem: Some(t),
                items,
                ..
            } if items.is_empty() && first_val.as_array_lit().is_none() => {
                format!("[]{}", self.type_text(t))
            }
            Expr::MapLit {
                key,
                value,
                entries,
                ..
            } if entries.is_empty() => {
                format!("map[{}]{}", self.type_text(key), self.type_text(value))
            }
            Expr::Ident { name, .. } => name.clone(),
            other => {
                return Err(EmitError::unsupported(format!(
                    "`make` first argument must be a type; got value `{}`",
                    other.render()
                )));
            }
        };
        let mut parts = vec![ty_text];
        for a in &args[1..] {
            parts.push(self.emit_call_arg(a)?);
        }
        Ok(format!("make({})", parts.join(", ")))
    }

    fn emit_new(&mut self, arg: &CallArg) -> Result<String, EmitError> {
        let mut val = &arg.value;
        while let Expr::Paren { inner, .. } = val {
            val = inner;
        }
        let ty_text = match val {
            Expr::StructLit {
                ty: Some(t),
                fields,
                ..
            } if fields.is_empty() => self.type_text(t),
            Expr::SliceLit {
                elem: Some(t),
                items,
                ..
            } if items.is_empty() => {
                if val.as_array_lit().is_some() {
                    self.type_text(t)
                } else {
                    format!("[]{}", self.type_text(t))
                }
            }
            Expr::MapLit {
                key,
                value,
                entries,
                ..
            } if entries.is_empty() => format!("map[{}]{}", self.type_text(key), self.type_text(value)),
            Expr::Ident { name, .. } => super::shapes::go_primitive_name(name)
                .unwrap_or(name.as_str())
                .to_string(),
            other => self.emit_expr(other, 0)?,
        };
        Ok(format!("new({})", ty_text))
    }

    /// The rust-crate call lowering (M6): `sha3.digest256(w)` →
    /// `sha3binding.Digest256([]byte(w))`; `sha3.stream_checksum(n, seed=7)`
    /// → `sha3binding.StreamChecksum(n, 7)` (named args place by the
    /// descriptor's param order — Go has no named args). Unknown fn or an
    /// unknown arg name is a CLEAN error naming the descriptor surface.
    fn emit_rust_call(
        &mut self,
        crate_desc: &'static RustCrateDesc,
        name: &str,
        args: &[CallArg],
    ) -> Result<String, EmitError> {
        let Some(f) = crate_desc.fns.iter().find(|f| f.hy == name) else {
            return Err(EmitError::unsupported(format!(
                "rust/{}: `{}` is not in the pinned binding surface (available: {})",
                crate_desc.hy,
                name,
                crate_desc
                    .fns
                    .iter()
                    .map(|f| f.hy)
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        };
        self.use_package(crate_desc.binding_root);
        // Slot the arguments: named → by descriptor position; positional →
        // first free slot left-to-right.
        let mut slots: Vec<Option<String>> = vec![None; f.params.len()];
        let mut next_free = 0usize;
        for a in args {
            let text = if f.bytes_coerce {
                format!("[]byte({})", self.emit_expr(&a.value, 0)?)
            } else {
                self.emit_expr(&a.value, 0)?
            };
            match &a.name {
                Some(n) => {
                    let i = f
                        .params
                        .iter()
                        .position(|p| *p == n.as_str())
                        .ok_or_else(|| {
                            EmitError::unsupported(format!(
                                "rust/{}.{}: no parameter named `{}` (params: {})",
                                crate_desc.hy,
                                name,
                                n,
                                f.params.join(", ")
                            ))
                        })?;
                    if slots[i].is_some() {
                        return Err(EmitError::unsupported(format!(
                            "rust/{}.{}: parameter `{}` bound twice",
                            crate_desc.hy, name, n
                        )));
                    }
                    slots[i] = Some(text);
                }
                None => {
                    while next_free < slots.len() && slots[next_free].is_some() {
                        next_free += 1;
                    }
                    if next_free >= slots.len() {
                        return Err(EmitError::unsupported(format!(
                            "rust/{}.{}: too many arguments (descriptor has {})",
                            crate_desc.hy,
                            name,
                            f.params.len()
                        )));
                    }
                    slots[next_free] = Some(text);
                    next_free += 1;
                }
            }
        }
        if slots.iter().any(|s| s.is_none()) {
            return Err(EmitError::unsupported(format!(
                "rust/{}.{}: missing arguments (params: {})",
                crate_desc.hy,
                name,
                f.params.join(", ")
            )));
        }
        Ok(format!(
            "{}.{}({})",
            crate_desc.binding_root,
            f.go,
            slots
                .into_iter()
                .map(|s| s.unwrap())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }

    fn emit_pkg_call(
        &mut self,
        pkg: &str,
        name: &str,
        args: &[CallArg],
    ) -> Result<String, EmitError> {
        // --- rust crate bindings (M6, Phase 2): `sha3.digest256(w)` →
        //     `sha3binding.Digest256([]byte(w))`. The binding descriptor
        //     owns param order (named args place by name — Go has none). ---
        if let Some(crate_desc) = lookup_rust_crate(pkg) {
            return self.emit_rust_call(crate_desc, name, args);
        }

        // Known to the checker, unshipped by the runtime (the audit's
        // flate2 row): refuse HERE, not at go build with `undefined: flate2`
        // — a call without an explicit import line used to reach this
        // fallthrough, get recorded as a plain Go package use, and die in
        // the toolchain with a message about std packages.
        if lookup_unbound_crate(pkg).is_some() {
            return Err(EmitError::unbound_crate(pkg));
        }

        if pkg != "errors" || name != "wrap" {
            self.use_package(pkg);
        }

        // --- conversion targets (Batch E): `http.handler(fn(w, r) { … })`
        //     → `http.HandlerFunc(func(w http.ResponseWriter, r *http.
        //     Request) { … })`. Untyped fn-lit params take the TARGET's
        //     types (pending_fnlit_conv); `?` inside takes the E11
        //     handler contract (handler_writer). ---
        if let Some(conv) = lookup_conversion(&format!("{}.{}", pkg, name)) {
            if args.len() != 1 {
                return Err(EmitError::unsupported(format!(
                    "{}.{} conversion takes exactly one function literal",
                    pkg, name
                )));
            }
            if !matches!(args[0].value, Expr::FnLit { .. }) {
                return Err(EmitError::unsupported(format!(
                    "{}.{} conversion argument must be a function literal",
                    pkg, name
                )));
            }
            self.pending_fnlit_conv = Some(conv);
            let lit = self.emit_expr(&args[0].value, ATOM)?;
            self.pending_fnlit_conv = None;
            return Ok(format!("{}({})", conv.go, lit));
        }

        // --- the httptest smoke client is a COMPOSITE lowering (Batch E):
        //     `httptest.get(mux, path)` is the in-process recorder dance.
        //     It carries (code, body, error) — only legal at a `?` binding
        //     (emit_try_define expands it statement-wise); a bare call
        //     errors cleanly here. ---
        if pkg == "httptest" && (name == "get" || name == "post") {
            return Err(EmitError::unsupported(
                "httptest.get/post is a (code, body, error) call — bind it with `?` \
                 (the E3/E6 desugar expands the recorder dance)",
            ));
        }

        if let Some(special) = special_member(pkg, name) {
            return match special {
                MemberSpecial::Var(text) => {
                    if args.is_empty() {
                        Ok(text.to_string())
                    } else {
                        Err(EmitError::unsupported(format!(
                            "{}.{} takes no arguments",
                            pkg, name
                        )))
                    }
                }
                MemberSpecial::Renamed(go_name) => {
                    let mut parts = Vec::new();
                    for a in args {
                        parts.push(self.emit_call_arg(a)?);
                    }
                    Ok(format!("{}.{}({})", pkg, go_name, parts.join(", ")))
                }
                MemberSpecial::StderrCall(go_name) => {
                    self.use_package("os");
                    let mut parts = vec!["os.Stderr".to_string()];
                    for a in args {
                        parts.push(self.emit_call_arg(a)?);
                    }
                    Ok(format!("{}.{}({})", pkg, go_name, parts.join(", ")))
                }
                MemberSpecial::Duration(unit) => {
                    self.use_package("time");
                    if args.len() != 1 {
                        return Err(EmitError::unsupported(format!(
                            "time.{} takes exactly one argument",
                            name
                        )));
                    }
                    let n = self.emit_call_arg(&args[0])?;
                    Ok(format!("time.Duration({}) * time.{}", n, unit))
                }
                MemberSpecial::ErrorsWrap => {
                    self.use_package("fmt");
                    if args.len() != 2 {
                        return Err(EmitError::unsupported(
                            "errors.wrap takes exactly two arguments (err, msg)",
                        ));
                    }
                    let (err_arg, msg_arg) = if args[0].name.as_deref() == Some("msg")
                        || args[1].name.as_deref() == Some("err")
                    {
                        (&args[1], &args[0])
                    } else {
                        (&args[0], &args[1])
                    };
                    let err = self.emit_call_arg(err_arg)?;
                    let msg = self.emit_call_arg(msg_arg)?;
                    Ok(format!("fmt.Errorf(\"%s: %w\", {}, {})", msg, err))
                }
            };
        }

        // fmt.print on a byte buffer → os.Stdout.Write (a byte-slice dump
        // prints `[104 105]`, not the bytes — the reason the hint exists).
        if pkg == "fmt" && name == "print" && args.len() == 1 {
            if let Expr::Ident { name: b, .. } = &args[0].value {
                if let Some(BindingKind::Bytes) = self.bindings.get(b) {
                    self.use_package("os");
                    let t = self.emit_call_arg(&args[0])?;
                    return Ok(format!("os.Stdout.Write({})", t));
                }
            }
        }

        // Interpolation in a FORMAT-carrying call (Batch E, example 06):
        // `fmt.fprintf(w, "{a}+{b}={a+b}\n")` must splice the interp's
        // arguments into the call — `fmt.Fprintf(w, "%v+%v=%v\n", a, b, a+b)`.
        // (Wrapping the whole interp in Sprintf would print the %v's as
        // literals — Fprintf's format param consumes them.)
        if (pkg == "fmt" && (name == "fprintf" || name == "printf")) && !args.is_empty() {
            if let Expr::Interp { parts, .. } = &args[args.len() - 1].value {
                let (fmt_str, interp_args) = self.interp_parts(parts)?;
                let mut call_args = Vec::new();
                for a in &args[..args.len() - 1] {
                    call_args.push(self.emit_call_arg(a)?);
                }
                call_args.push(go_quote(&fmt_str));
                call_args.extend(interp_args);
                return Ok(format!(
                    "{}.{}({})",
                    pkg,
                    to_go_name(name),
                    call_args.join(", ")
                ));
            }
        }

        let mut parts = Vec::new();
        for a in args {
            parts.push(self.emit_call_arg(a)?);
        }
        Ok(format!(
            "{}.{}({})",
            pkg,
            to_go_name(name),
            parts.join(", ")
        ))
    }
}

/// `new[string, int]` → ("new", Some("[string, int]")).
fn split_type_args(name: &str) -> (&str, Option<&str>) {
    match name.find('[') {
        Some(i) => (&name[..i], Some(&name[i..])),
        None => (name, None),
    }
}

// ---------------------------------------------------------------------------
// Total-rendering helper for type positions (never fail, never panic)
// ---------------------------------------------------------------------------

impl Emitter {
    /// Expression text for contexts that cannot hold a `?`/match (array
    /// lengths, etc.). Falls back to the AST's lossy `render()` — a
    /// description, never a wrong emission.
    pub fn expr_text_static(&mut self, e: &Expr) -> String {
        match self.emit_expr(e, 0) {
            Ok(t) => t,
            Err(err) => format!("{} /* unsupported: {} */", e.render(), err.message),
        }
    }
}

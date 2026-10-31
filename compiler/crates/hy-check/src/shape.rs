//! shape.rs — the lightweight TypeShape inference (NOT a type system).
//!
//! Resolves a `TypeShape` from a declared `TypeExpr` (params, `var`/`let`
//! ascriptions) or from an initializer expression (defines, loop variables),
//! consulting the file-level struct/fn/const tables and the builtin foreign
//! tables in foreign.rs. Everything unresolvable degrades to `Unknown`,
//! which is FAIL-CLOSED at spawn/send boundaries (O4.9): an unproven shape
//! may not silently cross (HYB-E3005) — but remains legal inside a function,
//! where no boundary is involved. Fewer unknowns is still the goal: the
//! inference below deliberately widens coverage (closures, option, chan
//! elements) so honest programs classify without ceremony.

use crate::foreign;
use crate::model::{Checker, FnState, TypeShape};
use hy_syntax::{BinOp, CallArg, Expr, TypeExpr};

impl<'a> Checker<'a> {
    const MAX_TYPE_RESOLUTION_DEPTH: usize = 8;

    // -----------------------------------------------------------------------
    // Type shape extraction
    // -----------------------------------------------------------------------

    pub(crate) fn shape_of_type(&self, ty: &TypeExpr) -> TypeShape {
        self.shape_of_type_depth(ty, 0)
    }

    fn shape_of_type_depth(&self, ty: &TypeExpr, depth: usize) -> TypeShape {
        if depth > Self::MAX_TYPE_RESOLUTION_DEPTH {
            return TypeShape::Unknown;
        }
        match ty {
            TypeExpr::Named { name, .. } => self.named_shape_depth(name, depth),
            TypeExpr::Pointer { to, .. } => {
                TypeShape::Pointer(Box::new(self.shape_of_type_depth(to, depth + 1)))
            }
            TypeExpr::Slice { elem, .. } => {
                TypeShape::Slice(Box::new(self.shape_of_type_depth(elem, depth + 1)))
            }
            TypeExpr::Array { elem, .. } => {
                TypeShape::Array(Box::new(self.shape_of_type_depth(elem, depth + 1)))
            }
            TypeExpr::Map { key, value, .. } => TypeShape::Map(
                Box::new(self.shape_of_type_depth(key, depth + 1)),
                Box::new(self.shape_of_type_depth(value, depth + 1)),
            ),
            TypeExpr::Chan { elem, .. } => {
                TypeShape::Chan(Box::new(self.shape_of_type_depth(elem, depth + 1)))
            }
            // option[T] carries T by value in a struct { v T; ok bool } — its
            // sendability IS the payload's sendability (option[int] crosses,
            // option[map] does not — the aliased map inside decides).
            TypeExpr::Option { elem, .. } => self.shape_of_type_depth(elem, depth + 1),
            // fn-typed values are closures: captures make them unsendable at
            // boundaries (Tier 0 — the A5 perimeter).
            TypeExpr::Fn { .. } => TypeShape::Closure,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn named_shape(&self, name: &str) -> TypeShape {
        self.named_shape_depth(name, 0)
    }

    fn named_shape_depth(&self, name: &str, depth: usize) -> TypeShape {
        if depth > Self::MAX_TYPE_RESOLUTION_DEPTH {
            return TypeShape::Unknown;
        }
        // Generic instantiation `lru_cache[k, v]` folds its type arguments
        // into the name string (TypeExpr has no type-argument node — the
        // parser contract gap noted in parse_type). The struct and alias
        // tables carry the BASE name, so resolve through the head
        // (`base_named_type_str`, the same fold `base_named_type` applies
        // for method receivers). Everything below keeps matching the full
        // name: for a non-generic name the base IS the name, so this is a
        // pure widening — a generic receiver like `fn (c *box[t]) m()`
        // used to classify as `Unknown` and now reaches the declared
        // struct's field shapes (issue #411: `for i, e in c.order` must
        // see `c.order` as the declared `[]k`, not an opaque Unknown).
        let base = Self::base_named_type_str(name);
        // User structs in package scope take precedence over predeclared types.
        if self.structs.contains_key(base) {
            return TypeShape::Struct(Some(base.to_string()));
        }
        // User type aliases take precedence over predeclared types.
        if let Some(target) = self.type_aliases.get(base) {
            return self.shape_of_type_depth(target, depth + 1);
        }
        // #884: the T9 prelude's `set[T]` lowers to a struct wrapping a Go
        // map (`type set[t comparable] struct { m map[t]struct{} }`), so a
        // shallow copy shares that map exactly like any other map-carrying
        // struct. Without this arm `set[int]` resolved to `Unknown` and
        // `shape_may_share` said no, which let O5.2/O5.4/T22 be bypassed by
        // copying first and moving the original.
        if base == "set" {
            return TypeShape::Struct(Some(base.to_string()));
        }
        match name {
            "int" | "i8" | "int8" | "i16" | "int16" | "i32" | "int32" | "i64" | "int64" | "u8"
            | "uint8" | "u16" | "uint16" | "u32" | "uint32" | "u64" | "uint64" | "usize"
            | "isize" | "uintptr" | "float32" | "float64" | "f32" | "f64" | "bool" | "byte"
            | "rune" | "char" => TypeShape::Primitive,
            "string" => TypeShape::Str,
            _ => {
                if let Some(shape) = foreign::foreign_named_type(name) {
                    shape
                } else {
                    TypeShape::Unknown
                }
            }
        }
    }

    /// May a value of this shape share mutable GC storage with a shallow Go
    /// copy of it? True for reference types (map/slice/pointer), and — the
    /// #27 gap — transitively true for a struct or array that CARRIES a
    /// reference-typed field/element: Go copies such a value by value, but
    /// the reference interior is shared. Used by the freeze rule's alias
    /// tracking so `h2 := h1; task f(move h1)` freezes `h2` too.
    ///
    /// `Unknown` is deliberately NOT may-share: an unresolvable shape makes
    /// no aliasing claim (the boundary rules still fail closed on it). The
    /// depth guard is conservative (deep/recursive struct types claim
    /// sharing) and cannot loop on Go's acyclic value types.
    pub(crate) fn shape_may_share(&self, shape: &TypeShape) -> bool {
        self.shape_may_share_at(shape, 0)
    }

    fn shape_may_share_at(&self, shape: &TypeShape, depth: usize) -> bool {
        if depth > 16 {
            return true;
        }
        if shape.is_reference_alias()
            || shape.is_handle()
            || matches!(shape, TypeShape::Closure)
        {
            return true;
        }
        match shape {
            TypeShape::Array(elem) => self.shape_may_share_at(elem, depth + 1),
            // #884: `set[T]` is a prelude struct over a Go map, but it has no
            // entry in `self.structs` (the prelude is emitted, not declared),
            // so the field walk below returned false for it. A shallow copy of
            // a set shares the underlying map; say so.
            TypeShape::Struct(Some(name)) if name == "set" => true,
            TypeShape::Struct(Some(name)) => self
                .structs
                .get(name)
                .map(|fields| {
                    fields
                        .iter()
                        .any(|(_, ty)| self.shape_may_share_at(&self.shape_of_type(ty), depth + 1))
                })
                .unwrap_or(false),
            _ => false,
        }
    }

    /// Does a value of this shape carry a boundary handle (`OwnHandle` or
    /// `BorrowHandle`) directly or inside a composite container? Used to
    /// distinguish handle-preserving expressions (`doc2 := same_doc(doc)`,
    /// `h := holder{doc: doc}`, `items := doc.array("items")?`) from boundary
    /// calls that return a fresh Layer-1 Go copy (`out := enc.finish()?`
    /// returning `[]byte`, O9).
    pub(crate) fn shape_carries_handle(&self, shape: &TypeShape) -> bool {
        self.shape_carries_handle_at(shape, 0)
    }

    fn shape_carries_handle_at(&self, shape: &TypeShape, depth: usize) -> bool {
        if depth > 16 {
            return true;
        }
        if shape.is_handle() {
            return true;
        }
        match shape {
            TypeShape::Slice(e)
            | TypeShape::Array(e)
            | TypeShape::Pointer(e)
            | TypeShape::Chan(e) => self.shape_carries_handle_at(e, depth + 1),
            TypeShape::Map(k, v) => {
                self.shape_carries_handle_at(k, depth + 1)
                    || self.shape_carries_handle_at(v, depth + 1)
            }
            TypeShape::Struct(Some(name)) => self
                .structs
                .get(name)
                .map(|fields| {
                    fields.iter().any(|(_, ty)| {
                        self.shape_carries_handle_at(&self.shape_of_type(ty), depth + 1)
                    })
                })
                .unwrap_or(false),
            _ => false,
        }
    }

    /// Does this struct decl guard something with a `sync.*` lock? (Used for
    /// the contested-04 note: "Tier 0 does not credit locks at spawn sites".)
    pub(crate) fn struct_has_mutex_field(&self, sname: &str) -> bool {
        self.structs
            .get(sname)
            .map(|fields| {
                fields.iter().any(|(_, ty)| {
                    matches!(
                        self.shape_of_type(ty),
                        TypeShape::ForeignOpaque(ref t) if t.starts_with("sync.")
                    )
                })
            })
            .unwrap_or(false)
    }

    // -----------------------------------------------------------------------
    // From expressions
    // -----------------------------------------------------------------------

    pub(crate) fn shape_of_expr(&self, st: &FnState, expr: &Expr) -> TypeShape {
        match expr {
            Expr::Ident { name, .. } => self
                .lookup_binding(st, name)
                .map(|b| b.shape.clone())
                .unwrap_or(TypeShape::Unknown),
            Expr::Lit { kind, .. } => match kind {
                hy_syntax::LitKind::Int(_)
                | hy_syntax::LitKind::Float(_)
                | hy_syntax::LitKind::Bool(_)
                | hy_syntax::LitKind::Char(_) => TypeShape::Primitive,
                hy_syntax::LitKind::Str(_) | hy_syntax::LitKind::RawStr(_) => TypeShape::Str,
                hy_syntax::LitKind::Nil => TypeShape::Unknown,
            },
            Expr::Interp { .. } => TypeShape::Str,
            Expr::Binary { op, lhs, rhs, .. } => {
                // string concatenation stays a string (`const corpus = "a" + "b"`)
                if matches!(op, BinOp::Add)
                    && (self.shape_of_expr(st, lhs) == TypeShape::Str
                        || self.shape_of_expr(st, rhs) == TypeShape::Str)
                {
                    TypeShape::Str
                } else {
                    TypeShape::Primitive
                }
            }
            Expr::Unary { op, operand, .. } => match op {
                hy_syntax::UnOp::Addr => {
                    TypeShape::Pointer(Box::new(self.shape_of_expr(st, operand)))
                }
                _ => TypeShape::Primitive,
            },
            Expr::AddrOf { operand, .. } => {
                TypeShape::Pointer(Box::new(self.shape_of_expr(st, operand)))
            }
            Expr::Move { operand, .. }
            | Expr::Try { operand, .. }
            | Expr::Paren { inner: operand, .. } => self.shape_of_expr(st, operand),
            Expr::Receive { chan, .. } => match self.shape_of_expr(st, chan) {
                TypeShape::Chan(e) => *e,
                _ => TypeShape::Unknown,
            },
            Expr::Index { base, .. } => match self.shape_of_expr(st, base) {
                TypeShape::Map(_, v) => *v,
                TypeShape::Slice(e) | TypeShape::Array(e) => *e,
                _ => TypeShape::Unknown,
            },
            Expr::Slice { base, .. } => match self.shape_of_expr(st, base) {
                TypeShape::Slice(e) | TypeShape::Array(e) => TypeShape::Slice(e),
                TypeShape::Str => TypeShape::Str,
                _ => TypeShape::Unknown,
            },
            Expr::Field { base, name, .. } => {
                let base_shape = self.shape_of_expr(st, base);
                self.field_shape(&base_shape, name)
            }
            Expr::Call { callee, args, .. } => self.call_shape(st, callee, args),
            // A struct literal written through a type alias
            // (`type holder_alias = holder; h := holder_alias{m: m1}`) names the
            // alias, not the struct. Reporting the alias name made
            // `shape_may_share` miss it (`self.structs` is keyed by the real
            // name), so `h2 := h` recorded no may-share edge and moving `h` into
            // a task left `h2` free to mutate the same map (#385). Resolve the
            // alias first; predeclared spellings (`make(chan T, n)`,
            // `make([]T, n)`) keep their own shape through `shape_of_type`.
            Expr::StructLit { ty: Some(t), .. } => match self.shape_of_type(t) {
                shape @ TypeShape::Struct(_) => shape,
                _ => TypeShape::Struct(match t {
                    TypeExpr::Named { name, .. } => Some(name.clone()),
                    _ => None,
                }),
            },
            Expr::StructLit { ty: None, .. } => TypeShape::Struct(None),
            Expr::MapLit { key, value, .. } => TypeShape::Map(
                Box::new(self.shape_of_type(key)),
                Box::new(self.shape_of_type(value)),
            ),
            Expr::SliceLit { elem, items, .. } => {
                if let Some((_, inner_elem, _)) = expr.as_array_lit() {
                    return TypeShape::Array(Box::new(self.shape_of_type(inner_elem)));
                }
                TypeShape::Slice(Box::new(match elem {
                    Some(t) => self.shape_of_type(t),
                    None => items
                        .first()
                        .map(|i| self.shape_of_expr(st, i))
                        .unwrap_or(TypeShape::Unknown),
                }))
            }
            // closures are unsendable values in Tier 0 (the A5 perimeter);
            // classify_value rejects FnLit literals before shape lookup —
            // this arm covers fn-typed bindings and fn-typed fields
            Expr::FnLit { .. } => TypeShape::Closure,
        }
    }

    /// Field type through (a pointer to) a declared struct.
    pub(crate) fn field_shape(&self, base: &TypeShape, field: &str) -> TypeShape {
        match base {
            TypeShape::Struct(Some(sname)) => self
                .structs
                .get(sname)
                .and_then(|fs| fs.iter().find(|(n, _)| n == field))
                .map(|(_, t)| self.shape_of_type(t))
                .unwrap_or(TypeShape::Unknown),
            TypeShape::Pointer(inner) => self.field_shape(inner, field),
            _ => TypeShape::Unknown,
        }
    }

    fn call_shape(&self, st: &FnState, callee: &Expr, args: &[CallArg]) -> TypeShape {
        match crate::model::unwrap_parens(callee) {
            Expr::FnLit { results, .. } => results
                .first()
                .map(|r| self.shape_of_type(&r.ty))
                .unwrap_or(TypeShape::Unknown),
            Expr::Ident { name, .. } => {
                // Local bindings shadow top-level functions and builtins.
                if let Some(b) = self.lookup_binding(st, name) {
                    if let Some(TypeExpr::Fn { results, .. }) = &b.ty {
                        return results
                            .first()
                            .map(|r| self.shape_of_type(r))
                            .unwrap_or(TypeShape::Unknown);
                    }
                    return TypeShape::Unknown;
                }
                match name.as_str() {
                    // `make(chan T, n)` — defensive against parser spellings of
                    // the type argument; defaults Unknown (fail-closed at
                    // boundaries, O4.9). See the TECH DEBT note in foreign.rs.
                    "make" => self.make_shape(args),
                    "append" | "some" => args
                        .first()
                        .map(|a| self.shape_of_expr(st, &a.value))
                        .unwrap_or(TypeShape::Unknown),
                    "len" | "cap" | "min" | "max" | "int" | "int8" | "int16" | "int32"
                    | "int64" | "uint" | "uint8" | "uint16" | "uint32" | "uint64" | "uintptr"
                    | "float32" | "float64" | "bool" | "byte" | "rune" => TypeShape::Primitive,
                    "string" => TypeShape::Str,
                    // `new(T)` is Go's `new`: it returns `*T` — a POINTER to
                    // zeroed, mutable storage — never a primitive copy. Typing
                    // it Primitive made it sendable-by-type (class 1), so
                    // `p := new(S); task w(p)` shared a `*S` across tasks with no
                    // diagnostic and raced (O4 class table: `*T` is a named
                    // mutable GC value, not sendable; O4.9: unresolved/unknown
                    // shapes fail closed). Type it as a pointer so it is
                    // classified as shared mutable state and rejected at every
                    // check-site. The pointee is inferred when the type argument
                    // resolves; otherwise Unknown — still a non-sendable pointer.
                    "new" => {
                        let pointee = match args
                            .first()
                            .map(|a| crate::model::unwrap_parens(&a.value))
                        {
                            // `new(pool_stats)` / `new((pool_stats))` / `new(alias)` — the
                            // argument is a type name, not a value; resolve structs, type
                            // aliases, primitives, and foreign types via `named_shape`.
                            Some(Expr::Ident { name, .. }) => self.named_shape(name),
                            Some(Expr::Field { base, name, .. }) => {
                                if let Expr::Ident { name: pkg, .. } =
                                    crate::model::unwrap_parens(base)
                                {
                                    self.named_shape(&format!("{}.{}", pkg, name))
                                } else {
                                    TypeShape::Unknown
                                }
                            }
                            Some(Expr::Index { base, .. }) => {
                                if let Expr::Ident { name, .. } = crate::model::unwrap_parens(base)
                                {
                                    self.named_shape(name)
                                } else {
                                    TypeShape::Unknown
                                }
                            }
                            Some(other) => self.shape_of_expr(st, other),
                            None => TypeShape::Unknown,
                        };
                        TypeShape::Pointer(Box::new(pointee))
                    }
                    _ => {
                        let head = Self::base_named_type_str(name);
                        self.fns
                            .get(name.as_str())
                            .or_else(|| self.fns.get(head))
                            .and_then(|f| f.results.first())
                            .map(|r| self.shape_of_type(&r.ty))
                            .unwrap_or(TypeShape::Unknown)
                    }
                }
            }
            Expr::Field { base, name, .. } => {
                // package call (`serde_json.parse`) vs method call (`doc.array`)
                if let Expr::Ident { name: b, .. } = &**base {
                    if self.lookup_binding(st, b).is_none() && self.known_package(b) {
                        return foreign::package_result(b, name);
                    }
                }
                let head = Self::base_named_type_str(name);
                if let Expr::Ident { name: root, .. } = crate::model::unwrap_parens(base) {
                    if self.lookup_binding(st, root).is_none() {
                        let full = format!("{}.{}", root, head);
                        if let Some(f) = self.fns.get(full.as_str()) {
                            return f
                                .results
                                .first()
                                .map(|r| self.shape_of_type(&r.ty))
                                .unwrap_or(TypeShape::Unknown);
                        }
                    }
                }
                // Issue #411/H2: user-defined methods (`c.keys()`).
                let recv = self.shape_of_expr(st, base);
                if name == "pop" && args.is_empty() {
                    if let TypeShape::Slice(elem) = &recv {
                        return (**elem).clone();
                    }
                }
                if let Some(recv_base) = self
                    .ty_of_expr(st, base)
                    .map(|t| self.resolve_type_alias(&t))
                    .and_then(|t| Self::base_named_type(&t))
                    .or_else(|| match &recv {
                        TypeShape::Struct(Some(s)) => {
                            Some(Self::base_named_type_str(s).to_string())
                        }
                        TypeShape::Pointer(inner) => match &**inner {
                            TypeShape::Struct(Some(s)) => {
                                Some(Self::base_named_type_str(s).to_string())
                            }
                            _ => None,
                        },
                        _ => None,
                    })
                {
                    if let Some(f) = self.method_decls.get(&(recv_base, head.to_string())) {
                        if let Some(r) = f.results.first() {
                            return self.shape_of_type(&r.ty);
                        }
                    }
                }
                let recv = self.shape_of_expr(st, base);
                if let Some(t) = receiver_type_name(&recv) {
                    return foreign::method_result(&t, name);
                }
                let head = Self::base_named_type_str(name);
                let recv_base = self
                    .ty_of_expr(st, base)
                    .map(|t| self.resolve_type_alias(&t))
                    .and_then(|t| Self::base_named_type(&t))
                    .or_else(|| match &recv {
                        TypeShape::Struct(Some(s)) => {
                            Some(Self::base_named_type_str(s).to_string())
                        }
                        TypeShape::Pointer(inner) => match &**inner {
                            TypeShape::Struct(Some(s)) => {
                                Some(Self::base_named_type_str(s).to_string())
                            }
                            _ => None,
                        },
                        _ => None,
                    });
                recv_base
                    .and_then(|rb| self.method_decls.get(&(rb, head.to_string())))
                    .and_then(|f| f.results.first())
                    .map(|r| self.shape_of_type(&r.ty))
                    .unwrap_or(TypeShape::Unknown)
            }
            _ => TypeShape::Unknown,
        }
    }

    fn make_shape(&self, args: &[CallArg]) -> TypeShape {
        let Some(first) = args.first() else {
            return TypeShape::Unknown;
        };
        match crate::model::unwrap_parens(&first.value) {
            // The parser renders a type in expression position as a literal
            // node carrying the TypeExpr in `.ty` (`make(chan bool, n)` →
            // StructLit{ty: Chan}); other spellings degrade to Unknown.
            Expr::StructLit { ty: Some(t), .. } => self.shape_of_type(t),
            // `make([]T, n)` — keep the ELEMENT shape (not Unknown): the
            // element type decides whether a later `copy(dst, src)` shares
            // references (#178). A primitive element (`[]int`) shares
            // nothing; a reference element (`[]map[...]`) does.
            Expr::SliceLit { elem: Some(t), .. } if first.value.as_array_lit().is_none() => {
                TypeShape::Slice(Box::new(self.shape_of_type(t)))
            }
            // `make(map[K]V{})` — shape key AND value: an erasure to
            // Map(Unknown, Unknown) let containers holding closures (or
            // handles) slip past the closure-container move guard (#354,
            // regression-tested in const_write.rs).
            Expr::MapLit { key, value, .. } => TypeShape::Map(
                Box::new(self.shape_of_type(key)),
                Box::new(self.shape_of_type(value)),
            ),
            Expr::Ident { name, .. } => self.named_shape(name),
            _ => TypeShape::Unknown,
        }
    }

    pub(crate) fn known_package(&self, pkg: &str) -> bool {
        self.packages.contains(pkg) || foreign::BUILTIN_PACKAGES.contains(&pkg)
    }

    // -----------------------------------------------------------------------
    // Loop variables
    // -----------------------------------------------------------------------

    /// Shapes of the variables bound by `for v[, i] in <expr>`.
    pub(crate) fn iterate_var_shapes(
        &self,
        st: &FnState,
        expr: &Expr,
        n_vars: usize,
    ) -> Vec<TypeShape> {
        let s = self.shape_of_expr(st, expr);
        let one = |v: TypeShape| {
            if n_vars >= 2 {
                vec![TypeShape::Primitive, v]
            } else {
                vec![v]
            }
        };
        match &s {
            // iterating a view yields views (element loans, corpus 07/11/12)
            TypeShape::BorrowHandle(_) => vec![s.clone()],
            TypeShape::Slice(e) | TypeShape::Array(e) | TypeShape::Chan(e) => one((**e).clone()),
            TypeShape::Map(k, v) => {
                if n_vars >= 2 {
                    vec![(**k).clone(), (**v).clone()]
                } else {
                    vec![(**v).clone()]
                }
            }
            TypeShape::Struct(Some(n)) if n.starts_with("set[") => {
                let elem_str = n
                    .strip_prefix("set[")
                    .and_then(|s| s.strip_suffix(']'))
                    .unwrap_or("any");
                vec![self.named_shape(elem_str)]
            }
            _ => vec![TypeShape::Unknown; n_vars.max(1)],
        }
    }

    // -----------------------------------------------------------------------
    // Source types for diagnostics
    // -----------------------------------------------------------------------

    pub(crate) fn base_named_type_str(name: &str) -> &str {
        name.split('[').next().unwrap_or(name)
    }

    pub(crate) fn base_named_type(ty: &TypeExpr) -> Option<String> {
        match ty {
            TypeExpr::Named { name, .. } => Some(Self::base_named_type_str(name).to_string()),
            TypeExpr::Pointer { to, .. } => Self::base_named_type(to),
            _ => None,
        }
    }

    pub(crate) fn resolve_type_alias(&self, ty: &TypeExpr) -> TypeExpr {
        let mut cur = ty.clone();
        for _ in 0..8 {
            if let TypeExpr::Named { name, .. } = &cur {
                let base = Self::base_named_type_str(name);
                if let Some(target) = self.type_aliases.get(base) {
                    cur = target.clone();
                    continue;
                }
            }
            break;
        }
        cur
    }

    /// The as-written type of an initializer, when directly available
    /// (literals, aliases, local-fn results). `None` = use the shape render.
    pub(crate) fn ty_of_expr(&self, st: &FnState, expr: &Expr) -> Option<TypeExpr> {
        match expr {
            Expr::Ident { name, span } => {
                if let Some(b) = self.lookup_binding(st, name) {
                    return b.ty.clone();
                }
                if name == "none" {
                    return Some(TypeExpr::Option {
                        elem: Box::new(TypeExpr::Named {
                            name: "any".to_string(),
                            span: *span,
                        }),
                        span: *span,
                    });
                }
                if let Some((ename, _)) = self
                    .enums
                    .iter()
                    .find(|(_, vs)| vs.iter().any(|v| v == name))
                {
                    return Some(TypeExpr::Named {
                        name: ename.clone(),
                        span: *span,
                    });
                }
                let head = Self::base_named_type_str(name);
                if let Some(decl) = self.fns.get(name.as_str()).or_else(|| self.fns.get(head)) {
                    return Some(TypeExpr::Fn {
                        params: decl.params.clone(),
                        results: decl.results.iter().map(|r| r.ty.clone()).collect(),
                        span: *span,
                    });
                }
                None
            }
            Expr::Field { base, name, span } => {
                if let Expr::Ident { name: root, .. } = crate::model::unwrap_parens(base) {
                    if self.lookup_binding(st, root).is_none() {
                        if let Some(vs) = self.enums.get(root.as_str()) {
                            if vs.iter().any(|v| v == name) {
                                return Some(TypeExpr::Named {
                                    name: root.clone(),
                                    span: *span,
                                });
                            }
                        }
                    }
                }
                let base_ty = self.ty_of_expr(st, base)?;
                let resolved = self.resolve_type_alias(&base_ty);
                if name == "pop" {
                    if let TypeExpr::Slice { elem, .. } = &resolved {
                        return Some(TypeExpr::Fn {
                            params: Vec::new(),
                            results: vec![(**elem).clone()],
                            span: *span,
                        });
                    }
                }
                let sname = Self::base_named_type(&resolved)?;
                self.structs
                    .get(&sname)
                    .and_then(|fs| fs.iter().find(|(n, _)| n == name))
                    .map(|(_, t)| t.clone())
            }
            Expr::Index { base, .. } => {
                let base_ty = self.ty_of_expr(st, base)?;
                match self.resolve_type_alias(&base_ty) {
                    TypeExpr::Slice { elem, .. } | TypeExpr::Array { elem, .. } => Some(*elem),
                    TypeExpr::Map { value, .. } => Some(*value),
                    _ => None,
                }
            }
            Expr::StructLit { ty, .. } => ty.clone(),
            Expr::MapLit {
                key, value, span, ..
            } => Some(TypeExpr::Map {
                key: key.clone(),
                value: value.clone(),
                span: *span,
            }),
            Expr::SliceLit {
                elem: Some(e),
                items,
                span,
            } => {
                if let Some((len_opt, inner_elem, arr_ty)) = expr.as_array_lit() {
                    if len_opt.is_some() {
                        return Some(arr_ty.clone());
                    }
                    return Some(TypeExpr::Array {
                        len: Some(Box::new(Expr::Lit {
                            kind: hy_syntax::LitKind::Int(items.len().to_string()),
                            span: *span,
                        })),
                        elem: Box::new(inner_elem.clone()),
                        span: *span,
                    });
                }
                Some(TypeExpr::Slice {
                    elem: e.clone(),
                    span: *span,
                })
            }
            Expr::FnLit {
                params,
                results,
                span,
                ..
            } => Some(TypeExpr::Fn {
                params: params.clone(),
                results: results.iter().map(|r| r.ty.clone()).collect(),
                span: *span,
            }),
            Expr::Move { operand, .. }
            | Expr::Try { operand, .. }
            | Expr::Paren { inner: operand, .. } => self.ty_of_expr(st, operand),
            Expr::Call { callee, args, span } => match crate::model::unwrap_parens(callee) {
                Expr::FnLit { results, .. } => results.first().map(|r| r.ty.clone()),
                Expr::Ident { name, .. } => {
                    if let Some(b) = self.lookup_binding(st, name) {
                        if let Some(ty) = &b.ty {
                            if let TypeExpr::Fn { results, .. } = self.resolve_type_alias(ty) {
                                return results.first().cloned();
                            }
                        }
                        return None;
                    }
                    if name == "some" {
                        let elem = args
                            .first()
                            .and_then(|a| self.ty_of_expr(st, &a.value))
                            .unwrap_or_else(|| TypeExpr::Named {
                                name: "any".to_string(),
                                span: *span,
                            });
                        return Some(TypeExpr::Option {
                            elem: Box::new(elem),
                            span: *span,
                        });
                    }
                    if name == "make" {
                        return args.first().and_then(|a| self.ty_of_expr(st, &a.value));
                    }
                    if args.len() == 1 {
                        let is_type_name = crate::const_overflow::fixed_width_int_range(name).is_some()
                            || crate::const_init::is_conversion_type_name(name)
                            || self.type_aliases.contains_key(name);
                        if is_type_name && !self.fns.contains_key(name.as_str()) {
                            return Some(TypeExpr::Named {
                                name: name.clone(),
                                span: *span,
                            });
                        }
                    }
                    let head = Self::base_named_type_str(name);
                    self.fns
                        .get(name.as_str())
                        .or_else(|| self.fns.get(head))
                        .and_then(|f| f.results.first())
                        .map(|r| r.ty.clone())
                }
                Expr::Field { base, name, .. } => {
                    let head = Self::base_named_type_str(name);
                    if let Expr::Ident { name: root, .. } = crate::model::unwrap_parens(base) {
                        if self.lookup_binding(st, root).is_none() {
                            let full = format!("{}.{}", root, head);
                            if let Some(f) = self.fns.get(full.as_str()) {
                                return f.results.first().map(|r| r.ty.clone());
                            }
                        }
                    }
                    let recv_base = self
                        .ty_of_expr(st, base)
                        .map(|t| self.resolve_type_alias(&t))
                        .and_then(|t| Self::base_named_type(&t))
                        .or_else(|| match self.shape_of_expr(st, base) {
                            TypeShape::Struct(Some(s)) => {
                                Some(Self::base_named_type_str(&s).to_string())
                            }
                            TypeShape::Pointer(inner) => match *inner {
                                TypeShape::Struct(Some(s)) => {
                                    Some(Self::base_named_type_str(&s).to_string())
                                }
                                _ => None,
                            },
                            _ => None,
                        });
                    if let Some(rb) = recv_base {
                        if let Some(ret) = self
                            .method_decls
                            .get(&(rb, head.to_string()))
                            .and_then(|f| f.results.first())
                            .map(|r| r.ty.clone())
                        {
                            return Some(ret);
                        }
                    }
                    let callee_ty = self.ty_of_expr(st, callee)?;
                    if let TypeExpr::Fn { results, .. } = self.resolve_type_alias(&callee_ty) {
                        results.first().cloned()
                    } else {
                        None
                    }
                }
                other => {
                    let callee_ty = self.ty_of_expr(st, other)?;
                    if let TypeExpr::Fn { results, .. } = self.resolve_type_alias(&callee_ty) {
                        results.first().cloned()
                    } else {
                        None
                    }
                }
            },
            Expr::Binary { op, lhs, rhs, .. } => {
                if matches!(
                    op,
                    hy_syntax::BinOp::Add
                        | hy_syntax::BinOp::Sub
                        | hy_syntax::BinOp::Mul
                        | hy_syntax::BinOp::Div
                        | hy_syntax::BinOp::Mod
                        | hy_syntax::BinOp::BitAnd
                        | hy_syntax::BinOp::BitOr
                        | hy_syntax::BinOp::BitXor
                ) {
                    self.ty_of_expr(st, lhs).or_else(|| self.ty_of_expr(st, rhs))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// N3 (`HYB-E4002`): returns `Some(reason)` when `ty` has no zero value
    /// (an `enum` whose first variant carries a payload, or a `struct`/`[N]T`
    /// containing such a field/element transitively).
    pub(crate) fn no_zero_value_reason(&self, ty: &TypeExpr, depth: usize) -> Option<String> {
        if depth > 16 {
            return None;
        }
        let resolved = self.resolve_type_alias(ty);
        match &resolved {
            TypeExpr::Named { name, .. } => {
                let base = Self::base_named_type_str(name);
                if self.enum_has_zero_value.get(base) == Some(&false) {
                    return Some(format!(
                        "enum `{}` has no zero value because its first variant carries a payload (N3)",
                        base
                    ));
                }
                if let Some(fields) = self.structs.get(base) {
                    for (fname, fty) in fields {
                        if let Some(inner) = self.no_zero_value_reason(fty, depth + 1) {
                            return Some(format!(
                                "struct `{}` has no zero value because field `{}` ({})",
                                base, fname, inner
                            ));
                        }
                    }
                }
                None
            }
            TypeExpr::Array { elem, .. } => self.no_zero_value_reason(elem, depth + 1),
            _ => None,
        }
    }

    /// N3 (`HYB-E4002`): check a `var x T` declaration with no initializer.
    pub(crate) fn check_zero_value_local(&mut self, l: &hy_syntax::LocalDecl, ty: &TypeExpr) {
        if let Some(reason) = self.no_zero_value_reason(ty, 0) {
            let names = l.names.join(", ");
            let msg = format!(
                "`{}` has no zero value — initialize `{}` explicitly (HYB-E4002)",
                ty.render(),
                names
            );
            self.diags.push(
                hy_diag::Diagnostic::error("HYB-E4002", self.path.clone(), l.span, msg)
                    .with_notes(vec![
                        format!("why: {}", reason),
                        format!(
                            "fix: initialize `{}` explicitly with `:=`, or reorder the enum variants so the first variant is payload-free",
                            names
                        ),
                    ]),
            );
        }
    }

    /// T13/T14/T15 (`HYB-E5001`): check `match` exhaustiveness and `_` arm placement.
    pub(crate) fn check_match(
        &mut self,
        st: &FnState,
        subject: &Expr,
        arms: &[hy_syntax::MatchArm],
        span: hy_diag::Span,
    ) {
        use hy_diag::Diagnostic;
        use hy_syntax::MatchPattern;

        if arms.is_empty() {
            self.diags.push(
                Diagnostic::error(
                    "HYB-E5001",
                    self.path.clone(),
                    span,
                    "non-exhaustive match: `match` has no arms (HYB-E5001)",
                )
                .with_notes(vec![
                    "why: every `match` on `option[T]` or an `enum` must cover all variants (T13/T14)".to_string(),
                ]),
            );
            return;
        }

        // T15 (R4): `_` is legal in `match` ONLY as the last arm, and requires
        // at least one specific variant arm before it.
        for (i, arm) in arms.iter().enumerate() {
            if matches!(arm.pattern, MatchPattern::Wildcard) {
                if i + 1 < arms.len() {
                    self.diags.push(
                        Diagnostic::error(
                            "HYB-E5001",
                            self.path.clone(),
                            span,
                            "`_` must be the last match arm — arms after `_` are unreachable (HYB-E5001)",
                        )
                        .with_notes(vec![
                            "why: `_` matches every remaining variant; placing it before specific arms leaves the following arms dead (T15)".to_string(),
                            "fix: move the `_` arm to the end of the `match`, or replace `_` with the specific variant names".to_string(),
                        ]),
                    );
                    return;
                }
                if i == 0 {
                    self.diags.push(
                        Diagnostic::error(
                            "HYB-E5001",
                            self.path.clone(),
                            span,
                            "`_` cannot be the first or only match arm — match at least one variant before `_` (HYB-E5001)",
                        )
                        .with_notes(vec![
                            "why: `_` is a catch-all for remaining variants at the end of a `match` (T15)".to_string(),
                            "fix: match at least one variant explicitly before the trailing `_` arm".to_string(),
                        ]),
                    );
                    return;
                }
            }
        }

        enum MatchTarget {
            Option,
            Enum(String, Vec<String>),
            Unknown,
        }

        let mut target = MatchTarget::Unknown;
        if let Some(ty) = self.ty_of_expr(st, subject) {
            let resolved = self.resolve_type_alias(&ty);
            match &resolved {
                TypeExpr::Option { .. } => target = MatchTarget::Option,
                TypeExpr::Named { name, .. } => {
                    let base = Self::base_named_type_str(name);
                    if let Some(vs) = self.enums.get(base) {
                        target = MatchTarget::Enum(base.to_string(), vs.clone());
                    }
                }
                _ => {}
            }
        }
        if matches!(target, MatchTarget::Unknown) {
            for arm in arms {
                if let MatchPattern::Path { segments, .. } = &arm.pattern {
                    if segments.as_slice() == ["some"] || segments.as_slice() == ["none"] {
                        target = MatchTarget::Option;
                        break;
                    } else if segments.len() == 2 {
                        if let Some(vs) = self.enums.get(segments[0].as_str()) {
                            target = MatchTarget::Enum(segments[0].clone(), vs.clone());
                            break;
                        }
                    } else if segments.len() == 1 {
                        if let Some((ename, vs)) = self
                            .enums
                            .iter()
                            .find(|(_, vs)| vs.iter().any(|v| v == &segments[0]))
                        {
                            target = MatchTarget::Enum(ename.clone(), vs.clone());
                            break;
                        }
                    }
                }
            }
        }

        match target {
            MatchTarget::Option => {
                let mut has_some = false;
                let mut has_none = false;
                for arm in arms {
                    if arm.guard.is_some() {
                        continue;
                    }
                    match &arm.pattern {
                        MatchPattern::Path { segments, .. } if segments.as_slice() == ["some"] => {
                            has_some = true;
                        }
                        MatchPattern::Path { segments, .. } if segments.as_slice() == ["none"] => {
                            has_none = true;
                        }
                        MatchPattern::Wildcard if has_some => {
                            has_none = true;
                        }
                        _ => {}
                    }
                }
                let mut missing: Vec<&str> = Vec::new();
                if !has_some {
                    missing.push("`some(v)`");
                }
                if !has_none {
                    missing.push("`none`");
                }
                if !missing.is_empty() {
                    let arm_word = if missing.len() == 1 { "arm" } else { "arms" };
                    let msg = format!(
                        "non-exhaustive match on `option`: missing {} {} (HYB-E5001)",
                        missing.join(", "),
                        arm_word
                    );
                    self.diags.push(
                        Diagnostic::error("HYB-E5001", self.path.clone(), span, msg).with_notes(
                            vec![
                                "why: every `option[T]` is either `some(v)` or `none`; all variants must be handled explicitly (T13/T14)".to_string(),
                                format!(
                                    "fix: add the missing {} ({}), or use a trailing `_:` arm after `some(v):`",
                                    arm_word,
                                    missing.join(", ")
                                ),
                            ],
                        ),
                    );
                }
            }
            MatchTarget::Enum(enum_name, variants) => {
                let has_trailing_wildcard = arms.last().is_some_and(|a| {
                    matches!(a.pattern, MatchPattern::Wildcard) && a.guard.is_none()
                });
                if !has_trailing_wildcard {
                    let mut covered = std::collections::HashSet::new();
                    for arm in arms {
                        if arm.guard.is_some() {
                            continue;
                        }
                        if let MatchPattern::Path { segments, .. } = &arm.pattern {
                            if segments.len() == 2 && segments[0] == enum_name {
                                covered.insert(segments[1].as_str());
                            } else if segments.len() == 1 {
                                covered.insert(segments[0].as_str());
                            }
                        }
                    }
                    let missing: Vec<String> = variants
                        .iter()
                        .filter(|v| !covered.contains(v.as_str()))
                        .map(|v| format!("`{v}`"))
                        .collect();
                    if !missing.is_empty() {
                        let arm_word = if missing.len() == 1 { "arm" } else { "arms" };
                        let missing_str = missing.join(", ");
                        let msg = format!(
                            "non-exhaustive match on `{}`: missing {} for {} (HYB-E5001)",
                            enum_name, arm_word, missing_str
                        );
                        self.diags.push(
                            Diagnostic::error("HYB-E5001", self.path.clone(), span, msg)
                                .with_notes(vec![
                                    format!(
                                        "why: every variant of enum `{}` ({}) must be covered by an arm or a final `_` wildcard (T13/T14)",
                                        enum_name,
                                        variants.join(", ")
                                    ),
                                    format!(
                                        "fix: add match {} for {}, or add a trailing `_:` arm",
                                        arm_word, missing_str
                                    ),
                                ]),
                        );
                    }
                }
            }
            MatchTarget::Unknown => {
                let has_trailing_wildcard = arms.last().is_some_and(|a| {
                    matches!(a.pattern, MatchPattern::Wildcard) && a.guard.is_none()
                });
                if !has_trailing_wildcard {
                    self.diags.push(
                        Diagnostic::error(
                            "HYB-E5001",
                            self.path.clone(),
                            span,
                            "non-exhaustive match: missing trailing `_` arm for non-enumerated subject (HYB-E5001)",
                        )
                        .with_notes(vec![
                            "why: `match` requires exhaustive variant coverage or a final `_` arm (T13/T14/T15)".to_string(),
                            "fix: add a trailing `_:` arm, or match on an `option[T]` or declared `enum` covering all variants".to_string(),
                        ]),
                    );
                }
            }
        }
    }

    pub(crate) fn call_result_tys(&self, st: &FnState, callee: &Expr) -> Option<Vec<TypeExpr>> {
        match crate::model::unwrap_parens(callee) {
            Expr::FnLit { results, .. } => Some(results.iter().map(|r| r.ty.clone()).collect()),
            Expr::Ident { name, .. } => {
                if let Some(b) = self.lookup_binding(st, name) {
                    if let Some(ty) = &b.ty {
                        if let TypeExpr::Fn { results, .. } = self.resolve_type_alias(ty) {
                            return Some(results);
                        }
                    }
                    return None;
                }
                let head = Self::base_named_type_str(name);
                self.fns
                    .get(name.as_str())
                    .or_else(|| self.fns.get(head))
                    .map(|f| f.results.iter().map(|r| r.ty.clone()).collect())
            }
            Expr::Field { base, name, .. } => {
                let head = Self::base_named_type_str(name);
                if let Expr::Ident { name: root, .. } = crate::model::unwrap_parens(base) {
                    if self.lookup_binding(st, root).is_none() {
                        let full = format!("{}.{}", root, head);
                        if let Some(f) = self.fns.get(full.as_str()) {
                            return Some(f.results.iter().map(|r| r.ty.clone()).collect());
                        }
                    }
                }
                let recv = self.shape_of_expr(st, base);
                let recv_base = self
                    .ty_of_expr(st, base)
                    .map(|t| self.resolve_type_alias(&t))
                    .and_then(|t| Self::base_named_type(&t))
                    .or_else(|| match &recv {
                        TypeShape::Struct(Some(s)) => {
                            Some(Self::base_named_type_str(s).to_string())
                        }
                        TypeShape::Pointer(inner) => match &**inner {
                            TypeShape::Struct(Some(s)) => {
                                Some(Self::base_named_type_str(s).to_string())
                            }
                            _ => None,
                        },
                        _ => None,
                    })?;
                self.method_decls
                    .get(&(recv_base, head.to_string()))
                    .map(|f| f.results.iter().map(|r| r.ty.clone()).collect())
            }
            _ => None,
        }
    }

    /// Shapes for a multi-name define/assign from one value (`v, ok := m[k]`,
    /// `v, ok := <-ch`, `a, b := f()`, `a, b = f()?`).
    pub(crate) fn multi_value_shapes(
        &self,
        st: &FnState,
        value: &Expr,
        n_names: usize,
    ) -> Vec<TypeShape> {
        let v = crate::model::unwrap_parens(value);
        if n_names == 2 {
            if let Expr::Index { base, .. } = v {
                if let TypeShape::Map(_, val) = self.shape_of_expr(st, base) {
                    return vec![*val, TypeShape::Primitive]; // v, ok := m[k]
                }
            }
            // `v, ok := <-ch` — the value is the channel's element; `ok` is
            // the receive-ok flag (a bool)
            if let Expr::Receive { chan, .. } = v {
                let elem = match self.shape_of_expr(st, chan) {
                    TypeShape::Chan(e) => *e,
                    _ => TypeShape::Unknown,
                };
                return vec![elem, TypeShape::Primitive];
            }
        }
        let (call_expr, is_try) = match v {
            Expr::Try { operand, .. } => (crate::model::unwrap_parens(operand), true),
            other => (other, false),
        };
        if let Expr::Call { callee, .. } = call_expr {
            if let Some(results) = self.call_result_tys(st, callee) {
                let avail = if is_try {
                    results.len().saturating_sub(1)
                } else {
                    results.len()
                };
                if avail == n_names {
                    return results
                        .iter()
                        .take(n_names)
                        .map(|ty| self.shape_of_type(ty))
                        .collect();
                }
            }
        }
        vec![TypeShape::Unknown; n_names]
    }

    /// As-written source types for a multi-name define/assign from one value
    /// (`a, b := f()`, `a, b = f()?`, `v, ok := m[k]`), when directly available.
    pub(crate) fn multi_value_tys(
        &self,
        st: &FnState,
        value: &Expr,
        n_names: usize,
    ) -> Vec<Option<TypeExpr>> {
        let v = crate::model::unwrap_parens(value);
        if n_names == 2 {
            if let Expr::Index { base, .. } = v {
                if let Some(TypeExpr::Map { value: val_ty, .. }) = self.ty_of_expr(st, base) {
                    return vec![Some(*val_ty), None];
                }
            }
        }
        let (call_expr, is_try) = match v {
            Expr::Try { operand, .. } => (crate::model::unwrap_parens(operand), true),
            other => (other, false),
        };
        if let Expr::Call { callee, .. } = call_expr {
            if let Some(results) = self.call_result_tys(st, callee) {
                let avail = if is_try {
                    results.len().saturating_sub(1)
                } else {
                    results.len()
                };
                if avail == n_names {
                    return results
                        .into_iter()
                        .take(n_names)
                        .map(Some)
                        .collect();
                }
            }
        }
        vec![None; n_names]
    }
}

/// The boundary type name that keys the method-mode table for a receiver
/// shape ("serde_json.value", "atomic", ...).
pub(crate) fn receiver_type_name(shape: &TypeShape) -> Option<String> {
    match shape {
        TypeShape::OwnHandle(t) | TypeShape::BorrowHandle(t) | TypeShape::ForeignOpaque(t) => {
            Some(t.clone())
        }
        TypeShape::Atomic => Some("atomic.int".to_string()),
        TypeShape::Pointer(inner) => receiver_type_name(inner),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::{check_file, Verdict};

    fn check(src: &str) -> crate::CheckResult {
        let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
        check_file(&ast)
    }

    #[test]
    fn pointer_receiver_alias_is_frozen_on_move() {
        let r = check(
            "import \"fmt\"\n\
             struct stats { count int }\n\
             fn worker(s *stats) { s.count = s.count + 1 }\n\
             fn (s *stats) run() {\n\
               alias := s\n\
               task worker(move s)\n\
               alias.count = 42\n\
               fmt.println(alias.count)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        // The non-owned-move guard (E3004) fires at the move site first: the
        // receiver aliases caller-owned storage, so `move s` is invalid
        // outright. The alias freeze (E3003) is the diagnosis for the
        // owned-binding variant of this shape.
        assert_eq!(r.diagnostics[0].code, "HYB-E3004");
    }

    #[test]
    fn pointer_receiver_to_empty_struct_is_still_a_pointer_at_spawn() {
        // Even when the pointee is a 0-field struct (which `named_shape` treats
        // as Primitive by value), a pointer receiver `s *marker` has shape
        // `Pointer(...)` and must be rejected at spawn without `move`.
        let r = check(
            "struct marker {}\n\
             fn worker(m *marker) {}\n\
             fn (m *marker) run() {\n\
               task worker(m)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert_eq!(r.diagnostics[0].code, "HYB-E3001");
        assert!(r.diagnostics[0].message.contains("*marker"));
    }

    #[test]
    fn method_with_same_name_does_not_overwrite_toplevel_fn_in_table() {
        let r = check(
            "struct dummy { x int }\n\
             fn make_data() map[string]int {\n\
               return map[string]int{\"init\": 0}\n\
             }\n\
             fn (d dummy) make_data() int {\n\
               return d.x\n\
             }\n\
             fn worker(m map[string]int) { m[\"k\"] = 1 }\n\
             fn main() {\n\
               m := make_data()\n\
               task worker(m)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert_eq!(r.diagnostics[0].code, "HYB-E3001");
        assert!(r.diagnostics[0].message.contains("map[string]int"));
    }

    #[test]
    fn local_closure_shadowing_toplevel_fn_is_not_resolved_from_fn_table() {
        let r = check(
            "fn get_data() int { return 0 }\n\
             fn worker(m map[string]int) { m[\"k\"] = 1 }\n\
             fn main() {\n\
               get_data := fn() map[string]int {\n\
                 return map[string]int{\"init\": 0}\n\
               }\n\
               m := get_data()\n\
               task worker(m)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert_eq!(r.diagnostics[0].code, "HYB-E3001");
        assert!(r.diagnostics[0].message.contains("map[string]int"));
    }

    #[test]
    fn pointer_receiver_move_rejects_as_non_owned_even_without_post_use() {
        // Updated to the non-owned-move doctrine (pr-61): a receiver aliases
        // caller-owned storage, so `move s` is rejected outright — no
        // post-move use required. The diagnostic still names the pointee
        // `*stats`, pinning the pointer-receiver shape fix this PR added.
        let r = check(
            "struct stats { count int }\n\
             fn worker(s *stats) { s.count = s.count + 1 }\n\
             fn (s *stats) run() {\n\
               task worker(move s)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert_eq!(r.diagnostics[0].code, "HYB-E3004");
        assert!(r.diagnostics[0].message.contains("*stats"));
    }

    #[test]
    fn match_option_missing_none_emits_e5001() {
        // R1 (#143): option[T] match with `some(v)` only must reject with HYB-E5001 naming `none`.
        let r = check(
            "fn main() {\n\
               var x option[int] = some(1)\n\
               match x {\n\
                 some(v): fmt.println(v)\n\
               }\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert_eq!(r.diagnostics[0].code, "HYB-E5001");
        assert!(
            r.diagnostics[0].message.contains("none"),
            "expected missing `none` in message: {}",
            r.diagnostics[0].message
        );
    }

    #[test]
    fn match_option_missing_some_emits_e5001() {
        // R2 (#143): option[T] match with `none` only must reject with HYB-E5001 naming `some`.
        let r = check(
            "fn main() {\n\
               var x option[int] = some(1)\n\
               match x {\n\
                 none: fmt.println(\"none\")\n\
               }\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert_eq!(r.diagnostics[0].code, "HYB-E5001");
        assert!(
            r.diagnostics[0].message.contains("some"),
            "expected missing `some` in message: {}",
            r.diagnostics[0].message
        );
    }

    #[test]
    fn match_enum_missing_variant_emits_e5001() {
        // R3 (#143): declared enum missing a variant without `_` must reject with HYB-E5001 naming `blue`.
        let r = check(
            "enum color {\n\
               red\n\
               green\n\
               blue\n\
             }\n\
             fn main() {\n\
               c := color.red\n\
               match c {\n\
                 color.red:   fmt.println(\"r\")\n\
                 color.green: fmt.println(\"g\")\n\
               }\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert_eq!(r.diagnostics[0].code, "HYB-E5001");
        assert!(
            r.diagnostics[0].message.contains("blue"),
            "expected missing `blue` in message: {}",
            r.diagnostics[0].message
        );
    }

    #[test]
    fn match_wildcard_not_last_emits_e5001() {
        // R4 (#143): `_` before the final arm must reject with HYB-E5001.
        let r = check(
            "fn main() {\n\
               var x option[int] = some(1)\n\
               match x {\n\
                 _:       fmt.println(\"any\")\n\
                 some(v): fmt.println(v)\n\
               }\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert_eq!(r.diagnostics[0].code, "HYB-E5001");
        assert!(
            r.diagnostics[0]
                .message
                .contains("`_` must be the last match arm"),
            "unexpected message: {}",
            r.diagnostics[0].message
        );
    }

    #[test]
    fn match_exhaustive_option_and_enum_accept() {
        let r = check(
            "enum color { red green blue }\n\
             fn main() {\n\
               var x option[int] = some(1)\n\
               match x {\n\
                 some(v): fmt.println(v)\n\
                 none:    fmt.println(\"none\")\n\
               }\n\
               match x {\n\
                 some(v): fmt.println(v)\n\
                 _:       fmt.println(\"none\")\n\
               }\n\
               c := color.red\n\
               match c {\n\
                 color.red:   fmt.println(\"r\")\n\
                 color.green: fmt.println(\"g\")\n\
                 color.blue:  fmt.println(\"b\")\n\
               }\n\
               match c {\n\
                 color.red: fmt.println(\"r\")\n\
                 _:         fmt.println(\"other\")\n\
               }\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    }

    #[test]
    fn match_inside_closure_checks_exhaustiveness() {
        let r = check(
            "fn main() {\n\
               f := fn() {\n\
                 x := some(1)\n\
                 match x {\n\
                   some(v): fmt.println(v)\n\
                 }\n\
               }\n\
               f()\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert_eq!(r.diagnostics[0].code, "HYB-E5001");
    }
    #[test]
    fn make_map_preserves_value_shape_across_index_hop() {
        let r = check(
            "fn worker(m map[string]int, done chan<- bool) { m[\"w\"] = 1\n done <- true }\n\
             fn main() {\n\
               done := make(chan bool, 1)\n\
               mm := make(map[string]map[string]int{})\n\
               mm[\"a\"] = map[string]int{\"k\": 1}\n\
               x := mm[\"a\"]\n\
               var m2 map[string]int = x\n\
               task worker(move m2, done)\n\
               x[\"k\"] = 2\n\
               <-done\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert_eq!(r.diagnostics[0].code, "HYB-E3003");
    }

    #[test]
    fn make_map_preserves_non_owned_origin_across_index_store_and_hop() {
        let r = check(
            "fn worker(m map[string]int, done chan<- bool) { m[\"w\"] = 1\n done <- true }\n\
             fn spawn_it(p map[string]int, done chan<- bool) {\n\
               mm := make(map[string]map[string]int{})\n\
               mm[\"a\"] = p\n\
               x := mm[\"a\"]\n\
               var m2 map[string]int = x\n\
               task worker(move m2, done)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert_eq!(r.diagnostics[0].code, "HYB-E3004");
    }

    #[test]
    fn scope_pop_propagates_move_across_restored_may_share_peers() {
        let r = check(
            "struct holder { m map[string]int }\n\
             fn worker(m map[string]int, done chan<- bool) { m[\"w\"] = 1\n done <- true }\n\
             fn main() {\n\
               done := make(chan bool, 1)\n\
               m := map[string]int{\"k\": 0}\n\
               h := holder{m: m}\n\
               if true {\n\
                 h := 1\n\
                 fmt.println(h)\n\
                 task worker(move m, done)\n\
               }\n\
               h.m[\"k\"] = 2\n\
               <-done\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert_eq!(r.diagnostics[0].code, "HYB-E3003");
    }

    #[test]
    fn self_shadowed_composite_define_preserves_may_share_and_outer_move() {
        let r = check(
            "struct holder { m map[string]int }\n\
             fn worker(h holder, done chan<- bool) { h.m[\"w\"] = 1\n done <- true }\n\
             fn main() {\n\
               done := make(chan bool, 1)\n\
               m := map[string]int{\"k\": 0}\n\
               h := holder{m: m}\n\
               if true {\n\
                 h := h\n\
                 task worker(move h, done)\n\
                 m[\"k\"] = 1\n\
               }\n\
               m[\"k\"] = 2\n\
               <-done\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert_eq!(r.diagnostics[0].code, "HYB-E3003");
    }
}

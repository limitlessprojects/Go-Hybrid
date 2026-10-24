//! zeroinit.rs — N5: a Hybrid-declared map's zero value auto-allocates.
//!
//! SPEC-LANGUAGE N5 (and T8, N3's zero-value table): `var m map[k]v;
//! m[k] = v` works — first write allocates — where Go panics on a nil map.
//! The emitter used to lower these declarations Go-exact, so an accepted
//! program built and then died with `assignment to entry in nil map`
//! (issue #645). D11 (keep N5 vs Go-exact) is still open; until it is
//! decided the emitted program follows the spec as written.
//!
//! Lowering: the zero value is allocated where it is DECLARED, never at the
//! write site — the write site would need the place's static type, and a
//! guard there evaluates the place twice. The spec makes the two the same
//! value: a Hybrid map has no `nil` (N1) and its zero value IS the empty
//! map (N3).
//!
//!   * `var m map[k]v`            → `var m map[k]v = map[k]v{}`
//!   * `var b bag` (bag has a map field, directly or through a nested
//!     struct field)              → `var b bag = bag{counts: map[string]int{}}`
//!   * `bag{}` / `bag{name: "x"}` → the unkeyed map fields are added
//!   * `new(bag)`                 → `&bag{counts: map[string]int{}}`
//!   * `var b box[u8]` / `box[u8]{}` / `new(box[u8])` → type arguments are
//!     substituted into the generic struct's fields so map-carrying generic
//!     structs allocate their zero-value maps as well (#645 / #722).

use std::collections::{BTreeMap, BTreeSet};

use hy_syntax::*;

use super::Emitter;

/// Nested-struct depth guard. Go value types are acyclic (a struct cannot
/// contain itself by value), so this only bounds pathological inputs.
const MAX_DEPTH: usize = 16;

impl Emitter {
    /// The Go initializer that gives a zero value of `ty` allocated maps
    /// (N5), or None when Go's zero value already is the Hybrid one.
    pub(crate) fn n5_zero_init(&mut self, ty: &TypeExpr) -> Option<String> {
        self.n5_zero_init_at(ty, 0)
    }

    fn n5_zero_init_at(&mut self, ty: &TypeExpr, depth: usize) -> Option<String> {
        if depth > MAX_DEPTH {
            return None;
        }
        match self.n5_resolve_alias(ty) {
            TypeExpr::Map { .. } => Some(format!("{}{{}}", self.type_text(ty))),
            TypeExpr::Named { name, .. } => {
                let fields = self.n5_resolve_struct_fields(&name)?;
                let extra = self.n5_missing_fields_at(&fields, &BTreeSet::new(), depth + 1);
                if extra.is_empty() {
                    None
                } else {
                    Some(format!("{}{{{}}}", self.type_text(ty), extra.join(", ")))
                }
            }
            _ => None,
        }
    }

    /// `name: init` entries for the map-carrying fields of the struct
    /// literal type `ty` that the literal does not key itself (`given`).
    pub(crate) fn n5_missing_fields(&mut self, ty: &TypeExpr, given: &[FieldInit]) -> Vec<String> {
        let TypeExpr::Named { name, .. } = self.n5_resolve_alias(ty) else {
            return Vec::new();
        };
        let Some(fields) = self.n5_resolve_struct_fields(&name) else {
            return Vec::new();
        };
        let given: BTreeSet<String> = given.iter().map(|f| f.name.clone()).collect();
        self.n5_missing_fields_at(&fields, &given, 1)
    }

    fn n5_missing_fields_at(
        &mut self,
        fields: &[FieldDecl],
        given: &BTreeSet<String>,
        depth: usize,
    ) -> Vec<String> {
        let mut out = Vec::new();
        for f in fields {
            if given.contains(&f.name) {
                continue;
            }
            if let Some(init) = self.n5_zero_init_at(&f.ty, depth) {
                out.push(format!("{}: {}", f.name, init));
            }
        }
        out
    }

    /// Look up the declared fields of `name`, substituting concrete type
    /// arguments when `name` is an instantiated generic struct (`box[u8]`,
    /// `table[string, bag]`).
    pub(crate) fn n5_resolve_struct_fields(&self, name: &str) -> Option<Vec<FieldDecl>> {
        let name = name.trim();
        if let Some(fields) = self.n5_structs.get(name) {
            return Some(fields.clone());
        }
        let open = name.find('[')?;
        if open == 0 || !name.ends_with(']') {
            return None;
        }
        let head = name[..open].trim();
        let inner = &name[open + 1..name.len() - 1];
        let type_args = Self::split_top_level_type_args(inner);
        let (param_specs, fields) = self.generic_struct_entry(head)?;
        if param_specs.len() != type_args.len() {
            return None;
        }
        let Some(first_field) = fields.first() else {
            return Some(Vec::new());
        };
        let mut subst = BTreeMap::new();
        for ((param_name, _), arg_str) in param_specs.into_iter().zip(type_args.into_iter()) {
            subst.insert(
                param_name,
                Self::parse_type_arg_str(arg_str, &first_field.ty),
            );
        }
        Some(
            fields
                .iter()
                .map(|f| FieldDecl {
                    name: f.name.clone(),
                    ty: Self::subst_type_expr(&f.ty, &subst),
                    span: f.span,
                })
                .collect(),
        )
    }

    /// Look up a generic struct `head` in `self.n5_structs` (encoded under key
    /// `"head[p1,p2:comparable]"`), returning `((param_name, is_comparable)[], fields)`.
    pub(crate) fn generic_struct_entry(
        &self,
        head: &str,
    ) -> Option<(Vec<(String, bool)>, &Vec<FieldDecl>)> {
        for (k, fields) in &self.n5_structs {
            let Some(rest) = k.strip_prefix(head).and_then(|r| r.strip_prefix('[')) else {
                continue;
            };
            let Some(params_str) = rest.strip_suffix(']') else {
                continue;
            };
            let specs: Vec<(String, bool)> = params_str
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(|entry| match entry.strip_suffix(":comparable") {
                    Some(p) => (p.trim().to_string(), true),
                    None => (entry.to_string(), false),
                })
                .collect();
            return Some((specs, fields));
        }
        None
    }

    pub(crate) fn parse_type_arg_str(s: &str, span_src: &TypeExpr) -> TypeExpr {
        let span = span_src.span();
        let s = s.trim();
        if let Some(rest) = s.strip_prefix('*') {
            return TypeExpr::Pointer {
                to: Box::new(Self::parse_type_arg_str(rest, span_src)),
                span,
            };
        }
        if let Some(rest) = s.strip_prefix("[]") {
            return TypeExpr::Slice {
                elem: Box::new(Self::parse_type_arg_str(rest, span_src)),
                span,
            };
        }
        if let Some(rest) = s.strip_prefix("<-chan ") {
            return TypeExpr::Chan {
                dir: ChanDir::Recv,
                elem: Box::new(Self::parse_type_arg_str(rest, span_src)),
                span,
            };
        }
        if let Some(rest) = s.strip_prefix("chan<- ") {
            return TypeExpr::Chan {
                dir: ChanDir::Send,
                elem: Box::new(Self::parse_type_arg_str(rest, span_src)),
                span,
            };
        }
        if let Some(rest) = s.strip_prefix("chan ") {
            return TypeExpr::Chan {
                dir: ChanDir::Both,
                elem: Box::new(Self::parse_type_arg_str(rest, span_src)),
                span,
            };
        }
        if let Some(rest) = s.strip_prefix("map[") {
            if let Some((k_s, v_s)) = Self::split_bracket_prefix(rest) {
                return TypeExpr::Map {
                    key: Box::new(Self::parse_type_arg_str(k_s, span_src)),
                    value: Box::new(Self::parse_type_arg_str(v_s, span_src)),
                    span,
                };
            }
        }
        if let Some(inner) = s.strip_prefix("option[").and_then(|r| r.strip_suffix(']')) {
            return TypeExpr::Option {
                elem: Box::new(Self::parse_type_arg_str(inner, span_src)),
                span,
            };
        }
        TypeExpr::Named {
            name: s.to_string(),
            span,
        }
    }

    pub(crate) fn subst_type_expr(
        ty: &TypeExpr,
        subst: &BTreeMap<String, TypeExpr>,
    ) -> TypeExpr {
        match ty {
            TypeExpr::Named { name, span } => {
                let trimmed = name.trim();
                if let Some(rep) = subst.get(trimmed) {
                    return rep.clone();
                }
                if let Some(open) = trimmed.find('[') {
                    if open > 0 && trimmed.ends_with(']') {
                        let head = &trimmed[..open];
                        let inner = &trimmed[open + 1..trimmed.len() - 1];
                        let args = Self::split_top_level_type_args(inner)
                            .into_iter()
                            .map(|a| {
                                let parsed = Self::parse_type_arg_str(a, ty);
                                Self::subst_type_expr(&parsed, subst).render()
                            })
                            .collect::<Vec<_>>()
                            .join(", ");
                        return TypeExpr::Named {
                            name: format!("{}[{}]", head, args),
                            span: *span,
                        };
                    }
                }
                TypeExpr::Named {
                    name: name.clone(),
                    span: *span,
                }
            }
            TypeExpr::Pointer { to, span } => TypeExpr::Pointer {
                to: Box::new(Self::subst_type_expr(to, subst)),
                span: *span,
            },
            TypeExpr::Slice { elem, span } => TypeExpr::Slice {
                elem: Box::new(Self::subst_type_expr(elem, subst)),
                span: *span,
            },
            TypeExpr::Array { len, elem, span } => TypeExpr::Array {
                len: len.clone(),
                elem: Box::new(Self::subst_type_expr(elem, subst)),
                span: *span,
            },
            TypeExpr::Map { key, value, span } => TypeExpr::Map {
                key: Box::new(Self::subst_type_expr(key, subst)),
                value: Box::new(Self::subst_type_expr(value, subst)),
                span: *span,
            },
            TypeExpr::Chan { dir, elem, span } => TypeExpr::Chan {
                dir: *dir,
                elem: Box::new(Self::subst_type_expr(elem, subst)),
                span: *span,
            },
            TypeExpr::Option { elem, span } => TypeExpr::Option {
                elem: Box::new(Self::subst_type_expr(elem, subst)),
                span: *span,
            },
            TypeExpr::Fn {
                params,
                results,
                span,
            } => TypeExpr::Fn {
                params: params
                    .iter()
                    .map(|p| Param {
                        name: p.name.clone(),
                        ty: Self::subst_type_expr(&p.ty, subst),
                        span: p.span,
                    })
                    .collect(),
                results: results
                    .iter()
                    .map(|r| Self::subst_type_expr(r, subst))
                    .collect(),
                span: *span,
            },
        }
    }

    /// Extract a `TypeExpr` from `new(T)`'s argument expression.
    pub(crate) fn new_arg_type_expr(mut val: &Expr) -> Option<TypeExpr> {
        while let Expr::Paren { inner, .. } = val {
            val = inner;
        }
        match val {
            Expr::Ident { name, span } => Some(TypeExpr::Named {
                name: name.clone(),
                span: *span,
            }),
            Expr::Field { base, name, span } => {
                if let TypeExpr::Named { name: b, .. } = Self::new_arg_type_expr(base)? {
                    Some(TypeExpr::Named {
                        name: format!("{}.{}", b, name),
                        span: *span,
                    })
                } else {
                    None
                }
            }
            Expr::Index { base, index, span } => {
                let TypeExpr::Named { name: b, .. } = Self::new_arg_type_expr(base)? else {
                    return None;
                };
                let idx_ty = Self::new_arg_type_expr(index)?;
                Some(TypeExpr::Named {
                    name: format!("{}[{}]", b, idx_ty.render()),
                    span: *span,
                })
            }
            Expr::StructLit {
                ty: Some(t),
                fields,
                ..
            } if fields.is_empty() => Some(t.clone()),
            Expr::SliceLit {
                elem: Some(t),
                items,
                span,
            } if items.is_empty() && val.as_array_lit().is_none() => Some(TypeExpr::Slice {
                elem: t.clone(),
                span: *span,
            }),
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
            _ => None,
        }
    }

    /// `new(T)` where T is a struct whose zero value needs N5 allocation → `&T{…}`.
    pub(crate) fn n5_new(&mut self, args: &[CallArg]) -> Option<String> {
        let [arg] = args else { return None };
        let ty = Self::new_arg_type_expr(&arg.value)?;
        if let TypeExpr::Named { name, .. } = &ty {
            let head = name.split('[').next().unwrap_or(name.as_str()).trim();
            // A local value named like a type is not a type argument.
            if self.local_vars.contains(head) {
                return None;
            }
        }
        let TypeExpr::Named {
            name: resolved_name,
            ..
        } = self.n5_resolve_alias(&ty)
        else {
            return None;
        };
        let fields = self.n5_resolve_struct_fields(&resolved_name)?;
        let extra = self.n5_missing_fields_at(&fields, &BTreeSet::new(), 1);
        if extra.is_empty() {
            None
        } else {
            Some(format!("&{}{{{}}}", self.type_text(&ty), extra.join(", ")))
        }
    }

    /// Follow user type aliases (`type counts = map[string]int`) to the
    /// type they name; anything else is returned as written.
    fn n5_resolve_alias(&self, ty: &TypeExpr) -> TypeExpr {
        let mut cur = ty.clone();
        for _ in 0..MAX_DEPTH {
            let next = match &cur {
                TypeExpr::Named { name, .. } => self.n5_aliases.get(name).cloned(),
                _ => None,
            };
            match next {
                Some(t) => cur = t,
                None => break,
            }
        }
        cur
    }
}

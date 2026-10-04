//! `syn` scan of one Rust source file: everything `hot-check` looks at, with line numbers. Items under
//! `#[cfg(test)]` are marked `test` (type and call-site rules skip them; `#[allow]` and `unsafe` rules
//! do not).

use proc_macro2::Span;
use syn::punctuated::Punctuated;
use syn::visit::{self, Visit};
use syn::{Attribute, Expr, Fields, Item, Meta, Token, Type, UseTree};

/// One finding with its line and whether it is inside `#[cfg(test)]` code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct At<T> {
    pub line: usize,
    pub test: bool,
    pub what: T,
}

/// A `#[repr(C)]` (or `repr(transparent)`) struct.
#[derive(Debug, Clone)]
pub struct StructDef {
    pub name: String,
    /// `repr(...)` arguments, e.g. `["C", "align(64)"]`.
    pub repr: Vec<String>,
    pub fields: Vec<(String, Type)>,
    pub generic: bool,
    /// Derives zerocopy `IntoBytes`/`AsBytes` (padding then fails at compile time).
    pub into_bytes: bool,
}

/// What one file contains.
#[derive(Debug, Default)]
pub struct FileScan {
    pub dyn_types: Vec<At<()>>,
    pub floats: Vec<At<String>>,
    /// `unsafe` blocks and `unsafe impl` (need `// SAFETY:`); `unsafe fn`/`unsafe trait` also count as
    /// `unsafe` uses (`kind`).
    pub unsafes: Vec<At<&'static str>>,
    pub spawns: Vec<At<String>>,
    pub mmap_mut: Vec<At<()>>,
    pub atomics: Vec<At<String>>,
    /// Paths referencing `model_math` or `hft_strat_lib::model`.
    pub model_refs: Vec<At<String>>,
    pub encrypt_calls: Vec<At<String>>,
    /// `#[allow]`/`#[expect]` (also inside `cfg_attr`): lint paths.
    pub allows: Vec<At<Vec<String>>>,
    pub structs: Vec<At<StructDef>>,
    /// `#[repr(uN)]` enums: name → bytes.
    pub enums: Vec<(String, u64)>,
    /// `const NAME: <int> = <literal>;`
    pub consts: Vec<(String, u64)>,
    /// `type A = B;`
    pub aliases: Vec<(String, Type)>,
    /// `#[cfg(test)] mod name;` declarations (files to treat as test code).
    pub test_mods: Vec<String>,
    pub global_allocator: bool,
}

fn line(s: Span) -> usize {
    s.start().line
}

fn is_cfg_test(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path().is_ident("cfg")
            && match &a.meta {
                Meta::List(l) => {
                    let t = l.tokens.to_string();
                    let has = t
                        .split(|c: char| !c.is_alphanumeric() && c != '_')
                        .any(|w| w == "test");
                    has && !t.replace(' ', "").contains("not(test")
                }
                _ => false,
            }
    })
}

fn path_str(p: &syn::Path) -> String {
    p.segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

/// Lint paths of an `allow`/`expect` attribute (also nested in `cfg_attr`).
fn allow_lints(a: &Attribute) -> Vec<String> {
    let parse_list = |tokens: &proc_macro2::TokenStream| -> Vec<String> {
        syn::parse::Parser::parse2(
            Punctuated::<syn::Path, Token![,]>::parse_terminated,
            tokens.clone(),
        )
        .map(|ps| ps.iter().map(path_str).collect())
        .unwrap_or_default()
    };
    if (a.path().is_ident("allow") || a.path().is_ident("expect"))
        && let Meta::List(l) = &a.meta
    {
        return parse_list(&l.tokens);
    }
    if a.path().is_ident("cfg_attr")
        && let Meta::List(l) = &a.meta
        && let Ok(metas) =
            syn::parse::Parser::parse2(Punctuated::<Meta, Token![,]>::parse_terminated, l.tokens.clone())
    {
        return metas
            .iter()
            .skip(1)
            .filter_map(|m| match m {
                Meta::List(ml) if ml.path.is_ident("allow") || ml.path.is_ident("expect") => {
                    Some(parse_list(&ml.tokens))
                }
                _ => None,
            })
            .flatten()
            .collect();
    }
    Vec::new()
}

fn repr_args(attrs: &[Attribute]) -> Vec<String> {
    let mut out = Vec::new();
    for a in attrs {
        if a.path().is_ident("repr")
            && let Meta::List(l) = &a.meta
            && let Ok(ms) =
                syn::parse::Parser::parse2(Punctuated::<Meta, Token![,]>::parse_terminated, l.tokens.clone())
        {
            for m in ms {
                out.push(match m {
                    Meta::Path(p) => path_str(&p),
                    Meta::List(ml) => format!("{}({})", path_str(&ml.path), ml.tokens),
                    Meta::NameValue(nv) => path_str(&nv.path),
                });
            }
        }
    }
    out
}

fn derives(attrs: &[Attribute]) -> Vec<String> {
    let mut out = Vec::new();
    for a in attrs {
        if a.path().is_ident("derive")
            && let Meta::List(l) = &a.meta
            && let Ok(ps) = syn::parse::Parser::parse2(
                Punctuated::<syn::Path, Token![,]>::parse_terminated,
                l.tokens.clone(),
            )
        {
            out.extend(
                ps.iter()
                    .filter_map(|p| p.segments.last().map(|s| s.ident.to_string())),
            );
        }
    }
    out
}

fn item_attrs(i: &Item) -> &[Attribute] {
    match i {
        Item::Const(x) => &x.attrs,
        Item::Enum(x) => &x.attrs,
        Item::ExternCrate(x) => &x.attrs,
        Item::Fn(x) => &x.attrs,
        Item::ForeignMod(x) => &x.attrs,
        Item::Impl(x) => &x.attrs,
        Item::Macro(x) => &x.attrs,
        Item::Mod(x) => &x.attrs,
        Item::Static(x) => &x.attrs,
        Item::Struct(x) => &x.attrs,
        Item::Trait(x) => &x.attrs,
        Item::TraitAlias(x) => &x.attrs,
        Item::Type(x) => &x.attrs,
        Item::Union(x) => &x.attrs,
        Item::Use(x) => &x.attrs,
        _ => &[],
    }
}

fn int_lit(e: &Expr) -> Option<u64> {
    match e {
        Expr::Lit(l) => match &l.lit {
            syn::Lit::Int(i) => i.base10_parse().ok(),
            _ => None,
        },
        Expr::Paren(p) => int_lit(&p.expr),
        Expr::Group(g) => int_lit(&g.expr),
        _ => None,
    }
}

/// Whether an expression mentions a path segment `name`.
fn mentions(e: &Expr, name: &str) -> bool {
    struct M<'a>(&'a str, bool);
    impl<'ast> Visit<'ast> for M<'_> {
        fn visit_path_segment(&mut self, s: &'ast syn::PathSegment) {
            if s.ident == self.0 {
                self.1 = true;
            }
            visit::visit_path_segment(self, s);
        }
    }
    let mut m = M(name, false);
    m.visit_expr(e);
    m.1
}

fn use_paths(t: &UseTree, prefix: &mut Vec<String>, out: &mut Vec<Vec<String>>) {
    match t {
        UseTree::Path(p) => {
            prefix.push(p.ident.to_string());
            use_paths(&p.tree, prefix, out);
            prefix.pop();
        }
        UseTree::Name(n) => {
            let mut v = prefix.clone();
            v.push(n.ident.to_string());
            out.push(v);
        }
        UseTree::Rename(r) => {
            let mut v = prefix.clone();
            v.push(r.ident.to_string());
            out.push(v);
        }
        UseTree::Glob(_) => out.push(prefix.clone()),
        UseTree::Group(g) => {
            for i in &g.items {
                use_paths(i, prefix, out);
            }
        }
    }
}

fn model_ref(segs: &[String]) -> Option<String> {
    let hit = segs.iter().any(|s| s == "model_math")
        || segs
            .windows(2)
            .any(|w| w[0] == "hft_strat_lib" && w[1] == "model");
    hit.then(|| segs.join("::"))
}

struct V<'a> {
    out: &'a mut FileScan,
    test: u32,
}

impl V<'_> {
    fn t(&self) -> bool {
        self.test > 0
    }
}

impl<'ast> Visit<'ast> for V<'_> {
    fn visit_item(&mut self, i: &'ast Item) {
        let test = is_cfg_test(item_attrs(i));
        if test {
            self.test += 1;
        }
        if let Item::Mod(m) = i
            && test
            && m.content.is_none()
        {
            self.out.test_mods.push(m.ident.to_string());
        }
        visit::visit_item(self, i);
        if test {
            self.test -= 1;
        }
    }

    fn visit_impl_item(&mut self, i: &'ast syn::ImplItem) {
        let attrs: &[Attribute] = match i {
            syn::ImplItem::Fn(f) => &f.attrs,
            syn::ImplItem::Const(c) => &c.attrs,
            syn::ImplItem::Type(t) => &t.attrs,
            _ => &[],
        };
        let test = is_cfg_test(attrs);
        if test {
            self.test += 1;
        }
        visit::visit_impl_item(self, i);
        if test {
            self.test -= 1;
        }
    }

    fn visit_attribute(&mut self, a: &'ast Attribute) {
        let lints = allow_lints(a);
        if !lints.is_empty() {
            self.out.allows.push(At {
                line: line(a.pound_token.span),
                test: self.t(),
                what: lints,
            });
        }
        if a.path().is_ident("global_allocator") {
            self.out.global_allocator = true;
        }
        visit::visit_attribute(self, a);
    }

    fn visit_type(&mut self, t: &'ast Type) {
        match t {
            Type::TraitObject(o) => self.out.dyn_types.push(At {
                line: o.dyn_token.map_or(0, |d| line(d.span)),
                test: self.t(),
                what: (),
            }),
            Type::Path(p) if p.qself.is_none() => {
                if let Some(id) = p.path.get_ident()
                    && (id == "f32" || id == "f64")
                {
                    self.out.floats.push(At {
                        line: line(id.span()),
                        test: self.t(),
                        what: id.to_string(),
                    });
                }
                if let Some(last) = p.path.segments.last() {
                    let n = last.ident.to_string();
                    if n.starts_with("Atomic") && n.len() > "Atomic".len() {
                        self.out.atomics.push(At {
                            line: line(last.ident.span()),
                            test: self.t(),
                            what: n,
                        });
                    }
                }
            }
            _ => {}
        }
        visit::visit_type(self, t);
    }

    fn visit_path(&mut self, p: &'ast syn::Path) {
        let segs: Vec<String> = p.segments.iter().map(|s| s.ident.to_string()).collect();
        if let Some(first) = p.segments.first() {
            let l = line(first.ident.span());
            if segs.iter().any(|s| s == "MmapMut") {
                self.out.mmap_mut.push(At {
                    line: l,
                    test: self.t(),
                    what: (),
                });
            }
            if let Some(r) = model_ref(&segs) {
                self.out.model_refs.push(At {
                    line: l,
                    test: self.t(),
                    what: r,
                });
            }
            if segs.ends_with(&["WriteTraffic".to_string(), "encrypt".to_string()]) {
                self.out.encrypt_calls.push(At {
                    line: l,
                    test: self.t(),
                    what: segs.join("::"),
                });
            }
        }
        visit::visit_path(self, p);
    }

    fn visit_item_use(&mut self, u: &'ast syn::ItemUse) {
        let mut out = Vec::new();
        use_paths(&u.tree, &mut Vec::new(), &mut out);
        for segs in out {
            if let Some(r) = model_ref(&segs) {
                self.out.model_refs.push(At {
                    line: line(u.use_token.span),
                    test: self.t(),
                    what: r,
                });
            }
            if segs.iter().any(|s| s == "MmapMut") {
                self.out.mmap_mut.push(At {
                    line: line(u.use_token.span),
                    test: self.t(),
                    what: (),
                });
            }
        }
        visit::visit_item_use(self, u);
    }

    fn visit_expr_unsafe(&mut self, e: &'ast syn::ExprUnsafe) {
        self.out.unsafes.push(At {
            line: line(e.unsafe_token.span),
            test: self.t(),
            what: "block",
        });
        visit::visit_expr_unsafe(self, e);
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        if let Some(u) = i.unsafety {
            self.out.unsafes.push(At {
                line: line(u.span),
                test: self.t(),
                what: "impl",
            });
        }
        visit::visit_item_impl(self, i);
    }

    fn visit_signature(&mut self, s: &'ast syn::Signature) {
        if let Some(u) = s.unsafety {
            self.out.unsafes.push(At {
                line: line(u.span),
                test: self.t(),
                what: "fn",
            });
        }
        visit::visit_signature(self, s);
    }

    fn visit_item_trait(&mut self, i: &'ast syn::ItemTrait) {
        if let Some(u) = i.unsafety {
            self.out.unsafes.push(At {
                line: line(u.span),
                test: self.t(),
                what: "trait",
            });
        }
        visit::visit_item_trait(self, i);
    }

    fn visit_expr_call(&mut self, c: &'ast syn::ExprCall) {
        if let Expr::Path(p) = &*c.func {
            let segs: Vec<String> = p.path.segments.iter().map(|s| s.ident.to_string()).collect();
            let n = segs.len();
            if n >= 2 && segs[n - 1] == "spawn" && (segs[n - 2] == "thread" || segs[n - 2] == "Builder") {
                self.out.spawns.push(At {
                    line: line(p.path.segments[0].ident.span()),
                    test: self.t(),
                    what: segs.join("::"),
                });
            }
        }
        visit::visit_expr_call(self, c);
    }

    fn visit_expr_method_call(&mut self, m: &'ast syn::ExprMethodCall) {
        let name = m.method.to_string();
        if (name == "spawn" || name == "spawn_scoped") && mentions(&m.receiver, "Builder") {
            self.out.spawns.push(At {
                line: line(m.method.span()),
                test: self.t(),
                what: format!("Builder::{name}"),
            });
        }
        if name == "encrypt" {
            self.out.encrypt_calls.push(At {
                line: line(m.method.span()),
                test: self.t(),
                what: ".encrypt()".into(),
            });
        }
        visit::visit_expr_method_call(self, m);
    }

    fn visit_item_struct(&mut self, s: &'ast syn::ItemStruct) {
        let repr = repr_args(&s.attrs);
        let named = |f: &Fields| -> Vec<(String, Type)> {
            f.iter()
                .enumerate()
                .map(|(i, x)| {
                    (
                        x.ident.as_ref().map_or(i.to_string(), |n| n.to_string()),
                        x.ty.clone(),
                    )
                })
                .collect()
        };
        let ds = derives(&s.attrs);
        self.out.structs.push(At {
            line: line(s.ident.span()),
            test: self.t(),
            what: StructDef {
                name: s.ident.to_string(),
                repr,
                fields: named(&s.fields),
                generic: !s.generics.params.is_empty(),
                into_bytes: ds.iter().any(|d| d == "IntoBytes" || d == "AsBytes"),
            },
        });
        visit::visit_item_struct(self, s);
    }

    fn visit_item_enum(&mut self, e: &'ast syn::ItemEnum) {
        for r in repr_args(&e.attrs) {
            let b = match r.as_str() {
                "u8" | "i8" => 1,
                "u16" | "i16" => 2,
                "u32" | "i32" => 4,
                "u64" | "i64" => 8,
                _ => continue,
            };
            self.out.enums.push((e.ident.to_string(), b));
        }
        visit::visit_item_enum(self, e);
    }

    fn visit_item_const(&mut self, c: &'ast syn::ItemConst) {
        if let Some(v) = int_lit(&c.expr) {
            self.out.consts.push((c.ident.to_string(), v));
        }
        visit::visit_item_const(self, c);
    }

    fn visit_item_type(&mut self, t: &'ast syn::ItemType) {
        if t.generics.params.is_empty() {
            self.out.aliases.push((t.ident.to_string(), (*t.ty).clone()));
        }
        visit::visit_item_type(self, t);
    }
}

/// Scans one file; `Err` = it does not parse.
pub fn scan(src: &str) -> Result<FileScan, String> {
    let f = syn::parse_file(src).map_err(|e| format!("{e} (line {})", e.span().start().line))?;
    let mut out = FileScan::default();
    let mut v = V {
        out: &mut out,
        test: 0,
    };
    v.visit_file(&f);
    Ok(out)
}

/// Inner attributes of a file (`#![…]`): `(path, lints)` for `forbid`/`deny`/`warn`/`allow`/`expect`.
pub fn inner_lint_levels(src: &str) -> Result<Vec<(String, String)>, String> {
    let f = syn::parse_file(src).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for a in &f.attrs {
        let lvl = path_str(a.path());
        if !matches!(lvl.as_str(), "forbid" | "deny" | "warn" | "allow" | "expect") {
            continue;
        }
        if let Meta::List(l) = &a.meta
            && let Ok(ps) = syn::parse::Parser::parse2(
                Punctuated::<syn::Path, Token![,]>::parse_terminated,
                l.tokens.clone(),
            )
        {
            for p in ps {
                out.push((lvl.clone(), path_str(&p)));
            }
        }
    }
    Ok(out)
}

//! 语义解析模块
//!
//! 两遍遍历 AST，产出 ResolutionDB：
//!   pass1 – 收集所有定义（class / mixin / fn / field）
//!   pass2 – 带类型上下文解析所有"使用"，把每个 use-span 绑定到 def-span
//!
//! 之后 goto-def / find-references / rename 全部查这张表，不再各自遍历 AST。

use std::collections::HashMap;
use nb_core::lexer::Lexer;
use nb_core::parser::{Parser, ast::*};

use crate::symbol_table::{type_ann_str, SymbolInfo};

// ── 公共数据结构 ──────────────────────────────────────────────────────────────

/// 每个定义的详细信息（用于 hover）
pub type DefInfo = SymbolInfo;

#[derive(Debug, Default)]
pub struct ResolutionDB {
    /// use-span → def-span（光标在使用处，跳到定义）
    pub use_to_def: HashMap<Span, Span>,
    /// def-span → [use-span]（光标在定义处，找所有引用）
    pub def_to_uses: HashMap<Span, Vec<Span>>,
    /// def-span → 符号信息（hover 渲染）
    pub def_info: HashMap<Span, DefInfo>,
}

impl ResolutionDB {
    /// 光标在 use 或 def 处，返回对应的 def-span
    pub fn resolve_def(&self, span: Span) -> Option<Span> {
        if let Some(&def) = self.use_to_def.get(&span) {
            return Some(def);
        }
        if self.def_info.contains_key(&span) {
            return Some(span); // 光标已在定义处
        }
        None
    }

    /// 给定一个 use 或 def span，返回所有引用 span（不含定义自身）
    pub fn find_references(&self, span: Span) -> Vec<Span> {
        let def = match self.use_to_def.get(&span) {
            Some(&d) => d,
            None => span, // 假设是 def 本身
        };
        self.def_to_uses.get(&def).cloned().unwrap_or_default()
    }

    /// 给定一个 use 或 def span，返回所有引用 + 定义本身
    pub fn find_all_occurrences(&self, span: Span) -> Vec<Span> {
        let def = match self.use_to_def.get(&span) {
            Some(&d) => d,
            None => span,
        };
        let mut result = vec![def];
        if let Some(uses) = self.def_to_uses.get(&def) {
            result.extend(uses);
        }
        result
    }
}

// ── 内部构建状态 ──────────────────────────────────────────────────────────────

/// Pass1 产出：所有已知定义
#[derive(Default)]
struct Definitions {
    /// 类名 → (ClassDef, def-span)
    classes: HashMap<String, (ClassDef, Span)>,
    /// 类名 → receiver 方法列表
    methods: HashMap<String, Vec<FnDef>>,
    /// mixin名 → (MixinDef, def-span)
    mixins: HashMap<String, (MixinDef, Span)>,
    /// 类名.字段名 → (FieldDef, def-span)
    fields: HashMap<(String, String), (FieldDef, Span)>,
    /// 顶层函数名 → (FnDef, def-span)
    functions: HashMap<String, (FnDef, Span)>,
}

/// Pass2 遍历时携带的作用域
#[derive(Clone)]
struct Scope {
    /// 变量名 → (def-span, 推断的类名)
    vars: HashMap<String, (Span, Option<String>)>,
    /// 当前所在方法的 self 类型
    self_type: Option<String>,
}

impl Scope {
    fn new() -> Self {
        Self { vars: HashMap::new(), self_type: None }
    }
    fn with_self(self_type: String) -> Self {
        Self { vars: HashMap::new(), self_type: Some(self_type) }
    }
    fn child(&self) -> Self {
        Self {
            vars: self.vars.clone(),
            self_type: self.self_type.clone(),
        }
    }
    fn define(&mut self, name: String, def_span: Span, type_name: Option<String>) {
        self.vars.insert(name, (def_span, type_name));
    }
    fn lookup_var(&self, name: &str) -> Option<(Span, Option<String>)> {
        self.vars.get(name).cloned()
    }
}

// ── 入口 ──────────────────────────────────────────────────────────────────────

pub fn build_resolution_db(source: &str) -> Option<ResolutionDB> {
    let tokens = Lexer::new(source).tokenize().ok()?;
    let stmts  = Parser::new(tokens).parse_program().ok()?;

    let defs = pass1_collect_definitions(&stmts);
    let db   = pass2_resolve(&stmts, &defs);
    Some(db)
}

// ── Pass 1：收集所有定义 ──────────────────────────────────────────────────────

fn pass1_collect_definitions(stmts: &[Stmt]) -> Definitions {
    let mut defs = Definitions::default();
    for stmt in stmts {
        pass1_stmt(stmt, &mut defs);
    }
    defs
}

fn pass1_stmt(stmt: &Stmt, defs: &mut Definitions) {
    match stmt {
        Stmt::ClassDef(cd) => {
            defs.classes.insert(cd.name.clone(), (cd.clone(), cd.name_span));
            for field in &cd.fields {
                defs.fields.insert(
                    (cd.name.clone(), field.name.clone()),
                    (field.clone(), field.name_span),
                );
            }
        }
        Stmt::MixinDef(md) => {
            defs.mixins.insert(md.name.clone(), (md.clone(), md.name_span));
            // mixin 的 require 字段只在 mixin 内部可见，不注册到 fields
        }
        Stmt::FnDef(f) => {
            if let Some(name) = &f.name {
                if let Some(receiver) = &f.receiver {
                    defs.methods
                        .entry(receiver.clone())
                        .or_default()
                        .push(f.clone());
                } else {
                    defs.functions.insert(name.clone(), (f.clone(), f.name_span));
                }
            }
        }
        Stmt::If { then_body, else_ifs, else_body, .. } => {
            for s in then_body { pass1_stmt(s, defs); }
            for (_, b) in else_ifs { for s in b { pass1_stmt(s, defs); } }
            if let Some(b) = else_body { for s in b { pass1_stmt(s, defs); } }
        }
        Stmt::While { body, .. } | Stmt::ForIn { body, .. } => {
            for s in body { pass1_stmt(s, defs); }
        }
        _ => {}
    }
}

// ── Pass 2：带上下文解析所有使用 ─────────────────────────────────────────────

fn pass2_resolve(stmts: &[Stmt], defs: &Definitions) -> ResolutionDB {
    let mut db = ResolutionDB::default();

    // 把所有定义先写入 def_info
    register_all_defs(defs, &mut db);

    // 顶层作用域
    let mut scope = Scope::new();
    resolve_stmts(stmts, &mut scope, defs, &mut db);

    db
}

fn register_all_defs(defs: &Definitions, db: &mut ResolutionDB) {
    for (name, (cd, span)) in &defs.classes {
        db.def_info.insert(*span, SymbolInfo::Class {
            name: name.clone(),
            mixins: cd.mixins.clone(),
            fields: cd.fields.clone(),
        });
        // 每个字段的定义
        for field in &cd.fields {
            db.def_info.insert(field.name_span, SymbolInfo::Field {
                name: field.name.clone(),
                class_name: name.clone(),
                mutable: field.mutable,
                type_ann: field.type_ann.clone(),
            });
        }
    }
    for (name, (md, span)) in &defs.mixins {
        db.def_info.insert(*span, SymbolInfo::Mixin {
            name: name.clone(),
            requires: md.requires.clone(),
            methods: md.methods.clone(),
        });
        // mixin 方法定义
        for f in &md.methods {
            if let Some(fname) = &f.name {
                db.def_info.insert(f.name_span, SymbolInfo::Function {
                    name: fname.clone(),
                    receiver: Some(name.clone()),
                    params: f.params.clone(),
                    ret_type: f.ret_type.clone(),
                    async_: f.async_,
                    throws: f.throws,
                });
            }
        }
    }
    for (name, (f, span)) in &defs.functions {
        db.def_info.insert(*span, SymbolInfo::Function {
            name: name.clone(),
            receiver: None,
            params: f.params.clone(),
            ret_type: f.ret_type.clone(),
            async_: f.async_,
            throws: f.throws,
        });
    }
    for ((class_name, _), (f, span)) in defs.methods.iter().flat_map(|(cn, fns)| {
        fns.iter().map(move |f| ((cn, f.name.as_deref().unwrap_or("")), (f, f.name_span)))
    }) {
        db.def_info.insert(span, SymbolInfo::Function {
            name: f.name.clone().unwrap_or_default(),
            receiver: Some(class_name.clone()),
            params: f.params.clone(),
            ret_type: f.ret_type.clone(),
            async_: f.async_,
            throws: f.throws,
        });
    }
}

fn record_use(db: &mut ResolutionDB, use_span: Span, def_span: Span) {
    db.use_to_def.insert(use_span, def_span);
    db.def_to_uses.entry(def_span).or_default().push(use_span);
}

// ── 语句遍历 ──────────────────────────────────────────────────────────────────

fn resolve_stmts(stmts: &[Stmt], scope: &mut Scope, defs: &Definitions, db: &mut ResolutionDB) {
    for stmt in stmts {
        resolve_stmt(stmt, scope, defs, db);
    }
}

fn resolve_stmt(stmt: &Stmt, scope: &mut Scope, defs: &Definitions, db: &mut ResolutionDB) {
    match stmt {
        Stmt::Let { name, name_span, value, .. } => {
            // 先解析右侧（避免自引用）
            let type_name = value.as_ref().and_then(|e| {
                resolve_expr(e, scope, defs, db);
                infer_type(e, scope)
            });
            scope.define(name.clone(), *name_span, type_name.clone());
            // 把 let 变量定义写入 def_info
            db.def_info.entry(*name_span).or_insert_with(|| SymbolInfo::Variable {
                name: name.clone(),
                mutable: matches!(stmt, Stmt::Let { mutable: true, .. }),
                type_ann: None,
            });
        }
        Stmt::FnDef(f) => {
            // fn 定义本身已在 register_all_defs 写入
            // 解析函数体，带 self_type
            let self_type = f.receiver.clone();
            let mut fn_scope = if let Some(ref st) = self_type {
                Scope::with_self(st.clone())
            } else {
                scope.child()
            };
            // 参数加入作用域
            for p in &f.params {
                if p.name != "self" {
                    fn_scope.define(p.name.clone(), p.name_span, None);
                    db.def_info.entry(p.name_span).or_insert_with(|| SymbolInfo::Parameter {
                        name: p.name.clone(),
                        mutable: p.mutable,
                        type_ann: p.type_ann.clone(),
                    });
                }
            }
            resolve_stmts(&f.body, &mut fn_scope, defs, db);
        }
        Stmt::ClassDef(cd) => {
            // 类名本身是使用处？不，这里是定义处，def_info 已注册
            // 解析 mixin 名作为对 mixin 定义的引用
            for mixin_name in &cd.mixins {
                if let Some((_, mixin_def_span)) = defs.mixins.get(mixin_name) {
                    // mixin 名在 AST 里没有 span，暂时跳过
                    let _ = mixin_def_span;
                }
            }
        }
        Stmt::MixinDef(md) => {
            // 解析 mixin 方法体，self_type = mixin 名（用于 require 字段查找）
            for f in &md.methods {
                let mut fn_scope = Scope::with_self(md.name.clone());
                for p in &f.params {
                    if p.name != "self" {
                        fn_scope.define(p.name.clone(), p.name_span, None);
                    }
                }
                resolve_stmts(&f.body, &mut fn_scope, defs, db);
            }
        }
        Stmt::Assign { target, value } | Stmt::CompoundAssign { target, value, .. } => {
            resolve_expr(target, scope, defs, db);
            resolve_expr(value, scope, defs, db);
        }
        Stmt::IncDec { target, .. } => resolve_expr(target, scope, defs, db),
        Stmt::Return(Some(e)) => resolve_expr(e, scope, defs, db),
        Stmt::Throw(e)        => resolve_expr(e, scope, defs, db),
        Stmt::Expr(e)         => resolve_expr(e, scope, defs, db),
        Stmt::If { cond, then_body, else_ifs, else_body, .. } => {
            resolve_expr(cond, scope, defs, db);
            let mut child = scope.child();
            resolve_stmts(then_body, &mut child, defs, db);
            for (c, b) in else_ifs {
                resolve_expr(c, scope, defs, db);
                let mut child = scope.child();
                resolve_stmts(b, &mut child, defs, db);
            }
            if let Some(b) = else_body {
                let mut child = scope.child();
                resolve_stmts(b, &mut child, defs, db);
            }
        }
        Stmt::While { cond, body, .. } => {
            resolve_expr(cond, scope, defs, db);
            let mut child = scope.child();
            resolve_stmts(body, &mut child, defs, db);
        }
        Stmt::ForIn { key, value, iter, body, .. } => {
            resolve_expr(iter, scope, defs, db);
            let mut child = scope.child();
            // key/value 变量（无 span 可用，跳过精确绑定）
            let _ = key;
            let _ = value;
            resolve_stmts(body, &mut child, defs, db);
        }
        _ => {}
    }
}

// ── 表达式遍历 ────────────────────────────────────────────────────────────────

fn resolve_expr(expr: &Expr, scope: &Scope, defs: &Definitions, db: &mut ResolutionDB) {
    match expr {
        Expr::Ident(name, span) => {
            resolve_ident(name, *span, scope, defs, db);
        }
        Expr::Field { obj, field, field_span } => {
            resolve_expr(obj, scope, defs, db);
            // 推断 obj 的类型，再查字段/方法定义
            let class_name = infer_type(obj, scope);
            resolve_field_access(field, *field_span, class_name.as_deref(), defs, db);
        }
        Expr::Call { callee, args, .. } => {
            resolve_expr(callee, scope, defs, db);
            for arg in args { resolve_expr(&arg.expr, scope, defs, db); }
        }
        Expr::StructLit { class, class_span, fields } => {
            // 类名是使用处 → 指向 ClassDef 的 def-span
            if let Some((_, def_span)) = defs.classes.get(class) {
                record_use(db, *class_span, *def_span);
            }
            // 每个字段名是使用处 → 指向 ClassDef 字段的 def-span
            for (fname, fname_span, fval) in fields {
                if let Some((_, fdef_span)) = defs.fields.get(&(class.clone(), fname.clone())) {
                    record_use(db, *fname_span, *fdef_span);
                }
                resolve_expr(fval, scope, defs, db);
            }
        }
        Expr::Index { obj, idx } => {
            resolve_expr(obj, scope, defs, db);
            resolve_expr(idx, scope, defs, db);
        }
        Expr::BinOp { left, right, .. } => {
            resolve_expr(left, scope, defs, db);
            resolve_expr(right, scope, defs, db);
        }
        Expr::UnaryOp { expr, .. } => resolve_expr(expr, scope, defs, db),
        Expr::Ternary { cond, then, else_ } => {
            resolve_expr(cond, scope, defs, db);
            resolve_expr(then, scope, defs, db);
            resolve_expr(else_, scope, defs, db);
        }
        Expr::Is { expr, type_name, type_span } => {
            resolve_expr(expr, scope, defs, db);
            // type_name 是类名或 mixin 名的引用
            if let Some((_, def_span)) = defs.classes.get(type_name) {
                record_use(db, *type_span, *def_span);
            } else if let Some((_, def_span)) = defs.mixins.get(type_name) {
                record_use(db, *type_span, *def_span);
            }
        }
        Expr::InterpolatedString(parts) => {
            for part in parts {
                if let nb_core::parser::ast::InterpPart::Expr(expr) = part {
                    resolve_expr(expr, scope, defs, db);
                }
            }
        }
        Expr::Fn(f) => {
            let mut fn_scope = scope.child();
            for p in &f.params {
                if p.name != "self" {
                    fn_scope.define(p.name.clone(), p.name_span, None);
                }
            }
            resolve_stmts(&f.body, &mut fn_scope, defs, db);
        }
        Expr::Protect(stmts) => {
            let mut child = scope.child();
            resolve_stmts(stmts, &mut child, defs, db);
        }
        Expr::Array(elems) => {
            for e in elems { resolve_expr(e, scope, defs, db); }
        }
        Expr::Dict(pairs) => {
            for (k, v) in pairs {
                resolve_expr(k, scope, defs, db);
                resolve_expr(v, scope, defs, db);
            }
        }
        Expr::Await(e) | Expr::Try(e) => resolve_expr(e, scope, defs, db),
        _ => {}
    }
}

/// 解析一个普通标识符引用
fn resolve_ident(name: &str, span: Span, scope: &Scope, defs: &Definitions, db: &mut ResolutionDB) {
    // self 不绑定到类定义（避免污染类的 find-references），直接跳过
    if name == "self" { return; }
    // 1. 局部变量 / 参数
    if let Some((def_span, _)) = scope.lookup_var(name) {
        record_use(db, span, def_span);
        return;
    }
    // 2. 顶层函数
    if let Some((_, def_span)) = defs.functions.get(name) {
        record_use(db, span, *def_span);
        return;
    }
    // 3. 类名
    if let Some((_, def_span)) = defs.classes.get(name) {
        record_use(db, span, *def_span);
        return;
    }
    // 4. mixin 名
    if let Some((_, def_span)) = defs.mixins.get(name) {
        record_use(db, span, *def_span);
    }
}

/// 解析 obj.field 中的 field 部分
fn resolve_field_access(
    field: &str,
    field_span: Span,
    class_name: Option<&str>,
    defs: &Definitions,
    db: &mut ResolutionDB,
) {
    let Some(class_name) = class_name else { return; };

    // 查 class 自身字段
    if let Some((_, def_span)) = defs.fields.get(&(class_name.to_string(), field.to_string())) {
        record_use(db, field_span, *def_span);
        return;
    }
    // 查 receiver 方法
    if let Some(methods) = defs.methods.get(class_name) {
        if let Some(f) = methods.iter().find(|f| f.name.as_deref() == Some(field)) {
            record_use(db, field_span, f.name_span);
            return;
        }
    }
    // 查 mixin 方法（按 class 的 mixins 列表顺序）
    if let Some((cd, _)) = defs.classes.get(class_name) {
        for mixin_name in &cd.mixins {
            if let Some((md, _)) = defs.mixins.get(mixin_name) {
                if let Some(f) = md.methods.iter().find(|f| f.name.as_deref() == Some(field)) {
                    record_use(db, field_span, f.name_span);
                    return;
                }
            }
        }
    }
}

// ── 类型推断 ──────────────────────────────────────────────────────────────────

/// 对一个表达式做轻量类型推断，返回类名（String）
fn infer_type(expr: &Expr, scope: &Scope) -> Option<String> {
    match expr {
        Expr::Ident(name, _) => {
            if name == "self" {
                return scope.self_type.clone();
            }
            scope.lookup_var(name).and_then(|(_, t)| t)
        }
        Expr::StructLit { class, .. } => Some(class.clone()),
        // obj.field 的类型推断（浅层，不追踪方法返回值）
        _ => None,
    }
}

// ── 公共查询工具 ──────────────────────────────────────────────────────────────

/// 从 ResolutionDB（已包含所有绝对坐标的 span）中找光标位置命中的 span。
/// 同时也扫描顶层 token 流里的普通 Ident（cover 未被 DB 收录的位置）。
pub fn span_at_position(source: &str, pos: tower_lsp::lsp_types::Position) -> Option<Span> {
    let target_line = (pos.line + 1) as usize;
    let target_col  = (pos.character + 1) as usize;

    // 只扫顶层 token 流中的普通 Ident（插值内的 ident 不在顶层流）
    let tokens = Lexer::new(source).tokenize().ok()?;
    for twp in &tokens {
        if twp.line != target_line { continue; }
        if let nb_core::lexer::Token::Ident(name) = &twp.token {
            let end = twp.col + name.len();
            if target_col >= twp.col && target_col < end {
                return Some(Span::new(twp.line, twp.col));
            }
        }
    }
    None
}

/// 在 ResolutionDB 里查找覆盖目标位置的 span（包含插值字符串内的 ident）。
/// 先用 span_at_position 找 token 流中的 span；找不到时在 DB 中枚举。
pub fn span_at_position_with_db(
    db: &ResolutionDB,
    source: &str,
    pos: tower_lsp::lsp_types::Position,
) -> Option<Span> {
    // 先试 token 流（快路径，覆盖绝大多数情况）
    if let Some(s) = span_at_position(source, pos) {
        return Some(s);
    }

    // 慢路径：在 DB 收录的所有 use-span 和 def-span 中查找
    // （插值字符串内的 ident 通过 parser 已获得绝对坐标，会被 record_use 写入 DB）
    let target_line = (pos.line + 1) as usize;
    let target_col  = (pos.character + 1) as usize;

    // 收集所有已知 span
    let mut candidates: Vec<Span> = Vec::new();
    candidates.extend(db.use_to_def.keys().copied());
    candidates.extend(db.def_info.keys().copied());

    for span in candidates {
        if span.line != target_line { continue; }
        // 从 def_info 或 use_to_def 反推符号名长度
        let name_len = if let Some(info) = db.def_info.get(&span) {
            info.name().len()
        } else if let Some(def) = db.use_to_def.get(&span) {
            db.def_info.get(def).map(|i| i.name().len()).unwrap_or(1)
        } else {
            1
        };
        let end = span.col + name_len;
        if target_col >= span.col && target_col < end {
            return Some(span);
        }
    }
    None
}

/// Span（AST 1-based）→ LSP Range（0-based）
pub fn span_to_range(span: &Span, name_len: u32) -> tower_lsp::lsp_types::Range {
    use tower_lsp::lsp_types::{Position, Range};
    let start = Position::new(
        (span.line as u32).saturating_sub(1),
        (span.col  as u32).saturating_sub(1),
    );
    Range::new(start, Position::new(start.line, start.character + name_len))
}

/// 根据 span 查符号名长度（从 def_info 取）
pub fn name_len_at(db: &ResolutionDB, span: Span) -> u32 {
    db.def_info.get(&span).map(|i| i.name().len() as u32).unwrap_or(1)
}

/// 把 ResolutionDB 的 span 解析为 LSP Location
pub fn span_to_location(span: Span, name_len: u32, uri: &tower_lsp::lsp_types::Url) -> tower_lsp::lsp_types::Location {
    tower_lsp::lsp_types::Location {
        uri: uri.clone(),
        range: span_to_range(&span, name_len),
    }
}

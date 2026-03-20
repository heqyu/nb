use tower_lsp::lsp_types::*;
use nb_core::lexer::Lexer;
use nb_core::parser::{Parser, ast::*};

// ── 符号信息 ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum SymbolInfo {
    Variable {
        name: String,
        mutable: bool,
        type_ann: Option<TypeAnnotation>,
    },
    Function {
        name: String,
        params: Vec<Param>,
        ret_type: Option<TypeAnnotation>,
        async_: bool,
        throws: bool,
    },
    Class {
        name: String,
        parents: Vec<String>,
        fields: Vec<FieldDef>,
        methods: Vec<FnDef>,
    },
    Trait {
        name: String,
        requires: Vec<FieldDef>,
        methods: Vec<FnDef>,
    },
    Parameter {
        name: String,
        mutable: bool,
        type_ann: Option<TypeAnnotation>,
    },
}

// ── 符号表条目（带定义位置）─────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct SymbolEntry {
    info: SymbolInfo,
    def_span: Span,
}

// ── 符号表构建 ────────────────────────────────────────────────────────────────

struct SymbolTable {
    entries: Vec<SymbolEntry>,
}

impl SymbolTable {
    fn build(stmts: &[Stmt]) -> Self {
        let mut entries = Vec::new();
        collect_stmts(stmts, &mut entries);
        Self { entries }
    }

    /// 根据 LSP 位置（0-based）查找最精确匹配的符号
    fn lookup(&self, line: u32, col: u32) -> Option<&SymbolInfo> {
        // 把 LSP 0-based 转为 AST 1-based
        let al = (line + 1) as usize;
        let ac = (col + 1) as usize;

        // 找同行内最近（列差最小）的条目
        self.entries
            .iter()
            .filter(|e| e.def_span.line == al)
            .filter(|e| e.def_span.col <= ac && ac <= e.def_span.col + symbol_name_len(&e.info))
            .next()
            .map(|e| &e.info)
    }
}

fn symbol_name_len(info: &SymbolInfo) -> usize {
    match info {
        SymbolInfo::Variable  { name, .. } => name.len(),
        SymbolInfo::Function  { name, .. } => name.len(),
        SymbolInfo::Class     { name, .. } => name.len(),
        SymbolInfo::Trait     { name, .. } => name.len(),
        SymbolInfo::Parameter { name, .. } => name.len(),
    }
}

fn collect_stmts(stmts: &[Stmt], out: &mut Vec<SymbolEntry>) {
    for stmt in stmts {
        collect_stmt(stmt, out);
    }
}

fn collect_stmt(stmt: &Stmt, out: &mut Vec<SymbolEntry>) {
    match stmt {
        Stmt::Let { name, mutable, type_ann, name_span, .. } => {
            out.push(SymbolEntry {
                info: SymbolInfo::Variable {
                    name: name.clone(),
                    mutable: *mutable,
                    type_ann: type_ann.clone(),
                },
                def_span: *name_span,
            });
        }
        Stmt::FnDef(f) => {
            collect_fndef(f, out);
        }
        Stmt::ClassDef(cd) => {
            let methods: Vec<FnDef> = cd.methods.iter().map(|m| m.fn_def.clone()).collect();
            out.push(SymbolEntry {
                info: SymbolInfo::Class {
                    name: cd.name.clone(),
                    parents: cd.parents.clone(),
                    fields: cd.fields.clone(),
                    methods: methods.clone(),
                },
                def_span: cd.name_span,
            });
            // 方法也单独注册，方便方法体内查找
            for m in &cd.methods {
                collect_fndef(&m.fn_def, out);
            }
        }
        Stmt::TraitDef(td) => {
            out.push(SymbolEntry {
                info: SymbolInfo::Trait {
                    name: td.name.clone(),
                    requires: td.requires.clone(),
                    methods: td.methods.clone(),
                },
                def_span: td.name_span,
            });
            for m in &td.methods {
                collect_fndef(m, out);
            }
        }
        Stmt::If { cond: _, then_body, else_ifs, else_body, .. } => {
            collect_stmts(then_body, out);
            for (_, b) in else_ifs { collect_stmts(b, out); }
            if let Some(b) = else_body { collect_stmts(b, out); }
        }
        Stmt::While { body, .. } => collect_stmts(body, out),
        Stmt::ForIn { body, .. } => collect_stmts(body, out),
        _ => {}
    }
}

fn collect_fndef(f: &FnDef, out: &mut Vec<SymbolEntry>) {
    if let Some(name) = &f.name {
        out.push(SymbolEntry {
            info: SymbolInfo::Function {
                name: name.clone(),
                params: f.params.clone(),
                ret_type: f.ret_type.clone(),
                async_: f.async_,
                throws: f.throws,
            },
            def_span: f.name_span,
        });
    }
    // 参数也注册
    for p in &f.params {
        if p.name != "self" {
            out.push(SymbolEntry {
                info: SymbolInfo::Parameter {
                    name: p.name.clone(),
                    mutable: p.mutable,
                    type_ann: p.type_ann.clone(),
                },
                def_span: p.name_span,
            });
        }
    }
    collect_stmts(&f.body, out);
}

// ── Hover 入口 ────────────────────────────────────────────────────────────────

pub fn get_hover(source: &str, position: Position) -> Option<Hover> {
    // 先从 token 流找光标下的标识符名字
    let cursor_name = ident_at_position(source, position)?;

    // 解析 AST 建符号表
    let tokens = Lexer::new(source).tokenize().ok()?;
    let stmts  = Parser::new(tokens).parse_program().ok()?;
    let table  = SymbolTable::build(&stmts);

    // 优先：在符号表定义处精确匹配
    if let Some(info) = table.lookup(position.line, position.character) {
        let md = render_hover(info);
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: md,
            }),
            range: None,
        });
    }

    // 回退：按名字在符号表中搜索（光标在使用处，非定义处）
    let info = table.entries.iter()
        .find(|e| symbol_name(e) == cursor_name)
        .map(|e| &e.info)?;

    let md = render_hover(info);
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: md,
        }),
        range: None,
    })
}

fn symbol_name(entry: &SymbolEntry) -> &str {
    match &entry.info {
        SymbolInfo::Variable  { name, .. } => name,
        SymbolInfo::Function  { name, .. } => name,
        SymbolInfo::Class     { name, .. } => name,
        SymbolInfo::Trait     { name, .. } => name,
        SymbolInfo::Parameter { name, .. } => name,
    }
}

// ── 位置 → 标识符名 ──────────────────────────────────────────────────────────

/// 从 token 流找光标（0-based line/col）下的 Ident token
fn ident_at_position(source: &str, pos: Position) -> Option<String> {
    let tokens = Lexer::new(source).tokenize().ok()?;
    let target_line = (pos.line + 1) as usize;
    let target_col  = (pos.character + 1) as usize;

    for twp in &tokens {
        if twp.line != target_line { continue; }
        if let nb_core::lexer::Token::Ident(name) = &twp.token {
            let start = twp.col;
            let end   = twp.col + name.len();
            if target_col >= start && target_col <= end {
                return Some(name.clone());
            }
        }
    }
    None
}

// ── Hover 内容渲染 ────────────────────────────────────────────────────────────

fn render_hover(info: &SymbolInfo) -> String {
    match info {
        SymbolInfo::Variable { name, mutable, type_ann } => {
            let mut_ = if *mutable { "mut " } else { "" };
            let ty = type_ann.as_ref()
                .map(|t| format!(": {}", type_ann_str(t)))
                .unwrap_or_default();
            format!("```nb\nlet {}{}{}\n```", mut_, name, ty)
        }
        SymbolInfo::Parameter { name, mutable, type_ann } => {
            let mut_ = if *mutable { "mut " } else { "" };
            let ty = type_ann.as_ref()
                .map(|t| format!(": {}", type_ann_str(t)))
                .unwrap_or_default();
            format!("```nb\nparam {}{}{}\n```", mut_, name, ty)
        }
        SymbolInfo::Function { name, params, ret_type, async_, throws } => {
            let async_kw = if *async_ { "async " } else { "" };
            let params_str: Vec<String> = params.iter()
                .filter(|p| p.name != "self")
                .map(|p| {
                    let mut_ = if p.mutable { "mut " } else { "" };
                    match &p.type_ann {
                        Some(t) => format!("{}{}: {}", mut_, p.name, type_ann_str(t)),
                        None    => format!("{}{}", mut_, p.name),
                    }
                })
                .collect();
            let ret = ret_type.as_ref()
                .map(|t| format!(": {}", type_ann_str(t)))
                .unwrap_or_default();
            let throws_kw = if *throws { " throws" } else { "" };
            format!("```nb\n{}fn {}({}){}{}\n```", async_kw, name, params_str.join(", "), ret, throws_kw)
        }
        SymbolInfo::Class { name, parents, fields, methods } => {
            let extends = if parents.is_empty() {
                String::new()
            } else {
                format!(" extends {}", parents.join(", "))
            };
            let mut lines = vec![format!("```nb\nclass{} {}{}", " ", name, extends)];
            for f in fields {
                let mut_ = if f.mutable { "mut " } else { "" };
                let ty = f.type_ann.as_ref()
                    .map(|t| format!(": {}", type_ann_str(t)))
                    .unwrap_or_default();
                lines.push(format!("  {}{}{}", mut_, f.name, ty));
            }
            for m in methods {
                if let Some(n) = &m.name {
                    lines.push(format!("  fn {}(...)", n));
                }
            }
            lines.push("```".to_string());
            lines.join("\n")
        }
        SymbolInfo::Trait { name, requires, methods } => {
            let mut lines = vec![format!("```nb\ntrait {}", name)];
            for r in requires {
                let ty = r.type_ann.as_ref()
                    .map(|t| format!(": {}", type_ann_str(t)))
                    .unwrap_or_default();
                lines.push(format!("  {}{}", r.name, ty));
            }
            for m in methods {
                if let Some(n) = &m.name {
                    lines.push(format!("  fn {}(...)", n));
                }
            }
            lines.push("```".to_string());
            lines.join("\n")
        }
    }
}

pub fn type_ann_str(t: &TypeAnnotation) -> String {
    match t {
        TypeAnnotation::Simple(s) => s.clone(),
        TypeAnnotation::Generic(name, args) => {
            let args: Vec<String> = args.iter().map(type_ann_str).collect();
            format!("{}<{}>", name, args.join(", "))
        }
        TypeAnnotation::Any => "any".to_string(),
    }
}

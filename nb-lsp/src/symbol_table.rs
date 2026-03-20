use nb_core::lexer::{Lexer, Token};
use nb_core::parser::{Parser, ast::*};
use tower_lsp::lsp_types::Position;

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
        mixins: Vec<String>,
        fields: Vec<FieldDef>,
        methods: Vec<FnDef>,
    },
    Mixin {
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

impl SymbolInfo {
    pub fn name(&self) -> &str {
        match self {
            SymbolInfo::Variable  { name, .. } => name,
            SymbolInfo::Function  { name, .. } => name,
            SymbolInfo::Class     { name, .. } => name,
            SymbolInfo::Mixin     { name, .. } => name,
            SymbolInfo::Parameter { name, .. } => name,
        }
    }
}

// ── 符号表条目（带定义位置）─────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SymbolEntry {
    pub info: SymbolInfo,
    pub def_span: Span,  // AST 1-based
}

// ── 符号表 ────────────────────────────────────────────────────────────────────

pub struct SymbolTable {
    pub entries: Vec<SymbolEntry>,
}

impl SymbolTable {
    pub fn build(stmts: &[Stmt]) -> Self {
        let mut entries = Vec::new();
        collect_stmts(stmts, &mut entries);
        Self { entries }
    }

    /// 按 LSP 位置（0-based）精确匹配定义处的符号
    pub fn lookup_at(&self, pos: Position) -> Option<&SymbolEntry> {
        let al = (pos.line + 1) as usize;
        let ac = (pos.character + 1) as usize;
        self.entries.iter().find(|e| {
            e.def_span.line == al
                && ac >= e.def_span.col
                && ac <= e.def_span.col + e.info.name().len()
        })
    }

    /// 按名字查找第一个匹配的符号（用于使用处回退查找）
    pub fn lookup_by_name(&self, name: &str) -> Option<&SymbolEntry> {
        self.entries.iter().find(|e| e.info.name() == name)
    }
}

// ── AST 遍历 ──────────────────────────────────────────────────────────────────

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
        Stmt::FnDef(f) => collect_fndef(f, out),
        Stmt::ClassDef(cd) => {
            let methods: Vec<FnDef> = cd.methods.iter().map(|m| m.fn_def.clone()).collect();
            out.push(SymbolEntry {
                info: SymbolInfo::Class {
                    name: cd.name.clone(),
                    mixins: cd.mixins.clone(),
                    fields: cd.fields.clone(),
                    methods,
                },
                def_span: cd.name_span,
            });
            for m in &cd.methods {
                collect_fndef(&m.fn_def, out);
            }
        }
        Stmt::MixinDef(md) => {
            out.push(SymbolEntry {
                info: SymbolInfo::Mixin {
                    name: md.name.clone(),
                    requires: md.requires.clone(),
                    methods: md.methods.clone(),
                },
                def_span: md.name_span,
            });
            for m in &md.methods {
                collect_fndef(m, out);
            }
        }
        Stmt::If { then_body, else_ifs, else_body, .. } => {
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

// ── 公共工具 ──────────────────────────────────────────────────────────────────

/// 从 token 流找光标（LSP 0-based）下的标识符名字
pub fn ident_at_position(source: &str, pos: Position) -> Option<String> {
    let tokens = Lexer::new(source).tokenize().ok()?;
    let target_line = (pos.line + 1) as usize;
    let target_col  = (pos.character + 1) as usize;
    for twp in &tokens {
        if twp.line != target_line { continue; }
        if let Token::Ident(name) = &twp.token {
            let end = twp.col + name.len();
            if target_col >= twp.col && target_col <= end {
                return Some(name.clone());
            }
        }
    }
    None
}

/// 解析源码，构建符号表；词法或语法错误时返回 None
pub fn build_table(source: &str) -> Option<SymbolTable> {
    let tokens = Lexer::new(source).tokenize().ok()?;
    let stmts  = Parser::new(tokens).parse_program().ok()?;
    Some(SymbolTable::build(&stmts))
}

/// span (AST 1-based) → LSP Range（0-based）
pub fn span_to_lsp_range(span: &Span, name_len: u32) -> tower_lsp::lsp_types::Range {
    let start = Position::new(
        (span.line as u32).saturating_sub(1),
        (span.col  as u32).saturating_sub(1),
    );
    tower_lsp::lsp_types::Range::new(start, Position::new(start.line, start.character + name_len))
}

/// TypeAnnotation → 可读字符串
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

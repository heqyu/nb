use std::collections::HashMap;
use tower_lsp::lsp_types::*;
use nb_core::lexer::Lexer;
use nb_core::parser::{Parser, ast::*};

use crate::symbol_table::type_ann_str;

/// NB 语言关键字补全列表（label, snippet）
const KEYWORDS: &[(&str, &str)] = &[
    ("let",      "let ${1:name} = $0"),
    ("mut",      "mut"),
    ("fn",       "fn ${1:name}($2) {\n\t$0\n}"),
    ("return",   "return $0"),
    ("if",       "if $1 {\n\t$0\n}"),
    ("else",     "else {\n\t$0\n}"),
    ("for",      "for ${1:item} in ${2:iter} {\n\t$0\n}"),
    ("while",    "while $1 {\n\t$0\n}"),
    ("in",       "in"),
    ("break",    "break"),
    ("continue", "continue"),
    ("class",    "class ${1:Name} {\n\t$0\n}"),
    ("mixin",    "mixin ${1:Name} {\n\t$0\n}"),
    ("new",      "new ${1:Class}($0)"),
    ("is",       "is"),
    ("self",     "self"),
    ("super",    "super"),
    ("static",   "static"),
    ("throw",    "throw $0"),
    ("protect",  "protect {\n\t$0\n}"),
    ("async",    "async"),
    ("await",    "await $0"),
    ("export",   "export"),
    ("require",  "require"),
    ("throws",   "throws"),
    ("nil",      "nil"),
    ("true",     "true"),
    ("false",    "false"),
];

// ── 解析辅助结构 ──────────────────────────────────────────────────────────────

struct ParsedDoc {
    /// 变量名 → 类名（通过 let x = new Foo(...) 推断）
    type_map: HashMap<String, String>,
    /// 类名 → ClassDef
    class_map: HashMap<String, ClassDef>,
    /// mixin名 → MixinDef
    mixin_map: HashMap<String, MixinDef>,
    /// 所有顶层符号名（用于普通补全）
    symbol_names: Vec<(String, SymbolKind, Option<String>)>,
}

impl ParsedDoc {
    fn from_source(source: &str) -> Option<Self> {
        let tokens = Lexer::new(source).tokenize().ok()?;
        let stmts  = Parser::new(tokens).parse_program().ok()?;

        let mut doc = ParsedDoc {
            type_map:     HashMap::new(),
            class_map:    HashMap::new(),
            mixin_map:    HashMap::new(),
            symbol_names: Vec::new(),
        };
        doc.collect_stmts(&stmts);
        Some(doc)
    }

    fn collect_stmts(&mut self, stmts: &[Stmt]) {
        for stmt in stmts {
            self.collect_stmt(stmt);
        }
    }

    fn collect_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let { name, value, type_ann, .. } => {
                // 推断类型：let x = new Foo(...)
                if let Some(Expr::New { class, .. }) = value {
                    self.type_map.insert(name.clone(), class.clone());
                }
                let detail = type_ann.as_ref().map(type_ann_str);
                self.symbol_names.push((name.clone(), SymbolKind::VARIABLE, detail));
            }
            Stmt::FnDef(f) => {
                if let Some(n) = &f.name {
                    self.symbol_names.push((n.clone(), SymbolKind::FUNCTION, None));
                }
            }
            Stmt::ClassDef(cd) => {
                self.class_map.insert(cd.name.clone(), cd.clone());
                self.symbol_names.push((cd.name.clone(), SymbolKind::CLASS, None));
            }
            Stmt::MixinDef(md) => {
                self.mixin_map.insert(md.name.clone(), md.clone());
                self.symbol_names.push((md.name.clone(), SymbolKind::INTERFACE, None));
            }
            Stmt::If { then_body, else_ifs, else_body, .. } => {
                self.collect_stmts(then_body);
                for (_, b) in else_ifs { self.collect_stmts(b); }
                if let Some(b) = else_body { self.collect_stmts(b); }
            }
            Stmt::While { body, .. } => self.collect_stmts(body),
            Stmt::ForIn { body, .. } => self.collect_stmts(body),
            _ => {}
        }
    }
}

// ── 主入口 ────────────────────────────────────────────────────────────────────

pub fn get_completions(source: &str, position: Position) -> Vec<CompletionItem> {
    let prefix = get_prefix(source, position);

    // 成员访问补全（x.___）
    if let Some(obj_name) = dot_object(source, position) {
        return member_completions(source, &obj_name, &prefix);
    }

    // 普通补全：关键字 + 当前文档符号
    let mut items: Vec<CompletionItem> = Vec::new();

    // 关键字
    for (kw, snippet) in KEYWORDS {
        if kw.starts_with(prefix.as_str()) {
            items.push(CompletionItem {
                label: kw.to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                insert_text: Some(snippet.to_string()),
                insert_text_format: Some(
                    if snippet.contains('$') { InsertTextFormat::SNIPPET }
                    else { InsertTextFormat::PLAIN_TEXT }
                ),
                ..Default::default()
            });
        }
    }

    // 文档符号
    if let Some(doc) = ParsedDoc::from_source(source) {
        for (name, kind, detail) in &doc.symbol_names {
            if name.starts_with(prefix.as_str()) && !items.iter().any(|i| &i.label == name) {
                items.push(symbol_item(name, *kind, detail.clone()));
            }
        }
    }

    items
}

// ── 成员补全 ──────────────────────────────────────────────────────────────────

fn member_completions(source: &str, obj_name: &str, prefix: &str) -> Vec<CompletionItem> {
    let Some(doc) = ParsedDoc::from_source(source) else { return vec![]; };

    // 推断对象类型
    let class_name = if obj_name == "self" {
        // self：从上下文找最近的 class，这里简单返回所有类成员
        // 先按 type_map，如果没有就返回空（self 的处理见下面特殊路径）
        doc.type_map.get(obj_name).cloned()
    } else {
        doc.type_map.get(obj_name).cloned()
    };

    let Some(class_name) = class_name else {
        // 类型未知，返回空（避免噪声）
        return vec![];
    };

    let Some(cd) = doc.class_map.get(&class_name) else {
        return vec![];
    };

    let mut items: Vec<CompletionItem> = Vec::new();

    // 类自身字段（不含 ctor）
    for field in &cd.fields {
        if field.name.starts_with(prefix) {
            items.push(CompletionItem {
                label: field.name.clone(),
                kind: Some(CompletionItemKind::FIELD),
                detail: field.type_ann.as_ref().map(type_ann_str),
                ..Default::default()
            });
        }
    }

    // 类自身方法（排除 ctor）
    for method in &cd.methods {
        if method.static_ { continue; }
        let Some(name) = &method.fn_def.name else { continue };
        if name == "ctor" { continue; }
        if name.starts_with(prefix) {
            items.push(fn_completion_item(name, &method.fn_def));
        }
    }

    // 所有 mixin 的方法
    for mixin_name in &cd.mixins {
        if let Some(md) = doc.mixin_map.get(mixin_name) {
            for f in &md.methods {
                let Some(name) = &f.name else { continue };
                if name == "ctor" { continue; }
                if name.starts_with(prefix) && !items.iter().any(|i| &i.label == name) {
                    items.push(fn_completion_item(name, f));
                }
            }
        }
    }

    items
}

// ── 工具函数 ──────────────────────────────────────────────────────────────────

/// 返回光标前正在输入的标识符前缀
fn get_prefix(source: &str, pos: Position) -> String {
    let line = source_line(source, pos.line as usize);
    let before = &line[..pos.character as usize];
    let start = before
        .rfind(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|i| i + 1)
        .unwrap_or(0);
    before[start..].to_string()
}

/// 如果光标在 `ident.` 之后，返回 ident；否则返回 None
fn dot_object(source: &str, pos: Position) -> Option<String> {
    let line = source_line(source, pos.line as usize);
    let char_pos = pos.character as usize;
    let before = &line[..char_pos.min(line.len())];
    // 去掉光标前正在输入的前缀（例如 "p.lev" 中的 "lev"）
    let stripped = before.trim_end_matches(|c: char| c.is_alphanumeric() || c == '_');
    if !stripped.ends_with('.') { return None; }
    // 取 '.' 之前的标识符
    let without_dot = &stripped[..stripped.len() - 1];
    let start = without_dot
        .rfind(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|i| i + 1)
        .unwrap_or(0);
    let ident = &without_dot[start..];
    if ident.is_empty() { None } else { Some(ident.to_string()) }
}

fn source_line(source: &str, line_idx: usize) -> String {
    source.lines().nth(line_idx).unwrap_or("").to_string()
}

fn symbol_item(name: &str, kind: SymbolKind, detail: Option<String>) -> CompletionItem {
    let lsp_kind = match kind {
        SymbolKind::FUNCTION  => CompletionItemKind::FUNCTION,
        SymbolKind::CLASS     => CompletionItemKind::CLASS,
        SymbolKind::INTERFACE => CompletionItemKind::INTERFACE,
        _                     => CompletionItemKind::VARIABLE,
    };
    CompletionItem {
        label: name.to_string(),
        kind: Some(lsp_kind),
        detail,
        ..Default::default()
    }
}

fn fn_completion_item(name: &str, f: &FnDef) -> CompletionItem {
    let params_str = f.params.iter()
        .filter(|p| p.name != "self")
        .enumerate()
        .map(|(i, p)| format!("${{{}:{}}}", i + 1, p.name))
        .collect::<Vec<_>>()
        .join(", ");
    CompletionItem {
        label: name.to_string(),
        kind: Some(CompletionItemKind::METHOD),
        detail: f.ret_type.as_ref().map(type_ann_str),
        insert_text: Some(format!("{}({})", name, params_str)),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..Default::default()
    }
}

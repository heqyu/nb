use tower_lsp::lsp_types::*;
use nb_core::parser::ast::*;

use crate::symbol_table::type_ann_str;
use crate::resolution::{AnalyzedDoc, CompletionIndex};

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
    ("is",       "is"),
    ("self",     "self"),
    ("super",    "super"),
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

// ── 主入口 ────────────────────────────────────────────────────────────────────

pub fn get_completions(doc: &AnalyzedDoc, position: Position) -> Vec<CompletionItem> {
    let source = &doc.source;
    let prefix = get_prefix(source, position);

    // 成员访问补全（x.___）
    if let Some(obj_name) = dot_object(source, position) {
        return member_completions(&doc.completion, &obj_name, &prefix);
    }

    // 普通补全：关键字 + 当前文档符号
    let mut items: Vec<CompletionItem> = Vec::new();

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

    for (name, kind, detail) in &doc.completion.symbol_names {
        if name.starts_with(prefix.as_str()) && !items.iter().any(|i| &i.label == name) {
            items.push(symbol_item(name, *kind, detail.clone()));
        }
    }

    items
}

// ── 成员补全 ──────────────────────────────────────────────────────────────────

fn member_completions(idx: &CompletionIndex, obj_name: &str, prefix: &str) -> Vec<CompletionItem> {
    let class_name = idx.type_map.get(obj_name).cloned();
    let Some(class_name) = class_name else { return vec![]; };
    let Some((cd, receiver_methods)) = idx.class_map.get(&class_name) else { return vec![]; };

    let mut items: Vec<CompletionItem> = Vec::new();

    // 1. 类自身字段
    for field in &cd.fields {
        if field.name.starts_with(prefix) {
            items.push(CompletionItem {
                label: field.name.clone(),
                kind: Some(CompletionItemKind::FIELD),
                detail: field.type_ann.as_ref().map(type_ann_str),
                sort_text: Some(format!("1_{}", field.name)),
                ..Default::default()
            });
        }
    }

    // 2. 类自身方法
    for f in receiver_methods {
        let Some(name) = &f.name else { continue };
        if name.starts_with(prefix) {
            let mut item = fn_completion_item(name, f);
            item.sort_text = Some(format!("2_{}", name));
            items.push(item);
        }
    }

    // 3. mixin 方法
    for mixin_name in &cd.mixins {
        if let Some(md) = idx.mixin_map.get(mixin_name) {
            for f in &md.methods {
                let Some(name) = &f.name else { continue };
                if name.starts_with(prefix) && !items.iter().any(|i| &i.label == name) {
                    let mut item = fn_completion_item(name, f);
                    item.sort_text = Some(format!("3_{}_{}", mixin_name, name));
                    let sig = item.detail.map(|d| format!("{d}  •  {mixin_name}"))
                        .unwrap_or_else(|| format!("• {mixin_name}"));
                    item.detail = Some(sig);
                    items.push(item);
                }
            }
        }
    }

    items
}

// ── 工具函数 ──────────────────────────────────────────────────────────────────

fn get_prefix(source: &str, pos: Position) -> String {
    let line = source_line(source, pos.line as usize);
    let before = &line[..pos.character as usize];
    let start = before
        .rfind(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|i| i + 1)
        .unwrap_or(0);
    before[start..].to_string()
}

fn dot_object(source: &str, pos: Position) -> Option<String> {
    let line = source_line(source, pos.line as usize);
    let char_pos = pos.character as usize;
    let before = &line[..char_pos.min(line.len())];
    let stripped = before.trim_end_matches(|c: char| c.is_alphanumeric() || c == '_');
    if !stripped.ends_with('.') { return None; }
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
    CompletionItem { label: name.to_string(), kind: Some(lsp_kind), detail, ..Default::default() }
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

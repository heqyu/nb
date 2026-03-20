use tower_lsp::lsp_types::*;

use crate::symbol_table::{build_table, SymbolInfo, type_ann_str};

/// NB 语言关键字列表
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

pub fn get_completions(source: &str, position: Position) -> Vec<CompletionItem> {
    let mut items: Vec<CompletionItem> = Vec::new();

    // 获取光标前的前缀词
    let prefix = get_prefix(source, position);

    // 判断是否是成员访问（.之后）
    if is_after_dot(source, position) {
        // 成员补全：从符号表找所有方法和字段
        if let Some(table) = build_table(source) {
            for entry in &table.entries {
                match &entry.info {
                    SymbolInfo::Class { fields, methods, .. } => {
                        for f in fields {
                            if f.name.starts_with(&prefix) {
                                items.push(field_item(&f.name, f.type_ann.as_ref().map(type_ann_str)));
                            }
                        }
                        for m in methods {
                            if let Some(n) = &m.name {
                                if n.starts_with(&prefix) {
                                    items.push(method_item(n, m));
                                }
                            }
                        }
                    }
                    SymbolInfo::Mixin { methods, .. } => {
                        for m in methods {
                            if let Some(n) = &m.name {
                                if n.starts_with(&prefix) {
                                    items.push(method_item(n, m));
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        return items;
    }

    // 关键字补全
    for (kw, snippet) in KEYWORDS {
        if kw.starts_with(&prefix) {
            let mut item = CompletionItem {
                label: kw.to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                insert_text: Some(snippet.to_string()),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                ..Default::default()
            };
            // 纯关键字（无占位符）就用 PlainText
            if !snippet.contains('$') {
                item.insert_text_format = Some(InsertTextFormat::PLAIN_TEXT);
            }
            items.push(item);
        }
    }

    // 符号补全（当前文档中定义的名字）
    if let Some(table) = build_table(source) {
        for entry in &table.entries {
            let name = entry.info.name();
            if name.starts_with(&prefix) {
                let item = symbol_completion_item(&entry.info);
                // 避免与关键字重复
                if !items.iter().any(|i| i.label == item.label) {
                    items.push(item);
                }
            }
        }
    }

    items
}

// ── 辅助：判断光标前是否紧跟 '.' ────────────────────────────────────────────

fn is_after_dot(source: &str, pos: Position) -> bool {
    let lines: Vec<&str> = source.lines().collect();
    let line_idx = pos.line as usize;
    if line_idx >= lines.len() { return false; }
    let line = lines[line_idx];
    let char_pos = pos.character as usize;

    // 找光标前跳过当前词后面的字符
    let before = &line[..char_pos.min(line.len())];
    // 去掉当前正在输入的标识符前缀
    let trimmed = before.trim_end_matches(|c: char| c.is_alphanumeric() || c == '_');
    trimmed.ends_with('.')
}

// ── 辅助：获取光标前正在输入的词前缀 ────────────────────────────────────────

fn get_prefix(source: &str, pos: Position) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let line_idx = pos.line as usize;
    if line_idx >= lines.len() { return String::new(); }
    let line = lines[line_idx];
    let char_pos = pos.character as usize;
    let before = &line[..char_pos.min(line.len())];
    // 从末尾取连续的标识符字符
    let start = before
        .rfind(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|i| i + 1)
        .unwrap_or(0);
    before[start..].to_string()
}

// ── 辅助：构建不同类型的 CompletionItem ──────────────────────────────────────

fn symbol_completion_item(info: &SymbolInfo) -> CompletionItem {
    match info {
        SymbolInfo::Variable { name, type_ann, .. } => CompletionItem {
            label: name.clone(),
            kind: Some(CompletionItemKind::VARIABLE),
            detail: type_ann.as_ref().map(type_ann_str),
            ..Default::default()
        },
        SymbolInfo::Parameter { name, type_ann, .. } => CompletionItem {
            label: name.clone(),
            kind: Some(CompletionItemKind::VARIABLE),
            detail: type_ann.as_ref().map(type_ann_str),
            ..Default::default()
        },
        SymbolInfo::Function { name, params, ret_type, .. } => {
            let params_str = params.iter()
                .filter(|p| p.name != "self")
                .enumerate()
                .map(|(i, p)| {
                    let t = p.type_ann.as_ref().map(type_ann_str).unwrap_or_default();
                    if t.is_empty() {
                        format!("${{{}:{}}}", i + 1, p.name)
                    } else {
                        format!("${{{}:{}}}", i + 1, p.name)
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            let detail = ret_type.as_ref().map(type_ann_str);
            CompletionItem {
                label: name.clone(),
                kind: Some(CompletionItemKind::FUNCTION),
                detail,
                insert_text: Some(format!("{}({})", name, params_str)),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                ..Default::default()
            }
        }
        SymbolInfo::Class { name, .. } => CompletionItem {
            label: name.clone(),
            kind: Some(CompletionItemKind::CLASS),
            insert_text: Some(format!("{}($0)", name)),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        },
        SymbolInfo::Mixin { name, .. } => CompletionItem {
            label: name.clone(),
            kind: Some(CompletionItemKind::INTERFACE),
            ..Default::default()
        },
    }
}

fn field_item(name: &str, type_str: Option<String>) -> CompletionItem {
    CompletionItem {
        label: name.to_string(),
        kind: Some(CompletionItemKind::FIELD),
        detail: type_str,
        ..Default::default()
    }
}

fn method_item(name: &str, f: &nb_core::parser::ast::FnDef) -> CompletionItem {
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

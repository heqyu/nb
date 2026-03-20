use tower_lsp::lsp_types::*;
use nb_core::parser::ast::*;
use crate::symbol_table::{span_to_lsp_range, type_ann_str};
use crate::resolution::AnalyzedDoc;

fn span_to_range(span: &Span, length: u32) -> Range {
    span_to_lsp_range(span, length)
}

/// 对给定 AnalyzedDoc 生成文档大纲（Document Symbols）
pub fn get_document_symbols(doc: &AnalyzedDoc) -> Vec<DocumentSymbol> {
    collect_symbols(&doc.stmts)
}

fn collect_symbols(stmts: &[Stmt]) -> Vec<DocumentSymbol> {
    let mut out = Vec::new();
    for stmt in stmts {
        match stmt {
            Stmt::FnDef(f) => {
                if let Some(sym) = fndef_symbol(f) {
                    out.push(sym);
                }
            }
            Stmt::ClassDef(cd) => out.push(classdef_symbol(cd)),
            Stmt::MixinDef(td) => out.push(mixindef_symbol(td)),
            Stmt::Let { name, name_span, .. } => {
                let range = span_to_range(name_span, name.len() as u32);
                out.push(make_symbol(name.clone(), None, SymbolKind::VARIABLE, range, None));
            }
            _ => {}
        }
    }
    out
}

fn fndef_symbol(f: &FnDef) -> Option<DocumentSymbol> {
    let name = f.name.as_ref()?;
    let range = span_to_range(&f.name_span, name.len() as u32);
    // receiver 函数显示为 "Player.method"
    let display_name = if let Some(r) = &f.receiver {
        format!("{}.{}", r, name)
    } else {
        name.clone()
    };
    Some(make_symbol(
        display_name,
        Some(fn_signature(f)),
        SymbolKind::FUNCTION,
        range,
        None,
    ))
}

fn classdef_symbol(cd: &ClassDef) -> DocumentSymbol {
    let range = span_to_range(&cd.name_span, cd.name.len() as u32);

    let mut children = Vec::new();

    // 字段
    for field in &cd.fields {
        let r = span_to_range(&field.name_span, field.name.len() as u32);
        children.push(make_symbol(
            field.name.clone(),
            field.type_ann.as_ref().map(type_ann_str),
            SymbolKind::FIELD,
            r,
            None,
        ));
    }

    let detail = if cd.mixins.is_empty() {
        None
    } else {
        Some(cd.mixins.join(", "))
    };
    make_symbol(cd.name.clone(), detail, SymbolKind::CLASS, range, Some(children))
}

fn mixindef_symbol(td: &MixinDef) -> DocumentSymbol {
    let range = span_to_range(&td.name_span, td.name.len() as u32);

    let mut children = Vec::new();

    // 必需字段
    for req in &td.requires {
        let r = span_to_range(&req.name_span, req.name.len() as u32);
        children.push(make_symbol(
            req.name.clone(),
            req.type_ann.as_ref().map(type_ann_str),
            SymbolKind::FIELD,
            r,
            None,
        ));
    }
    // 方法
    for m in &td.methods {
        if let Some(sym) = fndef_symbol(m) {
            children.push(sym);
        }
    }

    make_symbol(td.name.clone(), None, SymbolKind::INTERFACE, range, Some(children))
}

// ---------- 辅助函数 ----------

#[allow(deprecated)]
fn make_symbol(
    name: String,
    detail: Option<String>,
    kind: SymbolKind,
    range: Range,
    children: Option<Vec<DocumentSymbol>>,
) -> DocumentSymbol {
    DocumentSymbol {
        name,
        detail,
        kind,
        tags: None,
        deprecated: None,
        range,
        selection_range: range,
        children,
    }
}

fn fn_signature(f: &FnDef) -> String {
    let params: Vec<String> = f.params.iter()
        .filter(|p| p.name != "self")
        .map(|p| match &p.type_ann {
            Some(t) => format!("{}: {}", p.name, type_ann_str(t)),
            None    => p.name.clone(),
        })
        .collect();
    let ret = f.ret_type.as_ref()
        .map(|t| format!(": {}", type_ann_str(t)))
        .unwrap_or_default();
    format!("({}){}", params.join(", "), ret)
}

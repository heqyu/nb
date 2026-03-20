use tower_lsp::lsp_types::*;

use crate::resolution::{build_resolution_db, span_at_position_with_db, name_len_at};
use crate::symbol_table::{type_ann_str, SymbolInfo};

pub fn get_hover(source: &str, position: Position) -> Option<Hover> {
    let db   = build_resolution_db(source)?;
    let span = span_at_position_with_db(&db, source, position)?;

    let def_span = db.use_to_def.get(&span).copied().unwrap_or(span);
    let info = db.def_info.get(&def_span)?;

    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: render_hover(info),
        }),
        range: None,
    })
}

fn render_hover(info: &SymbolInfo) -> String {
    match info {
        SymbolInfo::Field { name, class_name, mutable, type_ann } => {
            let mut_ = if *mutable { "mut " } else { "" };
            let ty = opt_type(type_ann.as_ref());
            format!("```nb\n// {}.{}\n{}{}{};\n```", class_name, name, mut_, name, ty)
        }
        SymbolInfo::Variable { name, mutable, type_ann } => {
            let mut_ = if *mutable { "mut " } else { "" };
            let ty = opt_type(type_ann.as_ref());
            format!("```nb\nlet {}{}{};\n```", mut_, name, ty)
        }
        SymbolInfo::Parameter { name, mutable, type_ann } => {
            let mut_ = if *mutable { "mut " } else { "" };
            let ty = opt_type(type_ann.as_ref());
            format!("```nb\nparam {}{}{};\n```", mut_, name, ty)
        }
        SymbolInfo::Function { name, params, ret_type, async_, throws, receiver } => {
            let async_kw  = if *async_ { "async " } else { "" };
            let throws_kw = if *throws { " throws" } else { "" };
            let params_str = params.iter()
                .filter(|p| p.name != "self")
                .map(|p| {
                    let mut_ = if p.mutable { "mut " } else { "" };
                    format!("{}{}{}", mut_, p.name, opt_type(p.type_ann.as_ref()))
                })
                .collect::<Vec<_>>()
                .join(", ");
            let ret = opt_type(ret_type.as_ref());
            let prefix = receiver.as_ref().map(|r| format!("{}.", r)).unwrap_or_default();
            format!("```nb\n{}fn {}{}({}){}{};\n```", async_kw, prefix, name, params_str, ret, throws_kw)
        }
        SymbolInfo::Class { name, mixins, fields } => {
            let extends = if mixins.is_empty() {
                String::new()
            } else {
                format!(": {}", mixins.join(", "))
            };
            let mut lines = vec![format!("```nb\nclass {}{}", name, extends)];
            for f in fields {
                let mut_ = if f.mutable { "mut " } else { "" };
                lines.push(format!("  {}{}{}", mut_, f.name, opt_type(f.type_ann.as_ref())));
            }
            lines.push("```".into());
            lines.join("\n")
        }
        SymbolInfo::Mixin { name, requires, methods } => {
            let mut lines = vec![format!("```nb\nmixin {}", name)];
            for r in requires {
                lines.push(format!("  {}{}", r.name, opt_type(r.type_ann.as_ref())));
            }
            for m in methods {
                if let Some(n) = &m.name {
                    lines.push(format!("  fn {}(...)", n));
                }
            }
            lines.push("```".into());
            lines.join("\n")
        }
    }
}

fn opt_type(t: Option<&nb_core::parser::ast::TypeAnnotation>) -> String {
    t.map(|t| format!(": {}", type_ann_str(t))).unwrap_or_default()
}

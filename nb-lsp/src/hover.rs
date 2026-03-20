use tower_lsp::lsp_types::*;
use nb_core::parser::ast::*;

use crate::symbol_table::{
    build_table, ident_at_position, type_ann_str, SymbolInfo,
};

pub fn get_hover(source: &str, position: Position) -> Option<Hover> {
    let cursor_name = ident_at_position(source, position)?;
    let table = build_table(source)?;

    // 优先：光标在定义处；回退：光标在使用处，按名字查找
    let entry = table.lookup_at(position)
        .or_else(|| table.lookup_by_name(&cursor_name))?;

    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: render_hover(&entry.info),
        }),
        range: None,
    })
}

// ── 渲染 ──────────────────────────────────────────────────────────────────────

fn render_hover(info: &SymbolInfo) -> String {
    match info {
        SymbolInfo::Variable { name, mutable, type_ann } => {
            let mut_ = if *mutable { "mut " } else { "" };
            let ty = opt_type(type_ann.as_ref());
            format!("```nb\nlet {}{}{}\n```", mut_, name, ty)
        }
        SymbolInfo::Parameter { name, mutable, type_ann } => {
            let mut_ = if *mutable { "mut " } else { "" };
            let ty = opt_type(type_ann.as_ref());
            format!("```nb\nparam {}{}{}\n```", mut_, name, ty)
        }
        SymbolInfo::Function { name, params, ret_type, async_, throws } => {
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
            let ret = opt_type_colon(ret_type.as_ref());
            format!("```nb\n{}fn {}({}){}{}\n```", async_kw, name, params_str, ret, throws_kw)
        }
        SymbolInfo::Class { name, mixins, fields, methods } => {
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
            for m in methods {
                if let Some(n) = &m.name {
                    lines.push(format!("  fn {}(...)", n));
                }
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

fn opt_type(t: Option<&TypeAnnotation>) -> String {
    t.map(|t| format!(": {}", type_ann_str(t))).unwrap_or_default()
}

fn opt_type_colon(t: Option<&TypeAnnotation>) -> String {
    opt_type(t)
}

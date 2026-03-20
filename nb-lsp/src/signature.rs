use tower_lsp::lsp_types::*;
use nb_core::lexer::Lexer;
use nb_core::parser::{Parser, ast::*};

use crate::symbol_table::type_ann_str;

pub fn get_signature_help(source: &str, position: Position) -> Option<SignatureHelp> {
    // 找到光标所在调用表达式，提取函数名和当前参数索引
    let (fn_name, arg_index) = find_active_call(source, position)?;

    // 在 AST 中找到该函数的定义
    let tokens = Lexer::new(source).tokenize().ok()?;
    let stmts  = Parser::new(tokens).parse_program().ok()?;
    let fndef  = find_fndef_by_name(&stmts, &fn_name)?;

    // 过滤掉 self 参数
    let params: Vec<&Param> = fndef.params.iter()
        .filter(|p| p.name != "self")
        .collect();

    let label = build_signature_label(&fn_name, &params, fndef.ret_type.as_ref());

    let parameters: Vec<ParameterInformation> = params.iter().map(|p| {
        let param_label = format!("{}{}", p.name,
            p.type_ann.as_ref().map(|t| format!(": {}", type_ann_str(t))).unwrap_or_default());
        ParameterInformation {
            label: ParameterLabel::Simple(param_label),
            documentation: None,
        }
    }).collect();

    let active_parameter = if arg_index < params.len() as u32 {
        Some(arg_index)
    } else {
        Some(params.len().saturating_sub(1) as u32)
    };

    Some(SignatureHelp {
        signatures: vec![SignatureInformation {
            label,
            documentation: None,
            parameters: Some(parameters),
            active_parameter,
        }],
        active_signature: Some(0),
        active_parameter,
    })
}

// ── 解析光标位置找到当前调用 ──────────────────────────────────────────────────

/// 返回 (函数名, 当前参数序号)
fn find_active_call(source: &str, pos: Position) -> Option<(String, u32)> {
    let lines: Vec<&str> = source.lines().collect();
    let line_idx = pos.line as usize;
    if line_idx >= lines.len() { return None; }

    // 把光标之前的所有文本拼成一段，逆向扫描找最近未闭合的 '('
    let before_cursor: String = lines[..line_idx].join("\n")
        + "\n"
        + &lines[line_idx][..pos.character as usize];

    let chars: Vec<char> = before_cursor.chars().collect();
    let mut depth = 0i32;
    let mut arg_index = 0u32;
    let mut paren_pos = None;

    for i in (0..chars.len()).rev() {
        match chars[i] {
            ')' | ']' => depth += 1,
            '(' => {
                if depth == 0 {
                    paren_pos = Some(i);
                    break;
                }
                depth -= 1;
            }
            '[' => {
                if depth == 0 { return None; } // 下标访问，不是函数调用
                depth -= 1;
            }
            ',' if depth == 0 => arg_index += 1,
            _ => {}
        }
    }

    let paren_pos = paren_pos?;
    // 取 '(' 之前的标识符
    let before_paren: String = chars[..paren_pos].iter().collect();
    let trimmed = before_paren.trim_end();
    let start = trimmed.rfind(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|i| i + 1)
        .unwrap_or(0);
    let fn_name = trimmed[start..].to_string();
    if fn_name.is_empty() { return None; }

    Some((fn_name, arg_index))
}

// ── AST 查找函数定义 ──────────────────────────────────────────────────────────

fn find_fndef_by_name<'a>(stmts: &'a [Stmt], name: &str) -> Option<&'a FnDef> {
    for stmt in stmts {
        if let Some(f) = search_stmt(stmt, name) {
            return Some(f);
        }
    }
    None
}

fn search_stmt<'a>(stmt: &'a Stmt, name: &str) -> Option<&'a FnDef> {
    match stmt {
        Stmt::FnDef(f) if f.name.as_deref() == Some(name) => Some(f),
        Stmt::ClassDef(cd) => {
            for m in &cd.methods {
                if m.fn_def.name.as_deref() == Some(name) {
                    return Some(&m.fn_def);
                }
            }
            None
        }
        Stmt::MixinDef(md) => {
            for f in &md.methods {
                if f.name.as_deref() == Some(name) {
                    return Some(f);
                }
            }
            None
        }
        Stmt::If { then_body, else_ifs, else_body, .. } => {
            for s in then_body { if let Some(f) = search_stmt(s, name) { return Some(f); } }
            for (_, b) in else_ifs { for s in b { if let Some(f) = search_stmt(s, name) { return Some(f); } } }
            if let Some(b) = else_body { for s in b { if let Some(f) = search_stmt(s, name) { return Some(f); } } }
            None
        }
        Stmt::While { body, .. } | Stmt::ForIn { body, .. } => {
            for s in body { if let Some(f) = search_stmt(s, name) { return Some(f); } }
            None
        }
        _ => None,
    }
}

fn build_signature_label(name: &str, params: &[&Param], ret: Option<&TypeAnnotation>) -> String {
    let params_str = params.iter().map(|p| {
        format!("{}{}", p.name,
            p.type_ann.as_ref().map(|t| format!(": {}", type_ann_str(t))).unwrap_or_default())
    }).collect::<Vec<_>>().join(", ");
    let ret_str = ret.map(|t| format!(": {}", type_ann_str(t))).unwrap_or_default();
    format!("{}({}){}", name, params_str, ret_str)
}

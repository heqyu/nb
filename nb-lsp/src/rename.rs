use std::collections::HashMap;
use tower_lsp::lsp_types::*;
use nb_core::lexer::Lexer;
use nb_core::parser::{Parser, ast::*};

use crate::symbol_table::{ident_at_position, span_to_lsp_range};

pub fn get_rename(
    source: &str,
    uri: &Url,
    position: Position,
    new_name: &str,
) -> Option<WorkspaceEdit> {
    let old_name = ident_at_position(source, position)?;

    let tokens = Lexer::new(source).tokenize().ok()?;
    let stmts  = Parser::new(tokens).parse_program().ok()?;

    let mut spans: Vec<Span> = Vec::new();
    collect_all_refs_stmts(&stmts, &old_name, &mut spans);
    if spans.is_empty() { return None; }

    let edits: Vec<TextEdit> = spans.iter().map(|span| TextEdit {
        range: span_to_lsp_range(span, old_name.len() as u32),
        new_text: new_name.to_string(),
    }).collect();

    let mut changes = HashMap::new();
    changes.insert(uri.clone(), edits);

    Some(WorkspaceEdit {
        changes: Some(changes),
        ..Default::default()
    })
}

// ── 复用 references 模块的遍历逻辑 ───────────────────────────────────────────
// （为避免模块间循环依赖，直接内联遍历函数）

fn collect_all_refs_stmts(stmts: &[Stmt], name: &str, out: &mut Vec<Span>) {
    for stmt in stmts { collect_all_refs_stmt(stmt, name, out); }
}

fn collect_all_refs_stmt(stmt: &Stmt, name: &str, out: &mut Vec<Span>) {
    match stmt {
        Stmt::Let { name: n, name_span, value, .. } => {
            if n == name { out.push(*name_span); }
            if let Some(e) = value { collect_all_refs_expr(e, name, out); }
        }
        Stmt::Assign { target, value } | Stmt::CompoundAssign { target, value, .. } => {
            collect_all_refs_expr(target, name, out);
            collect_all_refs_expr(value, name, out);
        }
        Stmt::IncDec { target, .. } => collect_all_refs_expr(target, name, out),
        Stmt::FnDef(f) => {
            if f.name.as_deref() == Some(name) { out.push(f.name_span); }
            collect_all_refs_fndef(f, name, out);
        }
        Stmt::ClassDef(cd) => {
            if cd.name == name { out.push(cd.name_span); }
            for m in &cd.methods {
                if m.fn_def.name.as_deref() == Some(name) { out.push(m.fn_def.name_span); }
                collect_all_refs_fndef(&m.fn_def, name, out);
            }
        }
        Stmt::MixinDef(md) => {
            if md.name == name { out.push(md.name_span); }
            for f in &md.methods {
                if f.name.as_deref() == Some(name) { out.push(f.name_span); }
                collect_all_refs_fndef(f, name, out);
            }
        }
        Stmt::Return(Some(e)) => collect_all_refs_expr(e, name, out),
        Stmt::Throw(e)        => collect_all_refs_expr(e, name, out),
        Stmt::Expr(e)         => collect_all_refs_expr(e, name, out),
        Stmt::If { cond, then_body, else_ifs, else_body, .. } => {
            collect_all_refs_expr(cond, name, out);
            collect_all_refs_stmts(then_body, name, out);
            for (c, b) in else_ifs {
                collect_all_refs_expr(c, name, out);
                collect_all_refs_stmts(b, name, out);
            }
            if let Some(b) = else_body { collect_all_refs_stmts(b, name, out); }
        }
        Stmt::While { cond, body, .. } => {
            collect_all_refs_expr(cond, name, out);
            collect_all_refs_stmts(body, name, out);
        }
        Stmt::ForIn { iter, body, .. } => {
            collect_all_refs_expr(iter, name, out);
            collect_all_refs_stmts(body, name, out);
        }
        _ => {}
    }
}

fn collect_all_refs_fndef(f: &FnDef, name: &str, out: &mut Vec<Span>) {
    for p in &f.params {
        if p.name == name { out.push(p.name_span); }
    }
    collect_all_refs_stmts(&f.body, name, out);
}

fn collect_all_refs_expr(expr: &Expr, name: &str, out: &mut Vec<Span>) {
    match expr {
        Expr::Ident(n, span) => {
            if n == name { out.push(*span); }
        }
        Expr::Call { callee, args, .. } => {
            collect_all_refs_expr(callee, name, out);
            for arg in args { collect_all_refs_expr(&arg.expr, name, out); }
        }
        Expr::Field { obj, .. }       => collect_all_refs_expr(obj, name, out),
        Expr::Index { obj, idx }      => {
            collect_all_refs_expr(obj, name, out);
            collect_all_refs_expr(idx, name, out);
        }
        Expr::New { class, class_span, args, .. } => {
            if class == name { out.push(*class_span); }
            for arg in args { collect_all_refs_expr(&arg.expr, name, out); }
        }
        Expr::BinOp { left, right, .. } => {
            collect_all_refs_expr(left, name, out);
            collect_all_refs_expr(right, name, out);
        }
        Expr::UnaryOp { expr, .. }    => collect_all_refs_expr(expr, name, out),
        Expr::Ternary { cond, then, else_ } => {
            collect_all_refs_expr(cond, name, out);
            collect_all_refs_expr(then, name, out);
            collect_all_refs_expr(else_, name, out);
        }
        Expr::Is { expr, .. }         => collect_all_refs_expr(expr, name, out),
        Expr::Fn(f)                   => collect_all_refs_fndef(f, name, out),
        Expr::Protect(stmts)          => collect_all_refs_stmts(stmts, name, out),
        Expr::Array(elems)            => { for e in elems { collect_all_refs_expr(e, name, out); } }
        Expr::Dict(pairs)             => {
            for (k, v) in pairs {
                collect_all_refs_expr(k, name, out);
                collect_all_refs_expr(v, name, out);
            }
        }
        Expr::Await(e) | Expr::Try(e) => collect_all_refs_expr(e, name, out),
        _ => {}
    }
}

use tower_lsp::lsp_types::*;
use nb_core::lexer::Lexer;
use nb_core::parser::{Parser, ast::*};

use crate::symbol_table::{ident_at_position, span_to_lsp_range};

pub fn get_references(source: &str, uri: &Url, position: Position) -> Vec<Location> {
    let cursor_name = match ident_at_position(source, position) {
        Some(n) => n,
        None => return vec![],
    };
    let tokens = match Lexer::new(source).tokenize() {
        Ok(t) => t,
        Err(_) => return vec![],
    };
    let stmts = match Parser::new(tokens).parse_program() {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let mut spans: Vec<Span> = Vec::new();
    collect_refs_stmts(&stmts, &cursor_name, &mut spans);

    spans
        .into_iter()
        .map(|span| Location {
            uri: uri.clone(),
            range: span_to_lsp_range(&span, cursor_name.len() as u32),
        })
        .collect()
}

// ── AST 遍历收集所有出现位置 ──────────────────────────────────────────────────

fn collect_refs_stmts(stmts: &[Stmt], name: &str, out: &mut Vec<Span>) {
    for stmt in stmts {
        collect_refs_stmt(stmt, name, out);
    }
}

fn collect_refs_stmt(stmt: &Stmt, name: &str, out: &mut Vec<Span>) {
    match stmt {
        Stmt::Let { name: n, name_span, value, .. } => {
            if n == name { out.push(*name_span); }
            if let Some(e) = value { collect_refs_expr(e, name, out); }
        }
        Stmt::Assign { target, value } | Stmt::CompoundAssign { target, value, .. } => {
            collect_refs_expr(target, name, out);
            collect_refs_expr(value, name, out);
        }
        Stmt::IncDec { target, .. } => collect_refs_expr(target, name, out),
        Stmt::FnDef(f) => {
            // 函数名定义处
            if f.name.as_deref() == Some(name) { out.push(f.name_span); }
            collect_refs_fndef(f, name, out);
        }
        Stmt::ClassDef(cd) => {
            if cd.name == name { out.push(cd.name_span); }
            for m in &cd.methods {
                if m.fn_def.name.as_deref() == Some(name) { out.push(m.fn_def.name_span); }
                collect_refs_fndef(&m.fn_def, name, out);
            }
        }
        Stmt::MixinDef(md) => {
            if md.name == name { out.push(md.name_span); }
            for f in &md.methods {
                if f.name.as_deref() == Some(name) { out.push(f.name_span); }
                collect_refs_fndef(f, name, out);
            }
        }
        Stmt::Return(Some(e)) => collect_refs_expr(e, name, out),
        Stmt::Throw(e) => collect_refs_expr(e, name, out),
        Stmt::Expr(e) => collect_refs_expr(e, name, out),
        Stmt::If { cond, then_body, else_ifs, else_body, .. } => {
            collect_refs_expr(cond, name, out);
            collect_refs_stmts(then_body, name, out);
            for (c, b) in else_ifs {
                collect_refs_expr(c, name, out);
                collect_refs_stmts(b, name, out);
            }
            if let Some(b) = else_body { collect_refs_stmts(b, name, out); }
        }
        Stmt::While { cond, body, .. } => {
            collect_refs_expr(cond, name, out);
            collect_refs_stmts(body, name, out);
        }
        Stmt::ForIn { iter, body, .. } => {
            collect_refs_expr(iter, name, out);
            collect_refs_stmts(body, name, out);
        }
        _ => {}
    }
}

fn collect_refs_fndef(f: &FnDef, name: &str, out: &mut Vec<Span>) {
    for p in &f.params {
        if p.name == name { out.push(p.name_span); }
    }
    collect_refs_stmts(&f.body, name, out);
}

fn collect_refs_expr(expr: &Expr, name: &str, out: &mut Vec<Span>) {
    match expr {
        Expr::Ident(n, span) => {
            if n == name { out.push(*span); }
        }
        Expr::Call { callee, args, .. } => {
            collect_refs_expr(callee, name, out);
            for arg in args { collect_refs_expr(&arg.expr, name, out); }
        }
        Expr::Field { obj, .. } => collect_refs_expr(obj, name, out),
        Expr::Index { obj, idx } => {
            collect_refs_expr(obj, name, out);
            collect_refs_expr(idx, name, out);
        }
        Expr::New { class, class_span, args, .. } => {
            if class == name { out.push(*class_span); }
            for arg in args { collect_refs_expr(&arg.expr, name, out); }
        }
        Expr::BinOp { left, right, .. } => {
            collect_refs_expr(left, name, out);
            collect_refs_expr(right, name, out);
        }
        Expr::UnaryOp { expr, .. } => collect_refs_expr(expr, name, out),
        Expr::Ternary { cond, then, else_ } => {
            collect_refs_expr(cond, name, out);
            collect_refs_expr(then, name, out);
            collect_refs_expr(else_, name, out);
        }
        Expr::Is { expr, type_name, .. } => {
            collect_refs_expr(expr, name, out);
            // type_name 无 span，不计入引用位置
            let _ = type_name;
        }
        Expr::Fn(f) => collect_refs_fndef(f, name, out),
        Expr::Protect(stmts) => collect_refs_stmts(stmts, name, out),
        Expr::Array(elems) => {
            for e in elems { collect_refs_expr(e, name, out); }
        }
        Expr::Dict(pairs) => {
            for (k, v) in pairs {
                collect_refs_expr(k, name, out);
                collect_refs_expr(v, name, out);
            }
        }
        Expr::Await(e) | Expr::Try(e) => collect_refs_expr(e, name, out),
        _ => {}
    }
}

use tower_lsp::lsp_types::*;
use nb_core::lexer::{Lexer, Token};
use nb_core::parser::{Parser, ast::*};

// 语义 token 类型索引（与 LEGEND 中的顺序对应）
pub const TOKEN_TYPE_NAMESPACE: u32  = 0;
pub const TOKEN_TYPE_CLASS: u32      = 1;
pub const TOKEN_TYPE_FUNCTION: u32   = 2;
pub const TOKEN_TYPE_VARIABLE: u32   = 3;
pub const TOKEN_TYPE_PARAMETER: u32  = 4;
pub const TOKEN_TYPE_KEYWORD: u32    = 5;
pub const TOKEN_TYPE_STRING: u32     = 6;
pub const TOKEN_TYPE_NUMBER: u32     = 7;
pub const TOKEN_TYPE_COMMENT: u32    = 8;

pub fn semantic_token_legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: vec![
            SemanticTokenType::NAMESPACE,
            SemanticTokenType::CLASS,
            SemanticTokenType::FUNCTION,
            SemanticTokenType::VARIABLE,
            SemanticTokenType::PARAMETER,
            SemanticTokenType::KEYWORD,
            SemanticTokenType::STRING,
            SemanticTokenType::NUMBER,
            SemanticTokenType::COMMENT,
        ],
        token_modifiers: vec![],
    }
}

/// 原始 token（编码前）
struct RawToken {
    line: u32,
    col: u32,
    length: u32,
    token_type: u32,
}

/// 对给定源码生成语义 token 列表（LSP 编码格式）
pub fn get_semantic_tokens(source: &str) -> Vec<SemanticToken> {
    let mut raw_tokens: Vec<RawToken> = Vec::new();

    // 第一遍：从 Token 流提取关键字、字符串、数字的位置
    if let Ok(token_list) = Lexer::new(source).tokenize() {
        for twp in &token_list {
            let line = (twp.line as u32).saturating_sub(1);
            let col  = (twp.col  as u32).saturating_sub(1);
            match &twp.token {
                // 关键字
                Token::Let | Token::Mut | Token::Fn | Token::Return |
                Token::If | Token::Else | Token::For | Token::While | Token::In |
                Token::Break | Token::Continue | Token::Class | Token::Mixin |
                Token::New | Token::Is | Token::Self_ | Token::Super |
                Token::Static | Token::Throw | Token::Protect |
                Token::Async | Token::Await | Token::Export | Token::Require |
                Token::Throws | Token::Nil | Token::True | Token::False => {
                    let kw = token_keyword_str(&twp.token);
                    raw_tokens.push(RawToken { line, col, length: kw.len() as u32, token_type: TOKEN_TYPE_KEYWORD });
                }
                // 字符串
                Token::StringLit(s) => {
                    // +2 for quotes
                    raw_tokens.push(RawToken { line, col, length: (s.len() + 2) as u32, token_type: TOKEN_TYPE_STRING });
                }
                Token::InterpolatedString(_) => {
                    // 插值字符串长度难以精确计算，跳过，由 TextMate 语法兜底
                }
                // 数字
                Token::Number(n) => {
                    let s = format_number(*n);
                    raw_tokens.push(RawToken { line, col, length: s.len() as u32, token_type: TOKEN_TYPE_NUMBER });
                }
                _ => {}
            }
        }
    }

    // 第二遍：从 AST 提取函数名、类名、参数名的精确位置
    if let Ok(token_list) = Lexer::new(source).tokenize() {
        if let Ok(stmts) = Parser::new(token_list).parse_program() {
            collect_ast_tokens(&stmts, &mut raw_tokens);
        }
    }

    // 按行列排序
    raw_tokens.sort_by(|a, b| a.line.cmp(&b.line).then(a.col.cmp(&b.col)));

    // 编码为 LSP delta 格式
    encode_semantic_tokens(&raw_tokens)
}

fn collect_ast_tokens(stmts: &[Stmt], out: &mut Vec<RawToken>) {
    for stmt in stmts {
        collect_stmt_tokens(stmt, out);
    }
}

fn collect_stmt_tokens(stmt: &Stmt, out: &mut Vec<RawToken>) {
    match stmt {
        Stmt::Let { name, name_span, .. } => {
            out.push(span_token(name_span, name.len() as u32, TOKEN_TYPE_VARIABLE));
        }
        Stmt::MultiLet { names, .. } => {
            let _ = names;
        }
        Stmt::FnDef(f) => collect_fndef_tokens(f, out),
        Stmt::ClassDef(cd) => {
            out.push(span_token(&cd.name_span, cd.name.len() as u32, TOKEN_TYPE_CLASS));
            for field in &cd.fields {
                out.push(span_token(&field.name_span, field.name.len() as u32, TOKEN_TYPE_VARIABLE));
            }
            for method in &cd.methods {
                collect_fndef_tokens(&method.fn_def, out);
            }
        }
        Stmt::MixinDef(md) => {
            out.push(span_token(&md.name_span, md.name.len() as u32, TOKEN_TYPE_CLASS));
            for req in &md.requires {
                out.push(span_token(&req.name_span, req.name.len() as u32, TOKEN_TYPE_VARIABLE));
            }
            for m in &md.methods {
                collect_fndef_tokens(m, out);
            }
        }
        Stmt::If { cond, then_body, else_ifs, else_body, .. } => {
            collect_expr_tokens(cond, out);
            collect_ast_tokens(then_body, out);
            for (c, b) in else_ifs {
                collect_expr_tokens(c, out);
                collect_ast_tokens(b, out);
            }
            if let Some(b) = else_body { collect_ast_tokens(b, out); }
        }
        Stmt::While { cond, body, .. } => {
            collect_expr_tokens(cond, out);
            collect_ast_tokens(body, out);
        }
        Stmt::ForIn { iter, body, .. } => {
            collect_expr_tokens(iter, out);
            collect_ast_tokens(body, out);
        }
        Stmt::Return(Some(e)) => collect_expr_tokens(e, out),
        Stmt::Throw(e) => collect_expr_tokens(e, out),
        Stmt::Assign { target, value } | Stmt::CompoundAssign { target, value, .. } => {
            collect_expr_tokens(target, out);
            collect_expr_tokens(value, out);
        }
        Stmt::IncDec { target, .. } => collect_expr_tokens(target, out),
        Stmt::Expr(e) => collect_expr_tokens(e, out),
        _ => {}
    }
}

fn collect_fndef_tokens(f: &FnDef, out: &mut Vec<RawToken>) {
    if let Some(name) = &f.name {
        out.push(span_token(&f.name_span, name.len() as u32, TOKEN_TYPE_FUNCTION));
    }
    for param in &f.params {
        if param.name != "self" {
            out.push(span_token(&param.name_span, param.name.len() as u32, TOKEN_TYPE_PARAMETER));
        }
    }
    collect_ast_tokens(&f.body, out);
}

fn collect_expr_tokens(expr: &Expr, out: &mut Vec<RawToken>) {
    match expr {
        Expr::Ident(name, span) => {
            if name != "self" && name != "super" {
                out.push(span_token(span, name.len() as u32, TOKEN_TYPE_VARIABLE));
            }
        }
        Expr::Call { callee, args, .. } => {
            if let Expr::Field { obj, field, field_span } = callee.as_ref() {
                collect_expr_tokens(obj, out);
                out.push(span_token(field_span, field.len() as u32, TOKEN_TYPE_FUNCTION));
            } else {
                collect_expr_tokens(callee, out);
            }
            for arg in args { collect_expr_tokens(&arg.expr, out); }
        }
        Expr::Field { obj, field, field_span } => {
            collect_expr_tokens(obj, out);
            out.push(span_token(field_span, field.len() as u32, TOKEN_TYPE_VARIABLE));
        }
        Expr::New { class_span, class, args, .. } => {
            out.push(span_token(class_span, class.len() as u32, TOKEN_TYPE_CLASS));
            for arg in args { collect_expr_tokens(&arg.expr, out); }
        }
        Expr::BinOp { left, right, .. } => {
            collect_expr_tokens(left, out);
            collect_expr_tokens(right, out);
        }
        Expr::UnaryOp { expr, .. } => collect_expr_tokens(expr, out),
        Expr::Ternary { cond, then, else_ } => {
            collect_expr_tokens(cond, out);
            collect_expr_tokens(then, out);
            collect_expr_tokens(else_, out);
        }
        Expr::Index { obj, idx } => {
            collect_expr_tokens(obj, out);
            collect_expr_tokens(idx, out);
        }
        Expr::Is { expr, .. } => collect_expr_tokens(expr, out),
        Expr::Fn(f) => collect_fndef_tokens(f, out),
        Expr::Protect(stmts) => {
            collect_ast_tokens(stmts, out);
        }
        Expr::Array(elems) => {
            for e in elems { collect_expr_tokens(e, out); }
        }
        Expr::Dict(pairs) => {
            for (k, v) in pairs {
                collect_expr_tokens(k, out);
                collect_expr_tokens(v, out);
            }
        }
        Expr::Await(e) | Expr::Try(e) => collect_expr_tokens(e, out),
        _ => {}
    }
}

fn span_token(span: &Span, length: u32, token_type: u32) -> RawToken {
    RawToken {
        line: (span.line as u32).saturating_sub(1),
        col:  (span.col  as u32).saturating_sub(1),
        length,
        token_type,
    }
}

/// 编码为 LSP delta 格式
fn encode_semantic_tokens(raw: &[RawToken]) -> Vec<SemanticToken> {
    let mut result = Vec::new();
    let mut prev_line = 0u32;
    let mut prev_col  = 0u32;

    for tok in raw {
        if tok.length == 0 { continue; }
        let delta_line = tok.line - prev_line;
        let delta_col  = if delta_line == 0 { tok.col - prev_col } else { tok.col };
        result.push(SemanticToken {
            delta_line,
            delta_start: delta_col,
            length: tok.length,
            token_type: tok.token_type,
            token_modifiers_bitset: 0,
        });
        prev_line = tok.line;
        prev_col  = tok.col;
    }
    result
}

fn token_keyword_str(tok: &Token) -> &'static str {
    match tok {
        Token::Let      => "let",
        Token::Mut      => "mut",
        Token::Fn       => "fn",
        Token::Return   => "return",
        Token::If       => "if",
        Token::Else     => "else",
        Token::For      => "for",
        Token::While    => "while",
        Token::In       => "in",
        Token::Break    => "break",
        Token::Continue => "continue",
        Token::Class    => "class",
        Token::Mixin    => "mixin",
        Token::New      => "new",
        Token::Is       => "is",
        Token::Self_    => "self",
        Token::Super    => "super",
        Token::Static   => "static",
        Token::Throw    => "throw",
        Token::Protect  => "protect",
        Token::Async    => "async",
        Token::Await    => "await",
        Token::Export   => "export",
        Token::Require  => "require",
        Token::Throws   => "throws",
        Token::Nil      => "nil",
        Token::True     => "true",
        Token::False    => "false",
        _               => "",
    }
}

fn format_number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

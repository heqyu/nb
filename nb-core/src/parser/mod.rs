pub mod ast;

use thiserror::Error;
use crate::lexer::{Token, TokenWithPos};
use ast::*;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("第 {line} 行第 {col} 列：期望 {expected}，但得到 {got}")]
    Unexpected { expected: String, got: String, line: usize, col: usize },
    #[error("第 {0} 行：意外的文件结尾")]
    UnexpectedEof(usize),
}

pub struct Parser {
    tokens: Vec<TokenWithPos>,
    pos: usize,
    /// 禁止在当前表达式中解析 struct literal（用于 if/while/for 条件）
    no_struct_lit: bool,
}

impl Parser {
    pub fn new(tokens: Vec<TokenWithPos>) -> Self {
        Self { tokens, pos: 0, no_struct_lit: false }
    }

    /// 公开给 vm 用于解析字符串插值表达式
    pub fn parse_expr_pub(&mut self) -> Result<Expr, ParseError> {
        self.parse_expr()
    }

    pub fn parse_program(&mut self) -> Result<Vec<Stmt>, ParseError> {
        let mut stmts = Vec::new();
        while !self.is_eof() {
            stmts.push(self.parse_stmt()?);
        }
        Ok(stmts)
    }

    fn peek(&self) -> &Token {
        self.tokens.get(self.pos).map(|t| &t.token).unwrap_or(&Token::Eof)
    }

    fn peek_pos(&self) -> (usize, usize) {
        self.tokens.get(self.pos).map(|t| (t.line, t.col)).unwrap_or((0, 0))
    }

    fn peek_span(&self) -> Span {
        let (line, col) = self.peek_pos();
        Span::new(line, col)
    }

    fn advance(&mut self) -> &Token {
        let tok = &self.tokens[self.pos].token;
        if self.pos < self.tokens.len() - 1 { self.pos += 1; }
        tok
    }

    fn is_eof(&self) -> bool {
        matches!(self.peek(), Token::Eof)
    }

    fn expect(&mut self, tok: &Token) -> Result<(), ParseError> {
        if self.peek() == tok {
            self.advance();
            Ok(())
        } else {
            let (line, col) = self.peek_pos();
            Err(ParseError::Unexpected {
                expected: format!("{tok:?}"),
                got: format!("{:?}", self.peek()),
                line, col,
            })
        }
    }

    /// 消费标识符，同时返回其 Span
    fn expect_ident_with_span(&mut self) -> Result<(String, Span), ParseError> {
        let span = self.peek_span();
        match self.peek().clone() {
            Token::Ident(s) => { self.advance(); Ok((s, span)) }
            Token::Self_    => { self.advance(); Ok(("self".into(), span)) }
            Token::Super    => { self.advance(); Ok(("super".into(), span)) }
            _ => {
                let (line, col) = self.peek_pos();
                Err(ParseError::Unexpected {
                    expected: "标识符".into(),
                    got: format!("{:?}", self.peek()),
                    line, col,
                })
            }
        }
    }

    fn expect_ident(&mut self) -> Result<String, ParseError> {
        self.expect_ident_with_span().map(|(s, _)| s)
    }

    fn peek_next_token(&self) -> &Token {
        self.tokens.get(self.pos + 1).map(|t| &t.token).unwrap_or(&Token::Eof)
    }

    fn check(&self, tok: &Token) -> bool { self.peek() == tok }

    fn eat(&mut self, tok: &Token) -> bool {
        if self.peek() == tok { self.advance(); true } else { false }
    }

    fn parse_stmt(&mut self) -> Result<Stmt, ParseError> {
        match self.peek().clone() {
            Token::Let      => self.parse_let(),
            Token::Fn       => { let f = self.parse_fn_def(false)?; Ok(Stmt::FnDef(f)) }
            Token::Async    => self.parse_async_fn(),
            Token::Return   => self.parse_return(),
            Token::If       => self.parse_if(),
            Token::For      => self.parse_for(),
            Token::While    => self.parse_while(),
            Token::Break    => { self.advance(); Ok(Stmt::Break) }
            Token::Continue => { self.advance(); Ok(Stmt::Continue) }
            Token::Class    => self.parse_class(),
            Token::Mixin    => self.parse_mixin(),
            Token::Throw    => self.parse_throw(),
            Token::Export   => self.parse_export(),
            _               => self.parse_expr_stmt(),
        }
    }

    fn parse_let(&mut self) -> Result<Stmt, ParseError> {
        let span = self.peek_span();
        self.expect(&Token::Let)?;
        let mutable = self.eat(&Token::Mut);
        let (name, name_span) = self.expect_ident_with_span()?;
        // 支持多变量：let a, b, c = expr
        let mut extra_names: Vec<String> = Vec::new();
        while self.eat(&Token::Comma) {
            extra_names.push(self.expect_ident()?);
        }
        // 类型注解：只在单变量且下一个 token 是 Colon 且再下一个是 Ident 时解析
        // 避免把三元表达式的 : 误当作类型注解
        let type_ann = if extra_names.is_empty() {
            if self.check(&Token::Colon) {
                let is_type_ann = matches!(self.peek_next_token(), Token::Ident(_));
                if is_type_ann {
                    self.advance(); // eat Colon
                    Some(self.parse_type_ann()?)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };
        let value = if self.eat(&Token::Assign) { Some(self.parse_expr()?) } else { None };
        if extra_names.is_empty() {
            Ok(Stmt::Let { name, mutable, type_ann, value, span, name_span })
        } else {
            Ok(Stmt::MultiLet {
                names: std::iter::once(name).chain(extra_names).collect(),
                mutable,
                value,
                span,
            })
        }
    }

    fn parse_fn_def(&mut self, async_: bool) -> Result<FnDef, ParseError> {
        let span = self.peek_span();
        self.expect(&Token::Fn)?;
        // 解析函数名，可能带 receiver：fn Player.method 或 fn name 或匿名
        let (receiver, name, name_span) = match self.peek().clone() {
            Token::Ident(s) => {
                let s_span = self.peek_span();
                self.advance();
                // 检查是否是 receiver.method 形式
                if self.eat(&Token::Dot) {
                    let (method, method_span) = self.expect_ident_with_span()?;
                    (Some(s), Some(method), method_span)
                } else {
                    (None, Some(s), s_span)
                }
            }
            _ => (None, None, span),  // 匿名函数
        };
        self.expect(&Token::LParen)?;
        let params = self.parse_params()?;
        self.expect(&Token::RParen)?;
        let ret_type = if self.eat(&Token::Colon) { Some(self.parse_type_ann()?) } else { None };
        let throws = self.eat(&Token::Throws);
        let body = self.parse_block()?;
        Ok(FnDef { name, name_span, receiver, async_, params, ret_type, throws, body, span })
    }

    fn parse_async_fn(&mut self) -> Result<Stmt, ParseError> {
        self.expect(&Token::Async)?;
        let f = self.parse_fn_def(true)?;
        Ok(Stmt::FnDef(f))
    }

    fn parse_params(&mut self) -> Result<Vec<Param>, ParseError> {
        let mut params = Vec::new();
        while !self.check(&Token::RParen) && !self.is_eof() {
            let mutable = self.eat(&Token::Mut);
            let (name, name_span) = self.expect_ident_with_span()?;
            let type_ann = if self.eat(&Token::Colon) { Some(self.parse_type_ann()?) } else { None };
            params.push(Param { name, name_span, mutable, type_ann });
            if !self.eat(&Token::Comma) { break; }
        }
        Ok(params)
    }

    fn parse_return(&mut self) -> Result<Stmt, ParseError> {
        self.expect(&Token::Return)?;
        if self.is_eof() || self.check(&Token::RBrace) {
            Ok(Stmt::Return(None))
        } else {
            Ok(Stmt::Return(Some(self.parse_expr()?)))
        }
    }

    fn parse_if(&mut self) -> Result<Stmt, ParseError> {
        let span = self.peek_span();
        self.expect(&Token::If)?;
        let cond = self.parse_expr_no_struct_lit()?;
        let then_body = self.parse_block()?;
        let mut else_ifs = Vec::new();
        let mut else_body = None;
        while self.eat(&Token::Else) {
            if self.check(&Token::If) {
                self.advance();
                let c = self.parse_expr_no_struct_lit()?;
                let b = self.parse_block()?;
                else_ifs.push((c, b));
            } else {
                else_body = Some(self.parse_block()?);
                break;
            }
        }
        Ok(Stmt::If { cond, then_body, else_ifs, else_body, span })
    }

    fn parse_for(&mut self) -> Result<Stmt, ParseError> {
        let span = self.peek_span();
        self.expect(&Token::For)?;
        let first = self.expect_ident()?;
        if self.eat(&Token::Comma) {
            let value_mutable = self.eat(&Token::Mut);
            let value = self.expect_ident()?;
            self.expect(&Token::In)?;
            let iter = self.parse_expr_no_struct_lit()?;
            let body = self.parse_block()?;
            Ok(Stmt::ForIn { key: first, value: Some(value), value_mutable, iter, body, span })
        } else {
            self.expect(&Token::In)?;
            let iter = self.parse_expr_no_struct_lit()?;
            let body = self.parse_block()?;
            Ok(Stmt::ForIn { key: first, value: None, value_mutable: false, iter, body, span })
        }
    }

    fn parse_while(&mut self) -> Result<Stmt, ParseError> {
        let span = self.peek_span();
        self.expect(&Token::While)?;
        let cond = self.parse_expr_no_struct_lit()?;
        let body = self.parse_block()?;
        Ok(Stmt::While { cond, body, span })
    }

    fn parse_class(&mut self) -> Result<Stmt, ParseError> {
        let span = self.peek_span();
        self.expect(&Token::Class)?;
        let (name, name_span) = self.expect_ident_with_span()?;
        let mut mixins = Vec::new();
        if self.eat(&Token::Colon) {
            mixins.push(self.expect_ident()?);
            while self.eat(&Token::Comma) {
                mixins.push(self.expect_ident()?);
            }
        }
        self.expect(&Token::LBrace)?;
        let mut fields = Vec::new();
        while !self.check(&Token::RBrace) && !self.is_eof() {
            let mutable = self.eat(&Token::Mut);
            let (fname, fname_span) = self.expect_ident_with_span()?;
            let type_ann = if self.eat(&Token::Colon) { Some(self.parse_type_ann()?) } else { None };
            fields.push(FieldDef { name: fname, name_span: fname_span, mutable, type_ann });
        }
        self.expect(&Token::RBrace)?;
        Ok(Stmt::ClassDef(ClassDef { name, name_span, mixins, fields, span }))
    }

    fn parse_mixin(&mut self) -> Result<Stmt, ParseError> {
        let span = self.peek_span();
        self.expect(&Token::Mixin)?;
        let (name, name_span) = self.expect_ident_with_span()?;
        self.expect(&Token::LBrace)?;
        let mut requires = Vec::new();
        let mut methods = Vec::new();
        while !self.check(&Token::RBrace) && !self.is_eof() {
            if self.eat(&Token::Require) {
                let mutable = self.eat(&Token::Mut);
                let (fname, fname_span) = self.expect_ident_with_span()?;
                let type_ann = if self.eat(&Token::Colon) { Some(self.parse_type_ann()?) } else { None };
                requires.push(FieldDef { name: fname, name_span: fname_span, mutable, type_ann });
            } else {
                let async_ = self.eat(&Token::Async);
                let f = self.parse_fn_def(async_)?;
                methods.push(f);
            }
        }
        self.expect(&Token::RBrace)?;
        Ok(Stmt::MixinDef(MixinDef { name, name_span, requires, methods, span }))
    }

    fn parse_throw(&mut self) -> Result<Stmt, ParseError> {
        self.expect(&Token::Throw)?;
        Ok(Stmt::Throw(self.parse_expr()?))
    }

    fn parse_export(&mut self) -> Result<Stmt, ParseError> {
        self.expect(&Token::Export)?;
        self.expect(&Token::LBrace)?;
        let mut names = Vec::new();
        while !self.check(&Token::RBrace) && !self.is_eof() {
            names.push(self.expect_ident()?);
            if !self.eat(&Token::Comma) { break; }
        }
        self.expect(&Token::RBrace)?;
        Ok(Stmt::Export(names))
    }

    fn parse_expr_stmt(&mut self) -> Result<Stmt, ParseError> {
        let (line, col) = self.peek_pos();
        let mut expr = self.parse_expr()?;
        // 后缀 ? 只在语句层面处理（避免和三元 ? 冲突）
        if self.eat(&Token::Question) {
            expr = Expr::Try(Box::new(expr));
        }
        match self.peek().clone() {
            Token::Assign      => { self.advance(); let v = self.parse_expr()?; Ok(Stmt::Assign { target: expr, value: v }) }
            Token::PlusAssign  => { self.advance(); let v = self.parse_expr()?; Ok(Stmt::CompoundAssign { target: expr, op: BinOp::Add, value: v }) }
            Token::MinusAssign => { self.advance(); let v = self.parse_expr()?; Ok(Stmt::CompoundAssign { target: expr, op: BinOp::Sub, value: v }) }
            Token::StarAssign  => { self.advance(); let v = self.parse_expr()?; Ok(Stmt::CompoundAssign { target: expr, op: BinOp::Mul, value: v }) }
            Token::SlashAssign => { self.advance(); let v = self.parse_expr()?; Ok(Stmt::CompoundAssign { target: expr, op: BinOp::Div, value: v }) }
            Token::PlusPlus    => { self.advance(); Ok(Stmt::IncDec { target: expr, inc: true }) }
            Token::MinusMinus  => { self.advance(); Ok(Stmt::IncDec { target: expr, inc: false }) }
            _ => {
                // 只有调用表达式、await、try(?) 才允许单独成语句
                let valid = matches!(
                    &expr,
                    Expr::Call { .. } | Expr::Await(_) | Expr::Try(_)
                );
                if valid {
                    Ok(Stmt::Expr(expr))
                } else {
                    Err(ParseError::Unexpected {
                        expected: "赋值或函数调用".into(),
                        got: format!("{:?}", self.peek()),
                        line,
                        col,
                    })
                }
            }
        }
    }

    fn parse_block(&mut self) -> Result<Vec<Stmt>, ParseError> {
        self.expect(&Token::LBrace)?;
        let mut stmts = Vec::new();
        while !self.check(&Token::RBrace) && !self.is_eof() {
            stmts.push(self.parse_stmt()?);
        }
        self.expect(&Token::RBrace)?;
        Ok(stmts)
    }

    fn parse_expr(&mut self) -> Result<Expr, ParseError> { self.parse_ternary() }

    /// 在禁止 struct literal 的上下文中解析表达式（if/while/for 条件）
    fn parse_expr_no_struct_lit(&mut self) -> Result<Expr, ParseError> {
        let saved = self.no_struct_lit;
        self.no_struct_lit = true;
        let result = self.parse_ternary();
        self.no_struct_lit = saved;
        result
    }

    fn parse_ternary(&mut self) -> Result<Expr, ParseError> {
        let expr = self.parse_range()?;
        if self.eat(&Token::Question) {
            let then = self.parse_range()?;
            self.expect(&Token::Colon)?;
            let else_ = self.parse_ternary()?;
            Ok(Expr::Ternary { cond: Box::new(expr), then: Box::new(then), else_: Box::new(else_) })
        } else {
            Ok(expr)
        }
    }

    fn parse_range(&mut self) -> Result<Expr, ParseError> {
        let left = self.parse_or()?;
        if self.eat(&Token::DotDot) {
            let right = self.parse_or()?;
            Ok(Expr::BinOp { left: Box::new(left), op: BinOp::Range, right: Box::new(right) })
        } else {
            Ok(left)
        }
    }

    fn parse_or(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_and()?;
        while self.eat(&Token::Or) {
            let right = self.parse_and()?;
            left = Expr::BinOp { left: Box::new(left), op: BinOp::Or, right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_eq()?;
        while self.eat(&Token::And) {
            let right = self.parse_eq()?;
            left = Expr::BinOp { left: Box::new(left), op: BinOp::And, right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_eq(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_cmp()?;
        loop {
            let op = match self.peek() {
                Token::Eq => BinOp::Eq, Token::NotEq => BinOp::NotEq, _ => break,
            };
            self.advance();
            let right = self.parse_cmp()?;
            left = Expr::BinOp { left: Box::new(left), op, right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_cmp(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_add()?;
        loop {
            let op = match self.peek() {
                Token::Lt => BinOp::Lt, Token::LtEq => BinOp::LtEq,
                Token::Gt => BinOp::Gt, Token::GtEq => BinOp::GtEq, _ => break,
            };
            self.advance();
            let right = self.parse_add()?;
            left = Expr::BinOp { left: Box::new(left), op, right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_add(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_mul()?;
        loop {
            let op = match self.peek() {
                Token::Plus => BinOp::Add, Token::Minus => BinOp::Sub, _ => break,
            };
            self.advance();
            let right = self.parse_mul()?;
            left = Expr::BinOp { left: Box::new(left), op, right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_mul(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_unary()?;
        loop {
            let op = match self.peek() {
                Token::Star => BinOp::Mul, Token::Slash => BinOp::Div,
                Token::Percent => BinOp::Mod, _ => break,
            };
            self.advance();
            let right = self.parse_unary()?;
            left = Expr::BinOp { left: Box::new(left), op, right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        match self.peek().clone() {
            Token::Not   => { self.advance(); Ok(Expr::UnaryOp { op: UnaryOp::Not, expr: Box::new(self.parse_unary()?) }) }
            Token::Minus => { self.advance(); Ok(Expr::UnaryOp { op: UnaryOp::Neg, expr: Box::new(self.parse_unary()?) }) }
            _            => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_primary()?;
        loop {
            match self.peek().clone() {
                Token::Dot => {
                    self.advance();
                    let (field, field_span) = self.expect_ident_with_span()?;
                    if self.check(&Token::LParen) {
                        let span = self.peek_span();
                        let args = self.parse_call_args()?;
                        expr = Expr::Call {
                            callee: Box::new(Expr::Field { obj: Box::new(expr), field, field_span }),
                            args,
                            span,
                        };
                    } else {
                        expr = Expr::Field { obj: Box::new(expr), field, field_span };
                    }
                }
                Token::LBracket => {
                    self.advance();
                    let idx = self.parse_expr()?;
                    self.expect(&Token::RBracket)?;
                    expr = Expr::Index { obj: Box::new(expr), idx: Box::new(idx) };
                }
                Token::LParen => {
                    let span = self.peek_span();
                    let args = self.parse_call_args()?;
                    expr = Expr::Call { callee: Box::new(expr), args, span };
                }
                Token::Is => {
                    self.advance();
                    let (type_name, type_span) = self.expect_ident_with_span()?;
                    expr = Expr::Is { expr: Box::new(expr), type_name, type_span };
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_call_args(&mut self) -> Result<Vec<CallArg>, ParseError> {
        self.expect(&Token::LParen)?;
        let mut args = Vec::new();
        while !self.check(&Token::RParen) && !self.is_eof() {
            let mutable = self.eat(&Token::Mut);
            let expr = self.parse_expr()?;
            args.push(CallArg { mutable, expr });
            if !self.eat(&Token::Comma) { break; }
        }
        self.expect(&Token::RParen)?;
        Ok(args)
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        match self.peek().clone() {
            Token::Nil                   => { self.advance(); Ok(Expr::Nil) }
            Token::True                  => { self.advance(); Ok(Expr::Bool(true)) }
            Token::False                 => { self.advance(); Ok(Expr::Bool(false)) }
            Token::Number(n)             => { self.advance(); Ok(Expr::Number(n)) }
            Token::StringLit(s)          => { self.advance(); Ok(Expr::StringLit(s)) }
            Token::InterpolatedString(p) => { self.advance(); Ok(Expr::InterpolatedString(p)) }
            Token::Ident(s) => {
                let span = self.peek_span();
                self.advance();
                // ClassName { field = val, .. } 结构体字面量
                // 在 if/while/for 条件中禁止解析，避免与语句块 { 歧义
                if !self.no_struct_lit && self.check(&Token::LBrace) {
                    self.parse_struct_lit(s, span)
                } else {
                    Ok(Expr::Ident(s, span))
                }
            }
            Token::Self_ => {
                let span = self.peek_span();
                self.advance();
                Ok(Expr::Ident("self".into(), span))
            }
            Token::Super => {
                let span = self.peek_span();
                self.advance();
                Ok(Expr::Ident("super".into(), span))
            }
            Token::LParen => {
                self.advance();
                let e = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                Ok(e)
            }
            Token::LBracket  => self.parse_array(),
            Token::LBrace    => self.parse_dict(),
            Token::Fn        => { let f = self.parse_fn_def(false)?; Ok(Expr::Fn(Box::new(f))) }
            Token::Async     => { self.advance(); let f = self.parse_fn_def(true)?; Ok(Expr::Fn(Box::new(f))) }
            Token::Protect   => self.parse_protect(),
            Token::Await     => { self.advance(); Ok(Expr::Await(Box::new(self.parse_unary()?))) }
            _ => {
                let (line, col) = self.peek_pos();
                Err(ParseError::Unexpected {
                    expected: "表达式".into(),
                    got: format!("{:?}", self.peek()),
                    line, col,
                })
            }
        }
    }

    fn parse_array(&mut self) -> Result<Expr, ParseError> {
        self.expect(&Token::LBracket)?;
        let mut elems = Vec::new();
        while !self.check(&Token::RBracket) && !self.is_eof() {
            elems.push(self.parse_expr()?);
            if !self.eat(&Token::Comma) { break; }
        }
        self.expect(&Token::RBracket)?;
        Ok(Expr::Array(elems))
    }

    fn parse_dict(&mut self) -> Result<Expr, ParseError> {
        self.expect(&Token::LBrace)?;
        let mut pairs = Vec::new();
        while !self.check(&Token::RBrace) && !self.is_eof() {
            let key = self.parse_expr()?;
            self.expect(&Token::Assign)?;
            let val = self.parse_expr()?;
            pairs.push((key, val));
            if !self.eat(&Token::Comma) { break; }
        }
        self.expect(&Token::RBrace)?;
        Ok(Expr::Dict(pairs))
    }

    fn parse_struct_lit(&mut self, class: String, class_span: Span) -> Result<Expr, ParseError> {
        self.expect(&Token::LBrace)?;
        let mut fields = Vec::new();
        while !self.check(&Token::RBrace) && !self.is_eof() {
            let (fname, fname_span) = self.expect_ident_with_span()?;
            self.expect(&Token::Assign)?;
            let val = self.parse_expr()?;
            fields.push((fname, fname_span, val));
            if !self.eat(&Token::Comma) { break; }
        }
        self.expect(&Token::RBrace)?;
        Ok(Expr::StructLit { class, class_span, fields })
    }

    fn parse_protect(&mut self) -> Result<Expr, ParseError> {
        self.expect(&Token::Protect)?;
        let body = self.parse_block()?;
        Ok(Expr::Protect(body))
    }

    fn parse_type_ann(&mut self) -> Result<TypeAnnotation, ParseError> {
        let name = self.expect_ident()?;
        if self.eat(&Token::Lt) {
            let mut args = Vec::new();
            args.push(self.parse_type_ann()?);
            while self.eat(&Token::Comma) {
                args.push(self.parse_type_ann()?);
            }
            self.expect(&Token::Gt)?;
            Ok(TypeAnnotation::Generic(name, args))
        } else {
            Ok(TypeAnnotation::Simple(name))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;

    fn parse(src: &str) -> Vec<Stmt> {
        let tokens = Lexer::new(src).tokenize().unwrap();
        Parser::new(tokens).parse_program().unwrap()
    }

    #[test]
    fn test_let() {
        let stmts = parse("let x = 10");
        assert!(matches!(stmts[0], Stmt::Let { mutable: false, .. }));
    }

    #[test]
    fn test_let_mut() {
        let stmts = parse("let mut x = 10");
        assert!(matches!(stmts[0], Stmt::Let { mutable: true, .. }));
    }

    #[test]
    fn test_fn_def() {
        let stmts = parse("fn add(a: number, b: number): number { return a + b }");
        assert!(matches!(stmts[0], Stmt::FnDef(_)));
    }

    #[test]
    fn test_if_else() {
        let stmts = parse("if x > 0 { x = 1 } else { x = 2 }");
        assert!(matches!(stmts[0], Stmt::If { .. }));
    }

    #[test]
    fn test_for_range() {
        let stmts = parse("for i in 0..5 { print(i) }");
        assert!(matches!(stmts[0], Stmt::ForIn { .. }));
    }

    #[test]
    fn test_for_iter() {
        let stmts = parse("for i, v in arr { print(v) }");
        assert!(matches!(stmts[0], Stmt::ForIn { value: Some(_), .. }));
    }

    #[test]
    fn test_class_def() {
        let stmts = parse(r#"class Animal {
            name: string
            mut hp: number
        }"#);
        assert!(matches!(stmts[0], Stmt::ClassDef(_)));
    }

    #[test]
    fn test_mixin_def() {
        let stmts = parse(r#"mixin Damageable {
            require hp: number
            fn damage(mut self, val: number) { self.hp -= val }
        }"#);
        assert!(matches!(stmts[0], Stmt::MixinDef(_)));
    }

    #[test]
    fn test_protect() {
        let stmts = parse("let err, result = protect { return parse(x) }");
        assert!(matches!(stmts[0], Stmt::MultiLet { .. }));
    }

    #[test]
    fn test_struct_lit() {
        let stmts = parse(r#"let x = Animal { name = "cat", hp = 100 }"#);
        assert!(matches!(stmts[0], Stmt::Let { .. }));
    }

    #[test]
    fn test_method_receiver() {
        let stmts = parse(r#"fn Animal.speak(self) { return "roar" }"#);
        if let Stmt::FnDef(f) = &stmts[0] {
            assert_eq!(f.receiver, Some("Animal".into()));
            assert_eq!(f.name, Some("speak".into()));
        } else {
            panic!("应该是 FnDef");
        }
    }

    #[test]
    fn test_span_fn() {
        let stmts = parse("fn add(a, b) { return a + b }");
        if let Stmt::FnDef(f) = &stmts[0] {
            assert_eq!(f.span.line, 1);
            assert_eq!(f.name_span.line, 1);
            assert!(f.name_span.col > f.span.col); // 函数名在 fn 之后
        } else {
            panic!("应该是 FnDef");
        }
    }

    #[test]
    fn test_span_ident() {
        let stmts = parse("let x = 10");
        if let Stmt::Let { name_span, .. } = &stmts[0] {
            assert_eq!(name_span.line, 1);
            assert!(name_span.col > 0);
        } else {
            panic!("应该是 Let");
        }
    }
}

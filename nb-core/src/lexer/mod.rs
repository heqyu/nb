use thiserror::Error;

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // 字面量
    Number(f64),
    StringLit(String),
    InterpolatedString(Vec<StringPart>),
    True,
    False,
    Nil,

    // 标识符
    Ident(String),

    // 关键字
    Let,
    Mut,
    Fn,
    Return,
    If,
    Else,
    For,
    In,
    While,
    Break,
    Continue,
    Class,
    Mixin,
    Require,
    Is,
    Self_,
    Super,
    Throw,
    Protect,
    Async,
    Await,
    Throws,
    Export,

    // 操作符
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    And,
    Or,
    Not,
    Assign,
    PlusAssign,
    MinusAssign,
    StarAssign,
    SlashAssign,
    PlusPlus,
    MinusMinus,
    Question,
    Colon,
    Dot,
    DotDot,
    Arrow,

    // 分隔符
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Semicolon,
    At,

    // 特殊
    Eof,
}

/// 字符串插值的组成部分
#[derive(Debug, Clone, PartialEq)]
pub enum StringPart {
    Literal(String),
    /// 原始表达式文本 + 在源文件中的起始行列（1-based），由 parser 进一步解析
    Expr(String, usize, usize),
}

#[derive(Debug, Clone)]
pub struct TokenWithPos {
    pub token: Token,
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Error)]
pub enum LexError {
    #[error("未知字符 '{0}' 在第 {1} 行第 {2} 列")]
    UnknownChar(char, usize, usize),
    #[error("未终止的字符串，在第 {0} 行")]
    UnterminatedString(usize),
    #[error("无效数字 '{0}' 在第 {1} 行")]
    InvalidNumber(String, usize),
}

pub struct Lexer {
    source: Vec<char>,
    pos: usize,
    line: usize,
    col: usize,
}

impl Lexer {
    pub fn new(source: &str) -> Self {
        Self {
            source: source.chars().collect(),
            pos: 0,
            line: 1,
            col: 1,
        }
    }

    pub fn tokenize(&mut self) -> Result<Vec<TokenWithPos>, LexError> {
        let mut tokens = Vec::new();
        loop {
            self.skip_whitespace_and_comments();
            if self.is_eof() {
                tokens.push(TokenWithPos { token: Token::Eof, line: self.line, col: self.col });
                break;
            }
            let tok = self.next_token()?;
            tokens.push(tok);
        }
        Ok(tokens)
    }

    fn peek(&self) -> Option<char> {
        self.source.get(self.pos).copied()
    }

    fn peek_next(&self) -> Option<char> {
        self.source.get(self.pos + 1).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.source.get(self.pos).copied();
        if let Some(c) = ch {
            self.pos += 1;
            if c == '\n' { self.line += 1; self.col = 1; }
            else { self.col += 1; }
        }
        ch
    }

    fn is_eof(&self) -> bool {
        self.pos >= self.source.len()
    }

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            // 跳过空白
            while let Some(c) = self.peek() {
                if c.is_whitespace() { self.advance(); } else { break; }
            }
            if self.peek() == Some('/') {
                match self.peek_next() {
                    // 单行注释  // ...  或文档注释 /// ...（词法层面统一跳过）
                    Some('/') => {
                        self.advance(); self.advance();
                        while let Some(c) = self.peek() {
                            if c == '\n' { break; }
                            self.advance();
                        }
                    }
                    // 多行注释  /* ... */
                    Some('*') => {
                        self.advance(); self.advance(); // 消耗 /*
                        loop {
                            match self.peek() {
                                None => break, // 未闭合，到 EOF 为止
                                Some('*') if self.peek_next() == Some('/') => {
                                    self.advance(); self.advance(); // 消耗 */
                                    break;
                                }
                                _ => { self.advance(); }
                            }
                        }
                    }
                    _ => break,
                }
            } else {
                break;
            }
        }
    }

    fn next_token(&mut self) -> Result<TokenWithPos, LexError> {
        let line = self.line;
        let col = self.col;
        let ch = self.peek().unwrap();

        let token = match ch {
            '0'..='9' => self.lex_number()?,
            '"' => self.lex_string()?,
            'a'..='z' | 'A'..='Z' | '_' => self.lex_ident_or_keyword(),
            '+' => {
                self.advance();
                match self.peek() {
                    Some('+') => { self.advance(); Token::PlusPlus }
                    Some('=') => { self.advance(); Token::PlusAssign }
                    _ => Token::Plus
                }
            }
            '-' => {
                self.advance();
                match self.peek() {
                    Some('-') => { self.advance(); Token::MinusMinus }
                    Some('=') => { self.advance(); Token::MinusAssign }
                    Some('>') => { self.advance(); Token::Arrow }
                    _ => Token::Minus
                }
            }
            '*' => {
                self.advance();
                if self.peek() == Some('=') { self.advance(); Token::StarAssign } else { Token::Star }
            }
            '/' => {
                self.advance();
                if self.peek() == Some('=') { self.advance(); Token::SlashAssign } else { Token::Slash }
            }
            '%' => { self.advance(); Token::Percent }
            '=' => {
                self.advance();
                if self.peek() == Some('=') { self.advance(); Token::Eq } else { Token::Assign }
            }
            '!' => {
                self.advance();
                if self.peek() == Some('=') { self.advance(); Token::NotEq } else { Token::Not }
            }
            '<' => {
                self.advance();
                if self.peek() == Some('=') { self.advance(); Token::LtEq } else { Token::Lt }
            }
            '>' => {
                self.advance();
                if self.peek() == Some('=') { self.advance(); Token::GtEq } else { Token::Gt }
            }
            '&' => {
                self.advance();
                if self.peek() == Some('&') { self.advance(); Token::And }
                else { return Err(LexError::UnknownChar('&', line, col)); }
            }
            '|' => {
                self.advance();
                if self.peek() == Some('|') { self.advance(); Token::Or }
                else { return Err(LexError::UnknownChar('|', line, col)); }
            }
            '?' => { self.advance(); Token::Question }
            ':' => { self.advance(); Token::Colon }
            '.' => {
                self.advance();
                if self.peek() == Some('.') { self.advance(); Token::DotDot } else { Token::Dot }
            }
            '(' => { self.advance(); Token::LParen }
            ')' => { self.advance(); Token::RParen }
            '{' => { self.advance(); Token::LBrace }
            '}' => { self.advance(); Token::RBrace }
            '[' => { self.advance(); Token::LBracket }
            ']' => { self.advance(); Token::RBracket }
            ',' => { self.advance(); Token::Comma }
            ';' => { self.advance(); Token::Semicolon }
            '@' => { self.advance(); Token::At }
            c => { self.advance(); return Err(LexError::UnknownChar(c, line, col)); }
        };

        Ok(TokenWithPos { token, line, col })
    }

    fn lex_number(&mut self) -> Result<Token, LexError> {
        let line = self.line;
        let mut s = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                s.push(c);
                self.advance();
            } else if c == '.' && self.peek_next() != Some('.') {
                // 只有下一个字符不是 '.' 时才作为小数点（避免把 .. 运算符吃掉）
                s.push(c);
                self.advance();
            } else {
                break;
            }
        }
        s.parse::<f64>().map(Token::Number).map_err(|_| LexError::InvalidNumber(s, line))
    }

    fn lex_string(&mut self) -> Result<Token, LexError> {
        let line = self.line;
        self.advance(); // 跳过开头 "
        let mut parts: Vec<StringPart> = Vec::new();
        let mut current = String::new();

        loop {
            match self.peek() {
                None => return Err(LexError::UnterminatedString(line)),
                Some('"') => { self.advance(); break; }
                Some('\\') => {
                    self.advance();
                    match self.advance() {
                        Some('n')  => current.push('\n'),
                        Some('t')  => current.push('\t'),
                        Some('\\') => current.push('\\'),
                        Some('"')  => current.push('"'),
                        Some('$')  => current.push('$'),
                        Some(c)    => { current.push('\\'); current.push(c); }
                        None       => return Err(LexError::UnterminatedString(line)),
                    }
                }
                Some('$') if self.peek_next() == Some('{') => {
                    if !current.is_empty() {
                        parts.push(StringPart::Literal(current.clone()));
                        current.clear();
                    }
                    self.advance(); // $
                    self.advance(); // {
                    let expr_line = self.line;
                    let expr_col  = self.col;
                    let expr = self.lex_interpolation()?;
                    parts.push(StringPart::Expr(expr, expr_line, expr_col));
                }
                Some(c) => { current.push(c); self.advance(); }
            }
        }

        if parts.is_empty() {
            Ok(Token::StringLit(current))
        } else {
            if !current.is_empty() {
                parts.push(StringPart::Literal(current));
            }
            Ok(Token::InterpolatedString(parts))
        }
    }

    /// 读取 ${...} 内部的表达式，支持嵌套大括号
    fn lex_interpolation(&mut self) -> Result<String, LexError> {
        let line = self.line;
        let mut expr = String::new();
        let mut depth = 1;
        loop {
            match self.peek() {
                None => return Err(LexError::UnterminatedString(line)),
                Some('{') => { depth += 1; expr.push('{'); self.advance(); }
                Some('}') => {
                    depth -= 1;
                    self.advance();
                    if depth == 0 { break; }
                    expr.push('}');
                }
                Some(c) => { expr.push(c); self.advance(); }
            }
        }
        Ok(expr)
    }

    fn lex_ident_or_keyword(&mut self) -> Token {
        let mut s = String::new();
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' { s.push(c); self.advance(); }
            else { break; }
        }
        match s.as_str() {
            "let"      => Token::Let,
            "mut"      => Token::Mut,
            "fn"       => Token::Fn,
            "return"   => Token::Return,
            "if"       => Token::If,
            "else"     => Token::Else,
            "for"      => Token::For,
            "in"       => Token::In,
            "while"    => Token::While,
            "break"    => Token::Break,
            "continue" => Token::Continue,
            "class"    => Token::Class,
            "mixin"    => Token::Mixin,
            "require"  => Token::Require,
            "is"       => Token::Is,
            "self"     => Token::Self_,
            "super"    => Token::Super,
            "throw"    => Token::Throw,
            "protect"  => Token::Protect,
            "async"    => Token::Async,
            "await"    => Token::Await,
            "throws"   => Token::Throws,
            "export"   => Token::Export,
            "true"     => Token::True,
            "false"    => Token::False,
            "nil"      => Token::Nil,
            _          => Token::Ident(s),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(src: &str) -> Vec<Token> {
        Lexer::new(src).tokenize().unwrap().into_iter().map(|t| t.token).collect()
    }

    #[test]
    fn test_basic_tokens() {
        let tokens = lex("let x = 10");
        assert_eq!(tokens, vec![
            Token::Let, Token::Ident("x".into()), Token::Assign, Token::Number(10.0), Token::Eof
        ]);
    }

    #[test]
    fn test_operators() {
        let tokens = lex("x += 1 != 2 && y || z");
        assert!(tokens.contains(&Token::PlusAssign));
        assert!(tokens.contains(&Token::NotEq));
        assert!(tokens.contains(&Token::And));
        assert!(tokens.contains(&Token::Or));
    }

    #[test]
    fn test_string_plain() {
        let tokens = lex(r#"let s = "hello""#);
        assert!(tokens.contains(&Token::StringLit("hello".into())));
    }

    #[test]
    fn test_string_interpolation() {
        let tokens = lex(r#""hello ${name}!""#);
        match &tokens[0] {
            Token::InterpolatedString(parts) => {
                assert_eq!(parts[0], StringPart::Literal("hello ".into()));
                assert_eq!(parts[1], StringPart::Expr("name".into(), 1, 9));
                assert_eq!(parts[2], StringPart::Literal("!".into()));
            }
            _ => panic!("应该是插值字符串"),
        }
    }

    #[test]
    fn test_keywords() {
        let tokens = lex("class Animal mixin Damageable fn ctor new self");
        assert!(tokens.contains(&Token::Class));
        assert!(tokens.contains(&Token::Mixin));
        assert!(tokens.contains(&Token::Fn));
        assert!(tokens.contains(&Token::New));
        assert!(tokens.contains(&Token::Self_));
    }

    #[test]
    fn test_comment_skip() {
        // 单行注释
        let tokens = lex("let x = 1 // this is a comment\nlet y = 2");
        assert_eq!(tokens.iter().filter(|t| **t == Token::Let).count(), 2);
    }

    #[test]
    fn test_doc_comment_skip() {
        // 文档注释（词法层等同单行注释）
        let tokens = lex("/// doc comment\nlet x = 1");
        assert!(tokens.contains(&Token::Let));
    }

    #[test]
    fn test_block_comment_skip() {
        // 多行注释
        let tokens = lex("let x = /* ignore this */ 42");
        assert_eq!(tokens, vec![
            Token::Let, Token::Ident("x".into()), Token::Assign, Token::Number(42.0), Token::Eof
        ]);
    }

    #[test]
    fn test_multiline_block_comment() {
        let tokens = lex("let x = /*\n  multi\n  line\n*/ 1");
        assert_eq!(tokens, vec![
            Token::Let, Token::Ident("x".into()), Token::Assign, Token::Number(1.0), Token::Eof
        ]);
    }

    #[test]
    fn test_range() {
        let tokens = lex("0..5");
        assert_eq!(tokens, vec![
            Token::Number(0.0), Token::DotDot, Token::Number(5.0), Token::Eof
        ]);
    }
}

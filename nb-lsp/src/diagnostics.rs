use tower_lsp::lsp_types::*;
use nb_core::lexer::{Lexer, LexError};
use nb_core::parser::{Parser, ParseError};

/// 对给定源码进行词法+语法分析，返回所有诊断错误
pub fn get_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    // 词法分析
    let tokens = match Lexer::new(source).tokenize() {
        Ok(tokens) => tokens,
        Err(e) => {
            diagnostics.push(lex_error_to_diagnostic(&e));
            return diagnostics;
        }
    };

    // 语法分析
    if let Err(e) = Parser::new(tokens).parse_program() {
        diagnostics.push(parse_error_to_diagnostic(&e));
    }

    diagnostics
}

fn lex_error_to_diagnostic(e: &LexError) -> Diagnostic {
    let (range, message) = match e {
        LexError::UnknownChar(_, line, col) => {
            let pos = Position::new((*line as u32).saturating_sub(1), (*col as u32).saturating_sub(1));
            (Range::new(pos, Position::new(pos.line, pos.character + 1)), format!("{e}"))
        }
        LexError::UnterminatedString(line) => {
            let pos = Position::new((*line as u32).saturating_sub(1), 0);
            (Range::new(pos, Position::new(pos.line, u32::MAX)), format!("{e}"))
        }
        LexError::InvalidNumber(_, line) => {
            let pos = Position::new((*line as u32).saturating_sub(1), 0);
            (Range::new(pos, Position::new(pos.line, u32::MAX)), format!("{e}"))
        }
    };
    Diagnostic {
        range,
        severity: Some(DiagnosticSeverity::ERROR),
        message,
        source: Some("nb".into()),
        ..Default::default()
    }
}

fn parse_error_to_diagnostic(e: &ParseError) -> Diagnostic {
    let (range, message) = match e {
        ParseError::Unexpected { line, col, .. } => {
            let pos = Position::new((*line as u32).saturating_sub(1), (*col as u32).saturating_sub(1));
            (Range::new(pos, Position::new(pos.line, pos.character + 1)), format!("{e}"))
        }
        ParseError::UnexpectedEof(line) => {
            let pos = Position::new((*line as u32).saturating_sub(1), 0);
            (Range::new(pos, Position::new(pos.line, u32::MAX)), format!("{e}"))
        }
    };
    Diagnostic {
        range,
        severity: Some(DiagnosticSeverity::ERROR),
        message,
        source: Some("nb".into()),
        ..Default::default()
    }
}

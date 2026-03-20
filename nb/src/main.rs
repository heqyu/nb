mod compiler;
mod vm;
mod stdlib;

use std::path::Path;
use nb_core::lexer::Lexer;
use nb_core::parser::Parser;
use vm::Interpreter;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    match args.get(1).map(|s| s.as_str()) {
        Some("run") => {
            let path = args.get(2).unwrap_or_else(|| {
                eprintln!("用法: nb run <文件.nb>");
                std::process::exit(1);
            });
            run_file(path);
        }
        _ => {
            eprintln!("NB 语言解释器 v0.1.0");
            eprintln!("用法: nb run <文件.nb>");
        }
    }
}

fn run_file(path: &str) {
    let source = std::fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("无法读取文件 '{path}': {e}");
        std::process::exit(1);
    });

    // 取文件名（不含扩展名）作为模块名
    let module_name = Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("main");

    // Lex
    let tokens = Lexer::new(&source).tokenize().unwrap_or_else(|e| {
        eprintln!("词法错误: {e}");
        std::process::exit(1);
    });

    // Parse
    let stmts = Parser::new(tokens).parse_program().unwrap_or_else(|e| {
        eprintln!("语法错误: {e}");
        std::process::exit(1);
    });

    // Interpret
    let mut interp = Interpreter::new(module_name);
    if let Err(e) = interp.run(&stmts) {
        eprintln!("运行时错误: {e}");
        std::process::exit(1);
    }
}

use rustpython_parser::{Parse, ast};
use std::fs;

pub struct ParsedProgram {
    pub func_defs: Vec<ast::StmtFunctionDef>,
    pub call_expr: Option<ast::ExprCall>,
}

pub fn parse_file(path: &str) -> ParsedProgram {
    let source = fs::read_to_string(path).expect(&format!("Could not read file: {}", path));

    let ast = ast::Suite::parse(&source, path).expect("Failed to parse Python source");

    // Collect ALL function definitions in order
    let func_defs: Vec<ast::StmtFunctionDef> = ast
        .iter()
        .filter_map(|s| {
            if let ast::Stmt::FunctionDef(f) = s {
                Some(f.clone())
            } else {
                None
            }
        })
        .collect();

    if func_defs.is_empty() {
        panic!("No function definitions found in {}", path);
    }

    // Find the last top-level call expression
    let call_expr = ast
        .iter()
        .filter_map(|s| {
            if let ast::Stmt::Expr(e) = s {
                if let ast::Expr::Call(c) = e.value.as_ref() {
                    return Some(c.clone());
                }
            }
            None
        })
        .last();

    ParsedProgram {
        func_defs,
        call_expr,
    }
}

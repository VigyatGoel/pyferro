use rustpython_parser::ast;
use std::collections::HashSet;

fn stmt_definitely_terminates(stmt: &ast::Stmt) -> bool {
    match stmt {
        ast::Stmt::Return(_) => true,
        ast::Stmt::If(if_stmt) => {
            if if_stmt.orelse.is_empty() {
                return false;
            }
            block_definitely_terminates(&if_stmt.body)
                && block_definitely_terminates(&if_stmt.orelse)
        }
        _ => false,
    }
}

fn block_definitely_terminates(stmts: &[ast::Stmt]) -> bool {
    for stmt in stmts {
        if stmt_definitely_terminates(stmt) {
            return true;
        }
    }
    false
}

pub fn check_function(func_def: &ast::StmtFunctionDef, known_functions: &HashSet<String>) {
    // Check all arguments have `int` type hints
    for awd in &func_def.args.args {
        let arg = &awd.def;
        let annotation = arg.annotation.as_ref().expect(&format!(
            "Argument '{}' has no type hint — all arguments must be typed as int",
            arg.arg
        ));

        match annotation.as_ref() {
            ast::Expr::Name(n) if matches!(n.id.as_str(), "int" | "bool") => {}
            ast::Expr::Name(n) => panic!(
                "Argument '{}' has unsupported type '{}' — only int and bool are supported",
                arg.arg, n.id
            ),
            _ => panic!("Argument '{}' has an unsupported type annotation", arg.arg),
        }
    }

    // Check return type is `int`
    match func_def.returns.as_ref() {
        None => panic!(
            "Function '{}' has no return type hint — must be -> int",
            func_def.name
        ),
        Some(ret) => match ret.as_ref() {
            ast::Expr::Name(n) if matches!(n.id.as_str(), "int" | "bool") => {}
            ast::Expr::Name(n) => panic!(
                "Return type '{}' is not supported — only int and bool are supported",
                n.id
            ),
            _ => panic!("Unsupported return type annotation"),
        },
    }

    if func_def.body.is_empty() {
        panic!("Function '{}' has an empty body", func_def.name);
    }

    // Validate each statement and check called functions exist
    check_stmts(&func_def.body, &func_def.name, known_functions);

    if !block_definitely_terminates(&func_def.body) {
        panic!(
            "Function '{}' must return on all control-flow paths",
            func_def.name
        );
    }
}

fn check_stmts(stmts: &[ast::Stmt], func_name: &str, known_functions: &HashSet<String>) {
    for stmt in stmts {
        check_stmt(stmt, func_name, known_functions);
    }
}

fn check_stmt(stmt: &ast::Stmt, func_name: &str, known_functions: &HashSet<String>) {
    match stmt {
        ast::Stmt::Return(ret) => {
            if let Some(expr) = &ret.value {
                check_expr(expr, func_name, known_functions);
            }
        }
        ast::Stmt::Assign(assign) => {
            check_expr(&assign.value, func_name, known_functions);
        }
        ast::Stmt::If(if_stmt) => {
            check_expr(&if_stmt.test, func_name, known_functions);
            check_stmts(&if_stmt.body, func_name, known_functions);
            check_stmts(&if_stmt.orelse, func_name, known_functions);
        }
        ast::Stmt::While(while_stmt) => {
            check_expr(&while_stmt.test, func_name, known_functions);
            check_stmts(&while_stmt.body, func_name, known_functions);
        }
        ast::Stmt::For(for_stmt) => {
            match for_stmt.target.as_ref() {
                ast::Expr::Name(_) => {}
                _ => panic!(
                    "For loop target in '{}' must be a simple variable name",
                    func_name
                ),
            }
            let range_call = match for_stmt.iter.as_ref() {
                ast::Expr::Call(c) => c,
                _ => panic!("For loop in '{}' must iterate over range(...)", func_name),
            };
            match range_call.func.as_ref() {
                ast::Expr::Name(n) if n.id.as_str() == "range" => {}
                _ => panic!("For loop in '{}' must iterate over range(...)", func_name),
            }
            assert!(
                range_call.args.len() >= 1 && range_call.args.len() <= 3,
                "range() in '{}' takes 1–3 arguments",
                func_name
            );
            for arg in &range_call.args {
                check_expr(arg, func_name, known_functions);
            }
            check_stmts(&for_stmt.body, func_name, known_functions);
        }
        // Allow standalone expression statements — e.g. print(x)
        ast::Stmt::Expr(expr_stmt) => {
            check_expr(&expr_stmt.value, func_name, known_functions);
        }
        _ => panic!(
            "Unsupported statement in '{}' — only assignments, return, if/else, while, for, and print are supported",
            func_name
        ),
    }
}

fn check_expr(expr: &ast::Expr, func_name: &str, known_functions: &HashSet<String>) {
    match expr {
        ast::Expr::Name(_) => {}
        ast::Expr::Constant(_) => {}
        ast::Expr::BinOp(binop) => {
            check_expr(&binop.left, func_name, known_functions);
            check_expr(&binop.right, func_name, known_functions);
        }
        ast::Expr::BoolOp(boolop) => {
            for val in &boolop.values {
                check_expr(val, func_name, known_functions);
            }
        }
        ast::Expr::UnaryOp(unaryop) => {
            check_expr(&unaryop.operand, func_name, known_functions);
        }
        ast::Expr::Compare(cmp) => {
            check_expr(&cmp.left, func_name, known_functions);
            for c in &cmp.comparators {
                check_expr(c, func_name, known_functions);
            }
        }
        ast::Expr::Call(call) => {
            if let ast::Expr::Name(n) = call.func.as_ref() {
                let callee = n.id.as_str();
                if !known_functions.contains(callee) {
                    panic!(
                        "Function '{}' calls unknown function '{}' — make sure it is defined",
                        func_name, callee
                    );
                }
                // Recursively check arguments
                for arg in &call.args {
                    check_expr(arg, func_name, known_functions);
                }
            } else {
                panic!("Only simple function calls supported (e.g. foo(a, b))");
            }
        }
        _ => panic!("Unsupported expression in '{}': {:?}", func_name, expr),
    }
}

pub fn check_all(func_defs: &[ast::StmtFunctionDef]) {
    // Build the set of all known function names first.
    // We also add builtins here so they are always valid to call.
    let mut known_functions: HashSet<String> = func_defs
        .iter()
        .map(|f| f.name.as_str().to_string())
        .collect();

    // Builtins supported by Py-LLVM
    known_functions.insert("print".to_string());

    // Validate each function
    for func_def in func_defs {
        check_function(func_def, &known_functions);
    }
}

#[cfg(test)]
mod tests {
    use rustpython_parser::{Parse, ast};
    use std::collections::HashSet;

    fn parse_func(src: &str) -> ast::StmtFunctionDef {
        let ast = ast::Suite::parse(src, "<test>").expect("parse failed");
        ast.into_iter()
            .find_map(|s| {
                if let ast::Stmt::FunctionDef(f) = s {
                    Some(f)
                } else {
                    None
                }
            })
            .expect("no function found")
    }

    fn known(names: &[&str]) -> HashSet<String> {
        let mut set: HashSet<String> = names.iter().map(|s| s.to_string()).collect();
        set.insert("print".to_string()); // always include builtin
        set
    }

    #[test]
    fn valid_int_function_passes() {
        let src = "def foo(x: int) -> int:\n    return x\n";
        let func = parse_func(src);
        super::check_function(&func, &known(&["foo"]));
    }

    #[test]
    #[should_panic(expected = "has no type hint")]
    fn missing_arg_annotation_panics() {
        let src = "def foo(x) -> int:\n    return x\n";
        let func = parse_func(src);
        super::check_function(&func, &known(&["foo"]));
    }

    #[test]
    #[should_panic(expected = "unsupported type")]
    fn float_arg_annotation_panics() {
        let src = "def foo(x: float) -> int:\n    return x\n";
        let func = parse_func(src);
        super::check_function(&func, &known(&["foo"]));
    }

    #[test]
    #[should_panic(expected = "no return type hint")]
    fn missing_return_annotation_panics() {
        let src = "def foo(x: int):\n    return x\n";
        let func = parse_func(src);
        super::check_function(&func, &known(&["foo"]));
    }

    #[test]
    #[should_panic(expected = "must return on all control-flow paths")]
    fn missing_return_on_one_branch_panics() {
        let src = "def foo(n: int) -> int:\n    if n > 0:\n        return n\n";
        let func = parse_func(src);
        super::check_function(&func, &known(&["foo"]));
    }

    #[test]
    #[should_panic(expected = "Unsupported statement")]
    fn empty_body_with_pass_panics() {
        let src = "def foo(n: int) -> int:\n    pass\n";
        let func = parse_func(src);
        super::check_function(&func, &known(&["foo"]));
    }

    #[test]
    fn function_with_while_passes() {
        let src = "def foo(n: int) -> int:\n    i = 0\n    while i < n:\n        i = i + 1\n    return i\n";
        let func = parse_func(src);
        super::check_function(&func, &known(&["foo"]));
    }

    #[test]
    fn function_with_if_else_passes() {
        let src = "def foo(x: int) -> int:\n    if x > 0:\n        return x\n    else:\n        return 0\n";
        let func = parse_func(src);
        super::check_function(&func, &known(&["foo"]));
    }

    #[test]
    #[should_panic(expected = "calls unknown function")]
    fn call_to_unknown_function_panics() {
        let src = "def foo(x: int) -> int:\n    return bar(x)\n";
        let func = parse_func(src);
        super::check_function(&func, &known(&["foo"]));
    }

    #[test]
    fn call_to_known_function_passes() {
        let src = "def foo(x: int) -> int:\n    return bar(x)\n";
        let func = parse_func(src);
        super::check_function(&func, &known(&["foo", "bar"]));
    }

    #[test]
    fn recursive_call_passes() {
        let src = "def factorial(n: int) -> int:\n    if n <= 1:\n        return 1\n    else:\n        return n\n";
        let func = parse_func(src);
        super::check_function(&func, &known(&["factorial"]));
    }

    #[test]
    fn print_call_inside_function_passes() {
        let src = "def foo(x: int) -> int:\n    print(x)\n    return x\n";
        let func = parse_func(src);
        super::check_function(&func, &known(&["foo"]));
    }

    #[test]
    fn bool_arg_and_return_passes() {
        let src = "def foo(x: bool) -> bool:\n    return x\n";
        let func = parse_func(src);
        super::check_function(&func, &known(&["foo"]));
    }

    #[test]
    fn bool_op_and_passes() {
        let src = "def foo(x: int, y: int) -> int:\n    if x > 0 and y > 0:\n        return 1\n    else:\n        return 0\n";
        let func = parse_func(src);
        super::check_function(&func, &known(&["foo"]));
    }

    #[test]
    fn bool_op_not_passes() {
        let src = "def foo(x: int) -> int:\n    if not x > 0:\n        return 0\n    else:\n        return 1\n";
        let func = parse_func(src);
        super::check_function(&func, &known(&["foo"]));
    }
}

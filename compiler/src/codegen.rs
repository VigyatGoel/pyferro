use inkwell::{FloatPredicate, IntPredicate};
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::values::{BasicValue, BasicValueEnum, FunctionValue, PointerValue};
use rustpython_parser::ast;
use std::collections::HashMap;

pub struct Scope<'ctx> {
    pub vars: HashMap<String, BasicValueEnum<'ctx>>,
    pub locals: HashMap<String, PointerValue<'ctx>>,
}

impl<'ctx> Scope<'ctx> {
    pub fn new() -> Self {
        Scope {
            vars: HashMap::new(),
            locals: HashMap::new(),
        }
    }

    pub fn get(
        &self,
        name: &str,
        builder: &Builder<'ctx>,
        ctx: &'ctx Context,
    ) -> BasicValueEnum<'ctx> {
        if let Some(ptr) = self.locals.get(name) {
            // Determine the stored type from the alloca's element type.
            // bool vars are stored as i1, int vars as i64.
            let alloca = ptr.as_instruction_value();
            let load_type = alloca
                .and_then(|i| i.get_allocated_type().ok())
                .unwrap_or(ctx.i64_type().into());
            builder.build_load(load_type, *ptr, name).unwrap()
        } else if let Some(val) = self.vars.get(name) {
            *val
        } else {
            panic!("Unknown variable: {}", name)
        }
    }
}

fn current_block_terminated<'ctx>(builder: &Builder<'ctx>) -> bool {
    builder
        .get_insert_block()
        .and_then(|block| block.get_terminator())
        .is_some()
}

fn compile_stmt_list<'ctx>(
    stmts: &[ast::Stmt],
    builder: &Builder<'ctx>,
    scope: &mut Scope<'ctx>,
    ctx: &'ctx Context,
    func: FunctionValue<'ctx>,
    module: &Module<'ctx>,
) -> bool {
    for stmt in stmts {
        if current_block_terminated(builder) {
            return true;
        }
        if compile_stmt(stmt, builder, scope, ctx, func, module) {
            return true;
        }
    }
    current_block_terminated(builder)
}

pub fn compile_expr<'ctx>(
    expr: &ast::Expr,
    builder: &Builder<'ctx>,
    scope: &Scope<'ctx>,
    ctx: &'ctx Context,
    module: &Module<'ctx>,
) -> BasicValueEnum<'ctx> {
    match expr {
        ast::Expr::Name(name_expr) => scope.get(name_expr.id.as_str(), builder, ctx),
        ast::Expr::Constant(c) => {
            match &c.value {
                ast::Constant::Int(n) => {
                    let val: i64 = n.to_string().parse().expect("Integer too large");
                    ctx.i64_type().const_int(val as u64, true).into()
                }
                ast::Constant::Bool(b) => {
                    // True → i1 1, False → i1 0
                    ctx.bool_type()
                        .const_int(if *b { 1 } else { 0 }, false)
                        .into()
                }
                ast::Constant::Float(f) => ctx.f64_type().const_float(*f).into(),
                _ => panic!("Only integer, bool, and float constants supported"),
            }
        }
        ast::Expr::BinOp(binop) => {
            let lhs = compile_expr(&binop.left, builder, scope, ctx, module);
            let rhs = compile_expr(&binop.right, builder, scope, ctx, module);
            match (lhs, rhs) {
                (BasicValueEnum::IntValue(l), BasicValueEnum::IntValue(r)) => match binop.op {
                    ast::Operator::Add => builder.build_int_add(l, r, "addtmp").unwrap().into(),
                    ast::Operator::Sub => builder.build_int_sub(l, r, "subtmp").unwrap().into(),
                    ast::Operator::Mult => builder.build_int_mul(l, r, "multmp").unwrap().into(),
                    ast::Operator::Div => {
                        builder.build_int_signed_div(l, r, "divtmp").unwrap().into()
                    }
                    _ => panic!("Unsupported operator"),
                },
                (BasicValueEnum::FloatValue(l), BasicValueEnum::FloatValue(r)) => match binop.op {
                    ast::Operator::Add => builder.build_float_add(l, r, "faddtmp").unwrap().into(),
                    ast::Operator::Sub => builder.build_float_sub(l, r, "fsubtmp").unwrap().into(),
                    ast::Operator::Mult => {
                        builder.build_float_mul(l, r, "fmultmp").unwrap().into()
                    }
                    ast::Operator::Div => {
                        builder.build_float_div(l, r, "fdivtmp").unwrap().into()
                    }
                    _ => panic!("Unsupported operator for float"),
                },
                _ => panic!("Type mismatch: cannot mix int and float in binary operation"),
            }
        }
        ast::Expr::BoolOp(boolop) => {
            // and/or — fold left-to-right: (a and b and c) → (a and (b and c))
            // All operands are coerced to i1 via icmp ne 0 (int) or fcmp one 0.0 (float).
            let to_i1 = |val: BasicValueEnum<'ctx>| -> inkwell::values::IntValue<'ctx> {
                match val {
                    BasicValueEnum::IntValue(iv) => {
                        if iv.get_type().get_bit_width() == 1 {
                            iv
                        } else {
                            builder
                                .build_int_compare(
                                    IntPredicate::NE,
                                    iv,
                                    ctx.i64_type().const_int(0, false),
                                    "bool_coerce",
                                )
                                .unwrap()
                        }
                    }
                    BasicValueEnum::FloatValue(fv) => builder
                        .build_float_compare(
                            FloatPredicate::ONE,
                            fv,
                            ctx.f64_type().const_float(0.0),
                            "bool_coerce",
                        )
                        .unwrap(),
                    _ => panic!("Cannot coerce value to bool"),
                }
            };

            let first = compile_expr(&boolop.values[0], builder, scope, ctx, module);
            let mut acc = to_i1(first);
            for operand in &boolop.values[1..] {
                let rhs = compile_expr(operand, builder, scope, ctx, module);
                let rhs_i1 = to_i1(rhs);
                acc = match boolop.op {
                    ast::BoolOp::And => builder.build_and(acc, rhs_i1, "andtmp").unwrap(),
                    ast::BoolOp::Or => builder.build_or(acc, rhs_i1, "ortmp").unwrap(),
                };
            }
            acc.into()
        }
        ast::Expr::UnaryOp(unaryop) => {
            let operand = compile_expr(&unaryop.operand, builder, scope, ctx, module);
            match unaryop.op {
                ast::UnaryOp::Not => {
                    let as_i1 = match operand {
                        BasicValueEnum::IntValue(iv) => {
                            if iv.get_type().get_bit_width() == 1 {
                                iv
                            } else {
                                builder
                                    .build_int_compare(
                                        IntPredicate::NE,
                                        iv,
                                        ctx.i64_type().const_int(0, false),
                                        "bool_coerce",
                                    )
                                    .unwrap()
                            }
                        }
                        BasicValueEnum::FloatValue(fv) => builder
                            .build_float_compare(
                                FloatPredicate::ONE,
                                fv,
                                ctx.f64_type().const_float(0.0),
                                "bool_coerce",
                            )
                            .unwrap(),
                        _ => panic!("Cannot apply 'not' to this type"),
                    };
                    builder.build_not(as_i1, "nottmp").unwrap().into()
                }
                ast::UnaryOp::USub => match operand {
                    BasicValueEnum::IntValue(iv) => {
                        builder.build_int_neg(iv, "negtmp").unwrap().into()
                    }
                    BasicValueEnum::FloatValue(fv) => {
                        builder.build_float_neg(fv, "fnegtmp").unwrap().into()
                    }
                    _ => panic!("Unsupported type for unary negation"),
                },
                ast::UnaryOp::UAdd => operand,
                _ => panic!("Unsupported unary operator: {:?}", unaryop.op),
            }
        }
        ast::Expr::Compare(cmp) => {
            assert!(
                cmp.ops.len() == 1 && cmp.comparators.len() == 1,
                "Only simple comparisons supported"
            );
            let lhs = compile_expr(&cmp.left, builder, scope, ctx, module);
            let rhs = compile_expr(&cmp.comparators[0], builder, scope, ctx, module);
            match (lhs, rhs) {
                (BasicValueEnum::IntValue(l), BasicValueEnum::IntValue(r)) => {
                    let predicate = match cmp.ops[0] {
                        ast::CmpOp::Gt => IntPredicate::SGT,
                        ast::CmpOp::Lt => IntPredicate::SLT,
                        ast::CmpOp::GtE => IntPredicate::SGE,
                        ast::CmpOp::LtE => IntPredicate::SLE,
                        ast::CmpOp::Eq => IntPredicate::EQ,
                        ast::CmpOp::NotEq => IntPredicate::NE,
                        _ => panic!("Unsupported comparison operator"),
                    };
                    builder.build_int_compare(predicate, l, r, "cmptmp").unwrap().into()
                }
                (BasicValueEnum::FloatValue(l), BasicValueEnum::FloatValue(r)) => {
                    let predicate = match cmp.ops[0] {
                        ast::CmpOp::Gt => FloatPredicate::OGT,
                        ast::CmpOp::Lt => FloatPredicate::OLT,
                        ast::CmpOp::GtE => FloatPredicate::OGE,
                        ast::CmpOp::LtE => FloatPredicate::OLE,
                        ast::CmpOp::Eq => FloatPredicate::OEQ,
                        ast::CmpOp::NotEq => FloatPredicate::ONE,
                        _ => panic!("Unsupported float comparison operator"),
                    };
                    builder.build_float_compare(predicate, l, r, "fcmptmp").unwrap().into()
                }
                _ => panic!("Type mismatch in comparison: cannot compare int and float"),
            }
        }
        ast::Expr::Call(call) => {
            let callee_name = if let ast::Expr::Name(n) = call.func.as_ref() {
                n.id.as_str()
            } else {
                panic!("Only simple function calls supported");
            };

            // Handle print() as a builtin — emit a printf call directly
            if callee_name == "print" {
                return emit_print(call, builder, scope, ctx, module);
            }

            // Look up the function in the LLVM module by name
            let callee_fn = module.get_function(callee_name).expect(&format!(
                "Function '{}' not found in module — make sure it is defined before use",
                callee_name
            ));

            // Compile each argument expression
            let call_args: Vec<_> = call
                .args
                .iter()
                .map(|arg| compile_expr(arg, builder, scope, ctx, module).into())
                .collect();

            // Emit the call instruction; void functions have no return value
            let call_site = builder
                .build_call(callee_fn, &call_args, "calltmp")
                .unwrap();
            match call_site.try_as_basic_value() {
                inkwell::values::ValueKind::Basic(v) => v,
                inkwell::values::ValueKind::Instruction(_) => ctx.i64_type().const_zero().into(),
            }
        }
        _ => panic!("Unsupported expression: {:?}", expr),
    }
}

fn emit_print<'ctx>(
    call: &ast::ExprCall,
    builder: &Builder<'ctx>,
    scope: &Scope<'ctx>,
    ctx: &'ctx Context,
    module: &Module<'ctx>,
) -> BasicValueEnum<'ctx> {
    assert!(
        call.args.len() == 1,
        "print() currently supports exactly one argument"
    );

    let printf_fn = match module.get_function("printf") {
        Some(f) => f,
        None => {
            let ptr_type = ctx.ptr_type(inkwell::AddressSpace::default());
            let printf_type = ctx.i32_type().fn_type(&[ptr_type.into()], true);
            module.add_function("printf", printf_type, None)
        }
    };

    let val = compile_expr(&call.args[0], builder, scope, ctx, module);

    match val {
        BasicValueEnum::IntValue(iv) if iv.get_type().get_bit_width() == 1 => {
            // Bool: print "true" or "false"
            let true_str = builder
                .build_global_string_ptr("true\n", "true_str")
                .unwrap();
            let false_str = builder
                .build_global_string_ptr("false\n", "false_str")
                .unwrap();
            let fmt_ptr = builder
                .build_select(
                    iv,
                    true_str.as_pointer_value(),
                    false_str.as_pointer_value(),
                    "bool_fmt",
                )
                .unwrap();
            builder
                .build_call(printf_fn, &[fmt_ptr.into()], "print_call")
                .unwrap();
        }
        BasicValueEnum::IntValue(iv) => {
            // Int: use %lld
            let fmt_str = builder
                .build_global_string_ptr("%lld\n", "print_fmt")
                .unwrap();
            builder
                .build_call(printf_fn, &[fmt_str.as_pointer_value().into(), iv.into()], "print_call")
                .unwrap();
        }
        BasicValueEnum::FloatValue(fv) => {
            // Float: use %f
            let fmt_str = builder
                .build_global_string_ptr("%f\n", "float_fmt")
                .unwrap();
            builder
                .build_call(printf_fn, &[fmt_str.as_pointer_value().into(), fv.into()], "print_call")
                .unwrap();
        }
        _ => panic!("Unsupported type for print()"),
    }

    ctx.i64_type().const_int(0, false).into()
}

pub fn compile_stmt<'ctx>(
    stmt: &ast::Stmt,
    builder: &Builder<'ctx>,
    scope: &mut Scope<'ctx>,
    ctx: &'ctx Context,
    func: FunctionValue<'ctx>,
    module: &Module<'ctx>,
) -> bool {
    match stmt {
        ast::Stmt::Return(ret) => {
            let is_none_value = match &ret.value {
                None => true,
                Some(v) => matches!(v.as_ref(), ast::Expr::Constant(c) if matches!(c.value, ast::Constant::None)),
            };
            if is_none_value {
                builder.build_return(None).unwrap();
            } else {
                let expr = ret.value.as_ref().unwrap();
                let val = compile_expr(expr, builder, scope, ctx, module);
                builder.build_return(Some(&val)).unwrap();
            }
            true
        }
        ast::Stmt::Assign(assign) => {
            compile_assign(assign, builder, scope, ctx, module);
            false
        }
        ast::Stmt::If(if_stmt) => compile_if(if_stmt, builder, scope, ctx, func, module),
        ast::Stmt::While(while_stmt) => {
            compile_while(while_stmt, builder, scope, ctx, func, module)
        }
        ast::Stmt::For(for_stmt) => compile_for(for_stmt, builder, scope, ctx, func, module),
        // Standalone expression statement — e.g. print(x)
        // Compile the expression and discard the result.
        ast::Stmt::Expr(expr_stmt) => {
            compile_expr(&expr_stmt.value, builder, scope, ctx, module);
            false // print() does not terminate the block
        }
        _ => panic!("Unsupported statement: {:?}", stmt),
    }
}

fn compile_assign<'ctx>(
    assign: &ast::StmtAssign,
    builder: &Builder<'ctx>,
    scope: &mut Scope<'ctx>,
    ctx: &'ctx Context,
    module: &Module<'ctx>,
) {
    assert!(
        assign.targets.len() == 1,
        "Only single assignment targets supported"
    );

    let target = &assign.targets[0];
    let name = if let ast::Expr::Name(n) = target {
        n.id.as_str().to_string()
    } else {
        panic!("Only simple variable assignment supported");
    };

    // STEP 1: If this name is a function parameter (in vars), migrate it to
    // locals BEFORE compiling the right-hand side expression.
    if scope.vars.contains_key(&name) && !scope.locals.contains_key(&name) {
        let original_val = scope.vars[&name];
        let entry_block = builder
            .get_insert_block()
            .unwrap()
            .get_parent()
            .unwrap()
            .get_first_basic_block()
            .unwrap();
        let current_block = builder.get_insert_block().unwrap();

        match entry_block.get_first_instruction() {
            Some(first_instr) => builder.position_before(&first_instr),
            None => builder.position_at_end(entry_block),
        }

        let ptr = match original_val {
            BasicValueEnum::IntValue(iv) => builder.build_alloca(iv.get_type(), &name).unwrap(),
            BasicValueEnum::FloatValue(fv) => builder.build_alloca(fv.get_type(), &name).unwrap(),
            _ => panic!("Unsupported type for param migration"),
        };
        builder.build_store(ptr, original_val).unwrap();
        builder.position_at_end(current_block);

        scope.locals.insert(name.clone(), ptr);
        scope.vars.remove(&name);
    }

    // STEP 2: Compile the RHS
    let val = compile_expr(&assign.value, builder, scope, ctx, module);

    // STEP 3: Get or create the stack slot and store the new value
    let ptr = if let Some(existing_ptr) = scope.locals.get(&name) {
        *existing_ptr
    } else {
        let entry_block = builder
            .get_insert_block()
            .unwrap()
            .get_parent()
            .unwrap()
            .get_first_basic_block()
            .unwrap();

        let current_block = builder.get_insert_block().unwrap();

        match entry_block.get_first_instruction() {
            Some(first_instr) => builder.position_before(&first_instr),
            None => builder.position_at_end(entry_block),
        }

        // Alloca with the actual type of the RHS (i1 for bool, i64 for int, f64 for float)
        let ptr = match val {
            BasicValueEnum::IntValue(iv) => builder.build_alloca(iv.get_type(), &name).unwrap(),
            BasicValueEnum::FloatValue(fv) => builder.build_alloca(fv.get_type(), &name).unwrap(),
            _ => panic!("Unsupported type for variable assignment"),
        };
        builder.position_at_end(current_block);
        scope.locals.insert(name.clone(), ptr);
        ptr
    };

    builder.build_store(ptr, val).unwrap();
}

fn compile_if<'ctx>(
    if_stmt: &ast::StmtIf,
    builder: &Builder<'ctx>,
    scope: &mut Scope<'ctx>,
    ctx: &'ctx Context,
    func: FunctionValue<'ctx>,
    module: &Module<'ctx>,
) -> bool {
    let condition = compile_expr(&if_stmt.test, builder, scope, ctx, module).into_int_value();

    let then_block = ctx.append_basic_block(func, "then");
    let else_block = ctx.append_basic_block(func, "else");
    let merge_block = ctx.append_basic_block(func, "if_cont");

    builder
        .build_conditional_branch(condition, then_block, else_block)
        .unwrap();

    builder.position_at_end(then_block);
    let then_terminated = compile_stmt_list(&if_stmt.body, builder, scope, ctx, func, module);
    if !then_terminated && !current_block_terminated(builder) {
        builder.build_unconditional_branch(merge_block).unwrap();
    }

    builder.position_at_end(else_block);
    let else_terminated = compile_stmt_list(&if_stmt.orelse, builder, scope, ctx, func, module);
    if !else_terminated && !current_block_terminated(builder) {
        builder.build_unconditional_branch(merge_block).unwrap();
    }

    builder.position_at_end(merge_block);
    let both_terminated = then_terminated && else_terminated;
    if both_terminated {
        builder.build_unreachable().unwrap();
    }
    both_terminated
}

fn collect_assigned_names(stmts: &[ast::Stmt]) -> Vec<String> {
    let mut names = Vec::new();
    for stmt in stmts {
        match stmt {
            ast::Stmt::Assign(a) => {
                for target in &a.targets {
                    if let ast::Expr::Name(n) = target {
                        names.push(n.id.as_str().to_string());
                    }
                }
            }
            ast::Stmt::While(w) => names.extend(collect_assigned_names(&w.body)),
            ast::Stmt::If(i) => {
                names.extend(collect_assigned_names(&i.body));
                names.extend(collect_assigned_names(&i.orelse));
            }
            ast::Stmt::For(f) => {
                if let ast::Expr::Name(n) = f.target.as_ref() {
                    names.push(n.id.as_str().to_string());
                }
                names.extend(collect_assigned_names(&f.body));
            }
            _ => {}
        }
    }
    names
}

fn compile_while<'ctx>(
    while_stmt: &ast::StmtWhile,
    builder: &Builder<'ctx>,
    scope: &mut Scope<'ctx>,
    ctx: &'ctx Context,
    func: FunctionValue<'ctx>,
    module: &Module<'ctx>,
) -> bool {
    // Pre-migrate any vars assigned inside the loop body to stack slots so
    // the loop header condition generates a `load` (re-read each iteration)
    // instead of using the frozen SSA parameter value.
    let assigned = collect_assigned_names(&while_stmt.body);
    let entry_block = builder
        .get_insert_block()
        .unwrap()
        .get_parent()
        .unwrap()
        .get_first_basic_block()
        .unwrap();
    let current_block = builder.get_insert_block().unwrap();
    for name in &assigned {
        if scope.vars.contains_key(name) && !scope.locals.contains_key(name) {
            let original_val = scope.vars[name];
            match entry_block.get_first_instruction() {
                Some(first_instr) => builder.position_before(&first_instr),
                None => builder.position_at_end(entry_block),
            }
            let ptr = match original_val {
                BasicValueEnum::IntValue(iv) => builder.build_alloca(iv.get_type(), name).unwrap(),
                BasicValueEnum::FloatValue(fv) => {
                    builder.build_alloca(fv.get_type(), name).unwrap()
                }
                _ => panic!("Unsupported type for while-loop var migration"),
            };
            builder.build_store(ptr, original_val).unwrap();
            builder.position_at_end(current_block);
            scope.locals.insert(name.clone(), ptr);
            scope.vars.remove(name);
        }
    }

    let loop_header = ctx.append_basic_block(func, "loop_header");
    let loop_body = ctx.append_basic_block(func, "loop_body");
    let loop_exit = ctx.append_basic_block(func, "loop_exit");

    builder.build_unconditional_branch(loop_header).unwrap();

    builder.position_at_end(loop_header);
    let condition = compile_expr(&while_stmt.test, builder, scope, ctx, module).into_int_value();
    builder
        .build_conditional_branch(condition, loop_body, loop_exit)
        .unwrap();

    builder.position_at_end(loop_body);
    let body_terminated = compile_stmt_list(&while_stmt.body, builder, scope, ctx, func, module);
    if !body_terminated && !current_block_terminated(builder) {
        builder.build_unconditional_branch(loop_header).unwrap();
    }

    builder.position_at_end(loop_exit);
    false
}

fn compile_for<'ctx>(
    for_stmt: &ast::StmtFor,
    builder: &Builder<'ctx>,
    scope: &mut Scope<'ctx>,
    ctx: &'ctx Context,
    func: FunctionValue<'ctx>,
    module: &Module<'ctx>,
) -> bool {
    // Decode range(stop) / range(start,stop) / range(start,stop,step)
    let range_call = if let ast::Expr::Call(c) = for_stmt.iter.as_ref() {
        c
    } else {
        panic!("for loop iter must be range(...)");
    };

    let loop_var_name = if let ast::Expr::Name(n) = for_stmt.target.as_ref() {
        n.id.as_str().to_string()
    } else {
        panic!("for loop target must be a simple name");
    };

    // Pre-migrate any vars assigned inside the loop body so the header re-reads
    // them from memory each iteration (same pattern as compile_while).
    let assigned = collect_assigned_names(&for_stmt.body);
    let entry_block = builder
        .get_insert_block()
        .unwrap()
        .get_parent()
        .unwrap()
        .get_first_basic_block()
        .unwrap();
    let current_block = builder.get_insert_block().unwrap();
    for name in &assigned {
        if scope.vars.contains_key(name) && !scope.locals.contains_key(name) {
            let original_val = scope.vars[name];
            match entry_block.get_first_instruction() {
                Some(first_instr) => builder.position_before(&first_instr),
                None => builder.position_at_end(entry_block),
            }
            let ptr = match original_val {
                BasicValueEnum::IntValue(iv) => builder.build_alloca(iv.get_type(), name).unwrap(),
                BasicValueEnum::FloatValue(fv) => {
                    builder.build_alloca(fv.get_type(), name).unwrap()
                }
                _ => panic!("Unsupported type for for-loop var migration"),
            };
            builder.build_store(ptr, original_val).unwrap();
            builder.position_at_end(current_block);
            scope.locals.insert(name.clone(), ptr);
            scope.vars.remove(name);
        }
    }

    // Compute range bounds — emit LLVM values directly to avoid fake AST nodes.
    let i64_t = ctx.i64_type();
    let start_val = match range_call.args.len() {
        1 => i64_t.const_int(0, false),
        _ => compile_expr(&range_call.args[0], builder, scope, ctx, module).into_int_value(),
    };
    let stop_val = {
        let stop_idx = if range_call.args.len() == 1 { 0 } else { 1 };
        compile_expr(&range_call.args[stop_idx], builder, scope, ctx, module).into_int_value()
    };
    let step_val = if range_call.args.len() == 3 {
        compile_expr(&range_call.args[2], builder, scope, ctx, module).into_int_value()
    } else {
        i64_t.const_int(1, false)
    };

    let loop_var_ptr = {
        let cur = builder.get_insert_block().unwrap();
        match entry_block.get_first_instruction() {
            Some(first_instr) => builder.position_before(&first_instr),
            None => builder.position_at_end(entry_block),
        }
        let ptr = builder
            .build_alloca(ctx.i64_type(), &loop_var_name)
            .unwrap();
        builder.position_at_end(cur);
        scope.locals.insert(loop_var_name.clone(), ptr);
        ptr
    };
    builder.build_store(loop_var_ptr, start_val).unwrap();

    let loop_header = ctx.append_basic_block(func, "for_header");
    let loop_body = ctx.append_basic_block(func, "for_body");
    let loop_exit = ctx.append_basic_block(func, "for_exit");

    builder.build_unconditional_branch(loop_header).unwrap();

    // Header: test i < stop
    builder.position_at_end(loop_header);
    let i_cur = builder
        .build_load(ctx.i64_type(), loop_var_ptr, &loop_var_name)
        .unwrap()
        .into_int_value();
    let cond = builder
        .build_int_compare(IntPredicate::SLT, i_cur, stop_val, "for_cond")
        .unwrap();
    builder
        .build_conditional_branch(cond, loop_body, loop_exit)
        .unwrap();

    // Body: execute stmts, then increment i
    builder.position_at_end(loop_body);
    let body_terminated = compile_stmt_list(&for_stmt.body, builder, scope, ctx, func, module);
    if !body_terminated && !current_block_terminated(builder) {
        let i_before_inc = builder
            .build_load(ctx.i64_type(), loop_var_ptr, &loop_var_name)
            .unwrap()
            .into_int_value();
        let i_next = builder
            .build_int_add(i_before_inc, step_val, "for_inc")
            .unwrap();
        builder.build_store(loop_var_ptr, i_next).unwrap();
        builder.build_unconditional_branch(loop_header).unwrap();
    }

    builder.position_at_end(loop_exit);
    false
}

pub fn build_all_functions<'ctx>(
    func_defs: &[ast::StmtFunctionDef],
    context: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
) -> Vec<FunctionValue<'ctx>> {
    // Declare printf upfront so it's available anywhere in the module,
    // including inside functions that call print()
    let ptr_type = context.ptr_type(inkwell::AddressSpace::default());
    let printf_type = context.i32_type().fn_type(&[ptr_type.into()], true);
    module.add_function("printf", printf_type, None);

    // First pass: register all function signatures
    let i64_type = context.i64_type();
    let i1_type = context.bool_type();
    let f64_type = context.f64_type();

    let py_type_to_llvm = |annotation: &ast::Expr| -> inkwell::types::BasicTypeEnum<'ctx> {
        if let ast::Expr::Name(n) = annotation {
            match n.id.as_str() {
                "bool" => i1_type.into(),
                "float" => f64_type.into(),
                _ => i64_type.into(),
            }
        } else {
            i64_type.into()
        }
    };

    for func_def in func_defs {
        let arg_types: Vec<inkwell::types::BasicMetadataTypeEnum> = func_def
            .args
            .args
            .iter()
            .map(|awd| {
                awd.def
                    .annotation
                    .as_ref()
                    .map(|a| py_type_to_llvm(a).into())
                    .unwrap_or(i64_type.into())
            })
            .collect();

        let is_void = matches!(
            func_def.returns.as_deref(),
            Some(ast::Expr::Constant(c)) if matches!(c.value, ast::Constant::None)
        );

        let fn_type = if is_void {
            context.void_type().fn_type(&arg_types, false)
        } else {
            let ret_type = func_def
                .returns
                .as_ref()
                .map(|r| py_type_to_llvm(r))
                .unwrap_or(i64_type.into());
            match ret_type {
                inkwell::types::BasicTypeEnum::IntType(it) => it.fn_type(&arg_types, false),
                inkwell::types::BasicTypeEnum::FloatType(ft) => ft.fn_type(&arg_types, false),
                _ => i64_type.fn_type(&arg_types, false),
            }
        };
        module.add_function(func_def.name.as_str(), fn_type, None);
    }

    // Second pass: compile each function body
    let mut llvm_fns = Vec::new();
    for func_def in func_defs {
        let llvm_fn = build_python_function(func_def, context, module, builder);
        llvm_fns.push(llvm_fn);
    }

    llvm_fns
}

pub fn build_python_function<'ctx>(
    func_def: &ast::StmtFunctionDef,
    context: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
) -> FunctionValue<'ctx> {
    let llvm_fn = module.get_function(func_def.name.as_str()).expect(&format!(
        "Function '{}' was not pre-registered",
        func_def.name
    ));

    let entry = context.append_basic_block(llvm_fn, "entry");
    builder.position_at_end(entry);

    let mut scope = Scope::new();
    for (i, awd) in func_def.args.args.iter().enumerate() {
        let name = awd.def.arg.as_str().to_string();
        let p = llvm_fn.get_nth_param(i as u32).unwrap();
        p.set_name(&name);
        scope.vars.insert(name, p);
    }

    let mut terminated = false;
    for stmt in &func_def.body {
        if compile_stmt(stmt, builder, &mut scope, context, llvm_fn, module) {
            terminated = true;
            break;
        }
    }

    // Void functions get an implicit return if no explicit terminator was emitted
    if !terminated {
        let is_void = matches!(
            func_def.returns.as_deref(),
            Some(ast::Expr::Constant(c)) if matches!(c.value, ast::Constant::None)
        );
        if is_void {
            builder.build_return(None).unwrap();
        }
    }

    llvm_fn
}

pub fn build_main<'ctx>(
    call_expr: &ast::ExprCall,
    context: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
) {
    let main_type = context.i32_type().fn_type(&[], false);
    let main_fn = module.add_function("main", main_type, None);
    let main_entry = context.append_basic_block(main_fn, "entry");
    builder.position_at_end(main_entry);

    let scope = Scope::new();
    compile_expr(
        &ast::Expr::Call(call_expr.clone()),
        builder,
        &scope,
        context,
        module,
    );

    builder
        .build_return(Some(&context.i32_type().const_int(0, false)))
        .unwrap();
}

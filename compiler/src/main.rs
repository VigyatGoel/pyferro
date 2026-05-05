use compiler::backend;
use compiler::codegen;
use compiler::parser;
use compiler::semantic;

use clap::Parser;
use inkwell::context::Context;

#[derive(Parser, Debug)]
#[command(name = "pyferro", version, about)]
struct Args {
    input: String,

    #[arg(short, long)]
    output: Option<String>,

    #[arg(long, default_value_t = false)]
    emit_ir: bool,
}

fn main() {
    let args = Args::parse();

    let output_name = args.output.unwrap_or_else(|| {
        std::path::Path::new(&args.input)
            .file_stem()
            .expect("Could not derive output name from input filename")
            .to_string_lossy()
            .into_owned()
    });

    // Parse
    let program = parser::parse_file(&args.input);

    // Semantic analysis — validate all functions
    semantic::check_all(&program.func_defs);

    // LLVM setup
    let context = Context::create();
    let module = context.create_module("pyferro");
    let builder = context.create_builder();

    // Compile all functions (two-pass: register signatures, then compile bodies)
    codegen::build_all_functions(&program.func_defs, &context, &module, &builder);

    // Emit main() if there is a top-level call
    let has_call = program.call_expr.is_some();
    if let Some(call_expr) = &program.call_expr {
        codegen::build_main(call_expr, &context, &module, &builder);
    }

    let machine = backend::create_target_machine();
    backend::run_optimization_passes(&module, &machine);

    if args.emit_ir {
        let ir_path = format!("{}.ll", output_name);
        module
            .print_to_file(&ir_path)
            .expect("Failed to write LLVM IR file");
        println!("IR written to: {}", ir_path);
        return;
    }

    backend::emit_object(&module, &machine, &output_name);

    if has_call {
        backend::link_executable(&output_name);
    } else {
        println!("No top-level call — skipping linking.");
        println!("{}.o is ready to link externally", output_name);
    }
}

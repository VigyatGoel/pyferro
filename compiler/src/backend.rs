use inkwell::OptimizationLevel;
use inkwell::module::Module;
use inkwell::passes::PassBuilderOptions;
use inkwell::targets::{
    CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine,
};
use std::path::Path;
use std::process::Command;

pub fn emit_object(module: &Module, machine: &TargetMachine, output_name: &str) {
    let obj_filename = format!("{}.o", output_name);
    let obj_path = Path::new(&obj_filename);
    machine
        .write_to_file(module, FileType::Object, obj_path)
        .expect("Failed to write object file");
    println!("Written: {}", obj_filename);
}

pub fn link_executable(output_name: &str) {
    let obj_filename = format!("{}.o", output_name);
    let status = Command::new("cc")
        .args([obj_filename.as_str(), "-o", output_name])
        .status()
        .expect("Failed to run linker");

    if status.success() {
        println!("Linked: ./{}", output_name);
        println!("Run it with: ./{}", output_name);
    } else {
        eprintln!("Linking failed");
    }
}

pub fn create_target_machine() -> TargetMachine {
    Target::initialize_native(&InitializationConfig::default())
        .expect("Failed to initialize native target");

    let triple = TargetMachine::get_default_triple();
    let target = Target::from_triple(&triple).expect("Could not get target");
    let cpu = TargetMachine::get_host_cpu_name();
    let features = TargetMachine::get_host_cpu_features();

    target
        .create_target_machine(
            &triple,
            cpu.to_str().unwrap(),
            features.to_str().unwrap(),
            OptimizationLevel::Default,
            RelocMode::Default,
            CodeModel::Default,
        )
        .expect("Could not create target machine")
}

pub fn run_optimization_passes(module: &Module, machine: &TargetMachine) {
    let pass_options = PassBuilderOptions::create();
    pass_options.set_verify_each(true);
    module
        .run_passes("default<O2>", machine, pass_options)
        .expect("Failed to run optimization passes");
}

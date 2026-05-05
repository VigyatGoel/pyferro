//! Snapshot tests: compile a fixture .py file to LLVM IR (--emit-ir) and assert
//! the emitted .ll content matches the golden snapshot stored in tests/snapshots/.
//! Run `cargo insta review` to accept new or changed snapshots.

use std::path::PathBuf;
use std::process::Command;

/// Compile `fixture_name.py` with `--emit-ir` and return the contents of the
/// emitted `.ll` file as a String. Panics with a clear message on any failure.
fn emit_ir(fixture_name: &str) -> String {
    let fixtures_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures");
    let input = fixtures_dir.join(format!("{}.py", fixture_name));
    let out_stem = fixtures_dir.join(fixture_name);

    let bin = env!("CARGO_BIN_EXE_pyferro");

    let status = Command::new(bin)
        .arg(input.to_str().unwrap())
        .arg("--output")
        .arg(out_stem.to_str().unwrap())
        .arg("--emit-ir")
        .status()
        .expect("failed to spawn pyferro");

    assert!(
        status.success(),
        "pyferro --emit-ir failed for fixture '{}'",
        fixture_name
    );

    let ll_path = fixtures_dir.join(format!("{}.ll", fixture_name));
    let ir = std::fs::read_to_string(&ll_path)
        .unwrap_or_else(|_| panic!("could not read emitted IR file: {:?}", ll_path));

    // Remove the .ll file — snapshots/ directory is the source of truth
    let _ = std::fs::remove_file(&ll_path);

    ir
}

#[test]
fn ir_snapshot_factorial() {
    let ir = emit_ir("factorial");
    insta::assert_snapshot!(ir);
}

#[test]
fn ir_snapshot_sum_to_n() {
    let ir = emit_ir("sum_to_n");
    insta::assert_snapshot!(ir);
}

#[test]
fn ir_snapshot_abs_value() {
    let ir = emit_ir("abs_value");
    insta::assert_snapshot!(ir);
}

#[test]
fn ir_snapshot_max_of_two() {
    let ir = emit_ir("max_of_two");
    insta::assert_snapshot!(ir);
}

#[test]
fn ir_snapshot_fibonacci() {
    let ir = emit_ir("fibonacci");
    insta::assert_snapshot!(ir);
}

#[test]
fn ir_snapshot_for_range() {
    let ir = emit_ir("test_for");
    insta::assert_snapshot!(ir);
}

#[test]
fn ir_snapshot_bool_is_positive() {
    let ir = emit_ir("bool_is_positive");
    insta::assert_snapshot!(ir);
}

#[test]
fn ir_snapshot_bool_and() {
    let ir = emit_ir("bool_and");
    insta::assert_snapshot!(ir);
}

#[test]
fn ir_snapshot_bool_not() {
    let ir = emit_ir("bool_not");
    insta::assert_snapshot!(ir);
}

#[test]
fn ir_snapshot_bool_param() {
    let ir = emit_ir("bool_param");
    insta::assert_snapshot!(ir);
}

#[test]
fn ir_snapshot_float_basic() {
    let ir = emit_ir("float_basic");
    insta::assert_snapshot!(ir);
}

#[test]
fn ir_snapshot_void_print() {
    let ir = emit_ir("void_print");
    insta::assert_snapshot!(ir);
}

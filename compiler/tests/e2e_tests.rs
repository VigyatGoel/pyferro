//! End-to-end tests: compile a fixture .py → native binary → run it → assert stdout.
//! Also contains negative tests that verify invalid fixtures cause a non-zero exit code.

use std::path::PathBuf;
use std::process::Command;

/// Compile `fixture_name.py` to a binary, run it, and return trimmed stdout.
/// Panics on compilation or execution failure.
fn compile_and_run(fixture_name: &str) -> String {
    let fixtures_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures");
    let input = fixtures_dir.join(format!("{}.py", fixture_name));
    let out_binary = fixtures_dir.join(format!("{}_bin", fixture_name));
    let out_obj = fixtures_dir.join(format!("{}_bin.o", fixture_name));

    let bin = env!("CARGO_BIN_EXE_pyferro");

    // Compile
    let status = Command::new(bin)
        .arg(input.to_str().unwrap())
        .arg("--output")
        .arg(out_binary.to_str().unwrap())
        .status()
        .expect("failed to spawn pyferro");

    assert!(
        status.success(),
        "pyferro compilation failed for fixture '{}'",
        fixture_name
    );

    // Run the compiled binary
    let output = Command::new(&out_binary)
        .output()
        .expect("failed to run compiled binary");

    // Cleanup
    let _ = std::fs::remove_file(&out_binary);
    let _ = std::fs::remove_file(&out_obj);

    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// Run pyferro on a fixture expected to fail compilation.
/// Returns the process exit code and stderr text.
fn expect_compile_failure(fixture_name: &str) -> (i32, String) {
    let fixtures_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures");
    let input = fixtures_dir.join(format!("{}.py", fixture_name));
    let out_stem = fixtures_dir.join(format!("{}_bin", fixture_name));

    let bin = env!("CARGO_BIN_EXE_pyferro");

    let output = Command::new(bin)
        .arg(input.to_str().unwrap())
        .arg("--output")
        .arg(out_stem.to_str().unwrap())
        .output()
        .expect("failed to spawn pyferro");

    let exit_code = output.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (exit_code, stderr)
}

// ── End-to-end execution tests ────────────────────────────────────────────────

#[test]
fn e2e_factorial_5_equals_120() {
    assert_eq!(compile_and_run("factorial"), "120");
}

#[test]
fn e2e_sum_to_n_10_equals_55() {
    assert_eq!(compile_and_run("sum_to_n"), "55");
}

#[test]
fn e2e_abs_value_neg7_equals_7() {
    assert_eq!(compile_and_run("abs_value"), "7");
}

#[test]
fn e2e_max_of_two_3_7_equals_7() {
    assert_eq!(compile_and_run("max_of_two"), "7");
}

#[test]
fn e2e_fibonacci_10_equals_55() {
    assert_eq!(compile_and_run("fibonacci"), "55");
}

#[test]
fn e2e_for_range_prints_0_to_4() {
    assert_eq!(compile_and_run("test_for"), "0\n1\n2\n3\n4");
}

#[test]
fn e2e_for_sum_range_equals_55() {
    assert_eq!(compile_and_run("for_sum"), "55");
}

#[test]
fn e2e_for_step_evens_equals_20() {
    assert_eq!(compile_and_run("for_step"), "20");
}

// ── Bool tests ───────────────────────────────────────────────────────────────

#[test]
fn e2e_bool_is_positive_true() {
    assert_eq!(compile_and_run("bool_is_positive"), "true");
}

#[test]
fn e2e_bool_is_positive_false() {
    assert_eq!(compile_and_run("bool_is_negative"), "false");
}

#[test]
fn e2e_bool_and_both_positive() {
    assert_eq!(compile_and_run("bool_and"), "true");
}

#[test]
fn e2e_bool_not_nonpositive() {
    assert_eq!(compile_and_run("bool_not"), "true");
}

#[test]
fn e2e_bool_param_identity_true() {
    assert_eq!(compile_and_run("bool_param"), "true");
}

// ── Float tests ──────────────────────────────────────────────────────────────

#[test]
fn e2e_float_average_3_7_equals_5() {
    assert_eq!(compile_and_run("float_basic"), "5.000000");
}

#[test]
fn e2e_multi_print_two_ints() {
    assert_eq!(compile_and_run("multi_print"), "1\n2");
}

// ── Negative tests — compiler must reject invalid Python ─────────────────────

#[test]
fn negative_missing_type_annotation_fails() {
    let (code, _stderr) = expect_compile_failure("bad_no_type");
    assert_ne!(code, 0, "expected non-zero exit for bad_no_type.py");
}

#[test]
fn negative_unsupported_type_fails() {
    let (code, _stderr) = expect_compile_failure("bad_wrong_type");
    assert_ne!(code, 0, "expected non-zero exit for bad_wrong_type.py");
}

#[test]
fn negative_missing_return_path_fails() {
    let (code, _stderr) = expect_compile_failure("bad_no_return");
    assert_ne!(code, 0, "expected non-zero exit for bad_no_return.py");
}

#[test]
fn negative_empty_body_fails() {
    let (code, _stderr) = expect_compile_failure("bad_empty_body");
    assert_ne!(code, 0, "expected non-zero exit for bad_empty_body.py");
}

#[test]
fn e2e_void_function_prints_value() {
    assert_eq!(compile_and_run("void_print"), "42");
}

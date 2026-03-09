#[test]
fn core_and_shell_model_types_must_match() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/shell_model_mismatch.rs");
}

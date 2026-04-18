#[test]
fn core_and_shell_model_types_must_match() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/shell_model_mismatch.rs");
    cases.compile_fail("tests/ui/model_duplicate_part.rs");
    cases.compile_fail("tests/ui/model_duplicate_wrapper.rs");
    cases.compile_fail("tests/ui/model_legacy_extract.rs");
    cases.compile_fail("tests/ui/model_repr_packed.rs");
}

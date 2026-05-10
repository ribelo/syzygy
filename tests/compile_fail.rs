#[test]
fn core_and_shell_model_types_must_match() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/shell_model_mismatch.rs");
    cases.compile_fail("tests/ui/model_duplicate_part.rs");
    cases.compile_fail("tests/ui/model_duplicate_wrapper.rs");
    cases.compile_fail("tests/ui/model_legacy_extract.rs");
    cases.compile_fail("tests/ui/model_repr_packed.rs");
}

#[test]
fn derive_model_subscription_parts_compile_in_downstream_crate() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/ui/subscription_part_downstream.rs");
}

#[test]
fn typed_effect_resources_are_checked_at_compile_time() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/ui/typed_effect_handler_method.rs");
    cases.pass("tests/ui/typed_effect_resource_present.rs");
    cases.pass("tests/ui/typed_effect_resource_tail.rs");
    cases.compile_fail("tests/ui/typed_effect_handler_method_missing.rs");
    cases.compile_fail("tests/ui/typed_effect_resource_missing.rs");
}

use crate::package_manager::name_validation::*;

// ── validate_cargo_package_name ──────────────────────────────────────────

#[test]
fn cargo_accepts_typical_names() {
	assert!(validate_cargo_package_name("my-crate").is_ok());
	assert!(validate_cargo_package_name("my_crate").is_ok());
	assert!(validate_cargo_package_name("MyCrate").is_ok());
	assert!(validate_cargo_package_name("_internal").is_ok());
	assert!(validate_cargo_package_name("1st-crate").is_ok());
}

#[test]
fn cargo_accepts_unicode_names() {
	assert!(validate_cargo_package_name("données").is_ok());
	assert!(validate_cargo_package_name("bibliothèque").is_ok());
}

#[test]
fn cargo_rejects_empty() {
	let err = validate_cargo_package_name("").unwrap_err();
	assert!(err.to_string().contains("must not be empty"));
}

#[test]
fn cargo_rejects_leading_dash() {
	let err = validate_cargo_package_name("--upload-pack=evil").unwrap_err();
	assert!(err.to_string().contains("must not start with '-'"));
}

#[test]
fn cargo_rejects_control_char() {
	assert!(validate_cargo_package_name("my\x00crate").is_err());
	assert!(validate_cargo_package_name("my\ncrate").is_err());
}

// ── validate_npm_package_name ────────────────────────────────────────────

#[test]
fn npm_accepts_typical_names() {
	assert!(validate_npm_package_name("my-app").is_ok());
	assert!(validate_npm_package_name("my.app").is_ok());
	assert!(validate_npm_package_name("MyApp").is_ok());
	assert!(validate_npm_package_name("cursus").is_ok());
}

#[test]
fn npm_accepts_scoped_names() {
	assert!(validate_npm_package_name("@my-org/my-app").is_ok());
	assert!(validate_npm_package_name("@cursus-test/app").is_ok());
	assert!(validate_npm_package_name("@scope/pkg-a").is_ok());
	assert!(validate_npm_package_name("@test/utils").is_ok());
}

#[test]
fn npm_rejects_empty() {
	let err = validate_npm_package_name("").unwrap_err();
	assert!(err.to_string().contains("must not be empty"));
}

#[test]
fn npm_rejects_leading_dash() {
	let err = validate_npm_package_name("--exec=evil").unwrap_err();
	assert!(err.to_string().contains("must not start with '-'"));
}

#[test]
fn npm_rejects_control_char() {
	assert!(validate_npm_package_name("my\x00pkg").is_err());
	assert!(validate_npm_package_name("my\npkg").is_err());
}

#[test]
fn npm_rejects_scoped_missing_slash() {
	let err = validate_npm_package_name("@scope-no-slash").unwrap_err();
	assert!(err.to_string().contains("@scope/name"));
}

#[test]
fn npm_rejects_scoped_empty_scope() {
	assert!(validate_npm_package_name("@/pkg").is_err());
}

#[test]
fn npm_rejects_scoped_empty_name() {
	assert!(validate_npm_package_name("@scope/").is_err());
}

#[test]
fn npm_rejects_scoped_leading_dash_in_scope() {
	assert!(validate_npm_package_name("@-scope/pkg").is_err());
}

#[test]
fn npm_rejects_scoped_leading_dash_in_name() {
	assert!(validate_npm_package_name("@scope/-pkg").is_err());
}

#[test]
fn rejects_over_1kb() {
	let long = "a".repeat(MAX_BYTES + 1);
	assert!(validate_cargo_package_name(&long).is_err());
	assert!(validate_npm_package_name(&long).is_err());
}

#[test]
fn cargo_accepts_exactly_1kb() {
	let at_limit = "a".repeat(MAX_BYTES);
	assert!(validate_cargo_package_name(&at_limit).is_ok());
}

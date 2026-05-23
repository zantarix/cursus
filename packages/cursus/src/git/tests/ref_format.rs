use crate::git::ref_format::*;

// ── validate_branch_name ─────────────────────────────────────────────────

#[test]
fn branch_accepts_typical_names() {
	assert!(validate_branch_name("main").is_ok());
	assert!(validate_branch_name("feature/foo").is_ok());
	assert!(validate_branch_name("cursus-release/main").is_ok());
	assert!(validate_branch_name("release-1.2.3").is_ok());
}

#[test]
fn branch_accepts_unicode() {
	assert!(validate_branch_name("fonctionnalité/ajout").is_ok());
}

#[test]
fn branch_rejects_empty() {
	let err = validate_branch_name("").unwrap_err();
	assert!(err.to_string().contains("must not be empty"));
}

#[test]
fn branch_rejects_leading_dash() {
	let err = validate_branch_name("--upload-pack=evil").unwrap_err();
	assert!(err.to_string().contains("must not start with '-'"));
}

#[test]
fn branch_rejects_single_dash() {
	assert!(validate_branch_name("-").is_err());
}

#[test]
fn branch_rejects_control_char() {
	assert!(validate_branch_name("feat\x07ure").is_err());
	assert!(validate_branch_name("feat\x00ure").is_err());
}

// ── validate_tag_name ────────────────────────────────────────────────────

#[test]
fn tag_accepts_typical_names() {
	assert!(validate_tag_name("v1.2.3").is_ok());
	assert!(validate_tag_name("my-crate@1.2.3").is_ok());
	assert!(validate_tag_name("@scope/pkg@1.0.0").is_ok());
	assert!(validate_tag_name("v1.0.0+build.1").is_ok());
}

#[test]
fn tag_accepts_unicode() {
	assert!(validate_tag_name("données@1.0.0").is_ok());
}

#[test]
fn tag_rejects_empty() {
	assert!(validate_tag_name("").is_err());
}

#[test]
fn tag_rejects_leading_dash() {
	let err = validate_tag_name("--upload-pack=evil").unwrap_err();
	assert!(err.to_string().contains("must not start with '-'"));
}

#[test]
fn tag_rejects_control_char() {
	assert!(validate_tag_name("v1.0\x00.0").is_err());
}

// ── validate_revision ────────────────────────────────────────────────────

#[test]
fn revision_accepts_typical_values() {
	assert!(validate_revision("HEAD").is_ok());
	assert!(validate_revision("origin/HEAD..HEAD").is_ok());
	assert!(validate_revision("HEAD~3").is_ok());
	assert!(validate_revision("abc1234def567890").is_ok());
	assert!(validate_revision("main").is_ok());
}

#[test]
fn revision_rejects_empty() {
	assert!(validate_revision("").is_err());
}

#[test]
fn revision_rejects_leading_dash() {
	let err = validate_revision("--exec=evil").unwrap_err();
	assert!(err.to_string().contains("must not start with '-'"));
}

#[test]
fn revision_rejects_control_char() {
	assert!(validate_revision("HEAD\x00evil").is_err());
	assert!(validate_revision("HEAD\nevil").is_err());
}

#[test]
fn rejects_over_1kb() {
	let long = "a".repeat(MAX_BYTES + 1);
	assert!(validate_branch_name(&long).is_err());
	assert!(validate_tag_name(&long).is_err());
	assert!(validate_revision(&long).is_err());
}

#[test]
fn accepts_exactly_1kb() {
	let at_limit = "a".repeat(MAX_BYTES);
	assert!(validate_branch_name(&at_limit).is_ok());
}

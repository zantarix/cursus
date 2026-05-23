use crate::conventional_commit::*;
use crate::model::changeset::ChangeType;

// --- parse ---

#[test]
fn parse_simple_fix() {
	let c = parse("fix: correct off-by-one error").unwrap();
	assert_eq!(c.commit_type, "fix");
	assert_eq!(c.scope, None);
	assert!(!c.breaking);
	assert_eq!(c.description, "correct off-by-one error");
	assert_eq!(c.body, None);
}

#[test]
fn parse_simple_feat() {
	let c = parse("feat: add new widget").unwrap();
	assert_eq!(c.commit_type, "feat");
	assert_eq!(c.scope, None);
	assert!(!c.breaking);
	assert_eq!(c.description, "add new widget");
}

#[test]
fn parse_chore_commit() {
	let c = parse("chore: update dependencies").unwrap();
	assert_eq!(c.commit_type, "chore");
	assert!(!c.breaking);
	assert_eq!(c.description, "update dependencies");
}

#[test]
fn parse_with_scope() {
	let c = parse("feat(auth): add OAuth2 support").unwrap();
	assert_eq!(c.commit_type, "feat");
	assert_eq!(c.scope, Some("auth".to_string()));
	assert!(!c.breaking);
	assert_eq!(c.description, "add OAuth2 support");
}

#[test]
fn parse_breaking_via_bang() {
	let c = parse("feat!: remove deprecated API").unwrap();
	assert_eq!(c.commit_type, "feat");
	assert!(c.breaking);
	assert_eq!(c.description, "remove deprecated API");
}

#[test]
fn parse_breaking_with_scope_and_bang() {
	let c = parse("feat(api)!: redesign authentication").unwrap();
	assert_eq!(c.commit_type, "feat");
	assert_eq!(c.scope, Some("api".to_string()));
	assert!(c.breaking);
	assert_eq!(c.description, "redesign authentication");
}

#[test]
fn parse_breaking_via_footer_breaking_change() {
	let msg = "feat: new login flow\n\nAdds support for SSO.\n\nBREAKING CHANGE: old login endpoint removed";
	let c = parse(msg).unwrap();
	assert_eq!(c.commit_type, "feat");
	assert!(c.breaking);
	assert_eq!(c.description, "new login flow");
}

#[test]
fn parse_breaking_via_footer_breaking_change_hyphen() {
	let msg =
		"refactor: overhaul config\n\nSome details.\n\nBREAKING-CHANGE: config format changed";
	let c = parse(msg).unwrap();
	assert!(c.breaking);
}

#[test]
fn parse_body_extracted() {
	let msg =
		"fix: resolve race condition\n\nThis was causing crashes under high load.\nSee issue #123.";
	let c = parse(msg).unwrap();
	assert_eq!(c.description, "resolve race condition");
	assert_eq!(
		c.body,
		Some("This was causing crashes under high load.\nSee issue #123.".to_string())
	);
}

#[test]
fn parse_body_none_when_empty_after_blank_line() {
	let c = parse("fix: something\n\n   \n").unwrap();
	assert_eq!(c.body, None);
}

#[test]
fn parse_no_blank_line_means_no_body() {
	let c = parse("fix: quick fix").unwrap();
	assert_eq!(c.body, None);
}

#[test]
fn parse_multiline_header_folds_continuation_into_description() {
	// Git can word-wrap long subjects; the parser treats everything before
	// the first blank line as the header, so the continuation line is
	// folded into the description verbatim.
	let msg = "chore: fixed something\nbut git wrapped this line\n\nBody goes here";
	let c = parse(msg).unwrap();
	assert_eq!(c.commit_type, "chore");
	assert_eq!(c.description, "fixed something\nbut git wrapped this line");
	assert_eq!(c.body, Some("Body goes here".to_string()));
}

#[test]
fn parse_single_trailing_newline_no_body() {
	// A trailing \n without a blank line never triggers the \n\n split.
	// The description is trimmed, so the trailing newline is stripped.
	let c = parse("fix: thing\n").unwrap();
	assert_eq!(c.description, "thing");
	assert_eq!(c.body, None);
}

#[test]
fn parse_single_newline_between_lines_folds_into_description() {
	// Without a blank line, the second line is part of the header, not the body.
	let c = parse("fix: thing\nsecond line").unwrap();
	assert_eq!(c.description, "thing\nsecond line");
	assert_eq!(c.body, None);
}

#[test]
fn parse_missing_separator_is_error() {
	assert!(parse("feat add thing").is_err());
}

#[test]
fn parse_empty_description_is_error() {
	assert!(parse("fix: ").is_err());
}

#[test]
fn parse_missing_type_is_error() {
	assert!(parse(": something").is_err());
}

#[test]
fn parse_unclosed_scope_is_error() {
	assert!(parse("feat(auth: add something").is_err());
}

#[test]
fn parse_hyphenated_type() {
	let c = parse("build-system: update toolchain").unwrap();
	assert_eq!(c.commit_type, "build-system");
}

#[test]
fn parse_invalid_char_in_type_is_error() {
	assert!(parse("feat@scope: desc").is_err());
}

// --- strip_trailers ---

#[test]
fn strip_trailers_only_trailers_returns_none() {
	assert_eq!(
		strip_trailers(
			"Signed-off-by: Alice <alice@example.com>\nCo-authored-by: Bob <bob@example.com>"
		),
		None
	);
}

#[test]
fn strip_trailers_body_with_trailers_strips_them() {
	let input = "This fixes the crash.\n\nSigned-off-by: Alice <alice@example.com>";
	assert_eq!(
		strip_trailers(input),
		Some("This fixes the crash.".to_string())
	);
}

#[test]
fn strip_trailers_body_without_trailers_unchanged() {
	let input = "This is a normal body.\nWith multiple lines.";
	assert_eq!(
		strip_trailers(input),
		Some("This is a normal body.\nWith multiple lines.".to_string())
	);
}

#[test]
fn strip_trailers_mixed_colon_and_hash_trailers() {
	let input = "Prose.\n\nSigned-off-by: Alice\nFixes #42\nCloses #99";
	assert_eq!(strip_trailers(input), Some("Prose.".to_string()));
}

#[test]
fn strip_trailers_breaking_change_trailer_stripped() {
	let input = "Some details.\n\nBREAKING CHANGE: old API removed";
	assert_eq!(strip_trailers(input), Some("Some details.".to_string()));
}

#[test]
fn strip_trailers_github_keyword_trailers() {
	let input = "Fix the crash.\n\nFixes #123\nCloses #456";
	assert_eq!(strip_trailers(input), Some("Fix the crash.".to_string()));
}

#[test]
fn strip_trailers_prose_resembling_trailer_in_middle_preserved() {
	// "Example: some value" is NOT at the tail — a non-trailer line follows it.
	let input = "Example: some value\nThis is a normal line.\n\nSigned-off-by: Alice";
	assert_eq!(
		strip_trailers(input),
		Some("Example: some value\nThis is a normal line.".to_string())
	);
}

#[test]
fn strip_trailers_all_blank_returns_none() {
	assert_eq!(strip_trailers("   \n  \n"), None);
}

// --- parse (trailer stripping) ---

#[test]
fn parse_body_with_trailers_strips_them() {
	let msg =
		"fix: resolve null pointer\n\nThis was important.\n\nSigned-off-by: Foo <foo@bar.com>";
	let c = parse(msg).unwrap();
	assert_eq!(c.body, Some("This was important.".to_string()));
}

#[test]
fn parse_trailers_only_body_becomes_none() {
	let msg = "feat: add feature\n\nSigned-off-by: Foo <foo@bar.com>";
	let c = parse(msg).unwrap();
	assert_eq!(c.body, None);
}

#[test]
fn parse_breaking_footer_still_detected_with_trailers() {
	let msg = "feat: new thing\n\nBREAKING CHANGE: old API removed\nSigned-off-by: Foo";
	let c = parse(msg).unwrap();
	assert!(c.breaking);
	assert_eq!(c.body, None);
}

#[test]
fn parse_body_with_inline_colon_not_stripped() {
	// "The config key: value" has a multi-word token before `: ` — not a trailer.
	let msg = "fix: thing\n\nThe config key: value format changed";
	let c = parse(msg).unwrap();
	assert_eq!(
		c.body,
		Some("The config key: value format changed".to_string())
	);
}

// --- change_type ---

#[test]
fn change_type_fix_is_patch() {
	let c = parse("fix: correct a bug").unwrap();
	assert_eq!(c.change_type(), Some(ChangeType::Patch));
}

#[test]
fn change_type_feat_is_minor() {
	let c = parse("feat: new feature").unwrap();
	assert_eq!(c.change_type(), Some(ChangeType::Minor));
}

#[test]
fn change_type_breaking_is_major() {
	let c = parse("fix!: breaking bugfix").unwrap();
	assert_eq!(c.change_type(), Some(ChangeType::Major));
}

#[test]
fn change_type_breaking_footer_is_major() {
	let c = parse("feat: new stuff\n\nBREAKING CHANGE: old api gone").unwrap();
	assert_eq!(c.change_type(), Some(ChangeType::Major));
}

#[test]
fn change_type_chore_is_none() {
	let c = parse("chore: update deps").unwrap();
	assert_eq!(c.change_type(), None);
}

#[test]
fn change_type_refactor_is_none() {
	let c = parse("refactor: tidy up code").unwrap();
	assert_eq!(c.change_type(), None);
}

#[test]
fn change_type_docs_is_none() {
	let c = parse("docs: update readme").unwrap();
	assert_eq!(c.change_type(), None);
}

//! Integration tests for Unicode handling across cursus operations.
//!
//! Verifies that non-ASCII content flows correctly through config loading,
//! changeset creation, parsing, and changelog generation, and that operations
//! work correctly in directories with Unicode names.

mod common;

use common::{
	add_local_remote, git_cmd, git_log, git_push_to_remote, run_cursus, temp_git_repo,
	temp_git_repo_with_cargo_workspace, temp_git_repo_with_project, temp_real_git_repo_with_config,
	write_changeset,
};
use cursus::model::config::{
	CargoConfig, Config, GitConfig, GitHubConfig, GlobalConfig, PackageManager,
};
use cursus::path::AbsolutePath;
use tempfile::TempDir;

// ── Local helpers ──────────────────────────────────────────────────────────────

/// Creates a temp dir with a Unicode prefix, sets up fake `.git`, Cargo config, and manifest.
fn temp_git_repo_with_project_in_unicode_dir(prefix: &str) -> TempDir {
	let dir = tempfile::Builder::new()
		.prefix(prefix)
		.tempdir()
		.expect("Failed to create unicode temp dir");
	std::fs::create_dir(dir.path().join(".git")).unwrap();
	let config =
		Config::new(&AbsolutePath::new(dir.path()).unwrap()).with_cargo(CargoConfig::enabled());
	config
		.with_env(common::test_env(dir.path()))
		.save()
		.unwrap();
	std::fs::write(
		dir.path().join("Cargo.toml"),
		"[package]\nname = \"test-project\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
	)
	.unwrap();
	std::fs::create_dir_all(dir.path().join("src")).unwrap();
	std::fs::write(dir.path().join("src/lib.rs"), "").unwrap();
	dir
}

/// Returns the contents of all `.md` files in `.cursus/`.
fn read_changeset_files(dir: &std::path::Path) -> Vec<String> {
	let cursus_dir = dir.join(".cursus");
	if !cursus_dir.exists() {
		return vec![];
	}
	std::fs::read_dir(&cursus_dir)
		.expect("Failed to read .cursus dir")
		.filter_map(|entry| {
			let entry = entry.ok()?;
			let path = entry.path();
			if path.extension()?.to_str()? == "md" {
				Some(std::fs::read_to_string(&path).expect("Failed to read changeset file"))
			} else {
				None
			}
		})
		.collect()
}

/// Writes a config file with the given TOML content under `.cursus/`.
fn write_config(dir: &std::path::Path, toml: &str) {
	let config_dir = dir.join(".cursus");
	std::fs::create_dir_all(&config_dir).unwrap();
	std::fs::write(config_dir.join("config.toml"), toml).unwrap();
}

/// Stages all files and creates a commit with the given message.
fn git_commit_all(dir: &std::path::Path, message: &str) {
	git_cmd(dir, &["add", "."]);
	git_cmd(dir, &["commit", "-m", message]);
}

/// Writes a single-package Cargo setup into the given directory and commits it.
fn setup_single_cargo_package(dir: &std::path::Path, name: &str, version: &str) {
	std::fs::write(
		dir.join("Cargo.toml"),
		format!("[package]\nname = \"{name}\"\nversion = \"{version}\"\nedition = \"2024\"\n"),
	)
	.unwrap();
	std::fs::create_dir_all(dir.join("src")).unwrap();
	std::fs::write(dir.join("src/lib.rs"), "").unwrap();
	git_commit_all(dir, "chore: add package");
}

// ── Category 1: Unicode in changeset messages via `change` command ─────────────

/// Runs `change -t <change_type> -m <message>` and asserts the message is preserved
/// verbatim in the written changeset file.
fn assert_change_preserves_message(change_type: &str, message: &str) {
	let dir = temp_git_repo_with_project(PackageManager::Cargo);
	let result = run_cursus(
		[
			"cursus",
			"--no-interactive",
			"change",
			"-t",
			change_type,
			"-m",
			message,
		],
		dir.path(),
	);
	assert!(result.is_ok(), "Expected success: {result:?}");
	let files = read_changeset_files(dir.path());
	assert!(!files.is_empty(), "Expected at least one changeset file");
	assert!(
		files.iter().any(|content| content.contains(message)),
		"Expected message {:?} in changeset, got: {files:?}",
		message
	);
}

#[test]
fn change_with_emoji_message() {
	assert_change_preserves_message("minor", "🎉 Added internationalization support");
}

#[test]
fn change_with_cjk_message() {
	assert_change_preserves_message("minor", "新機能を追加しました");
}

#[test]
fn change_with_mixed_script_message() {
	assert_change_preserves_message("patch", "Ändere die Konfiguration für café");
}

#[test]
fn change_with_rtl_message() {
	assert_change_preserves_message("minor", "إضافة ميزة جديدة");
}

#[test]
fn change_with_combining_characters_message() {
	// "cafe" + U+0301 COMBINING ACUTE ACCENT = "café" in NFD form
	assert_change_preserves_message("patch", "Update cafe\u{0301} configuration");
}

// ── Category 2: Unicode roundtripped through `prepare` ─────────────────────────

#[test]
fn prepare_preserves_emoji_in_changelog() {
	let dir = temp_git_repo_with_project(PackageManager::Cargo);
	let message = "🎉 Added internationalization support";
	write_changeset(
		dir.path(),
		"change.md",
		&format!("+++\ntest-project = \"minor\"\n+++\n\n{message}\n"),
	);
	let result = run_cursus(["cursus", "--no-interactive", "prepare"], dir.path());
	assert!(result.is_ok(), "Expected success: {result:?}");
	let changelog = std::fs::read_to_string(dir.path().join("CHANGELOG.md"))
		.expect("CHANGELOG.md should exist after prepare");
	assert!(
		changelog.contains(message),
		"Expected emoji message in changelog, got:\n{changelog}"
	);
}

#[test]
fn prepare_preserves_cjk_in_changelog() {
	let dir = temp_git_repo_with_project(PackageManager::Cargo);
	let message = "新機能を追加しました";
	write_changeset(
		dir.path(),
		"change.md",
		&format!("+++\ntest-project = \"minor\"\n+++\n\n{message}\n"),
	);
	let result = run_cursus(["cursus", "--no-interactive", "prepare"], dir.path());
	assert!(result.is_ok(), "Expected success: {result:?}");
	let changelog = std::fs::read_to_string(dir.path().join("CHANGELOG.md"))
		.expect("CHANGELOG.md should exist after prepare");
	assert!(
		changelog.contains(message),
		"Expected CJK message in changelog, got:\n{changelog}"
	);
}

#[test]
fn prepare_preserves_mixed_unicode_across_sections() {
	let dir = temp_git_repo_with_cargo_workspace(&[("alpha", "0.1.0"), ("beta", "0.1.0")]);
	let minor_message = "Füge neue Konfigurationsoptionen hinzu";
	let patch_message = "Исправить ошибку в обработке запросов";
	write_changeset(
		dir.path(),
		"feature.md",
		&format!("+++\nalpha = \"minor\"\n+++\n\n{minor_message}\n"),
	);
	write_changeset(
		dir.path(),
		"fix.md",
		&format!("+++\nbeta = \"patch\"\n+++\n\n{patch_message}\n"),
	);
	let result = run_cursus(["cursus", "--no-interactive", "prepare"], dir.path());
	assert!(result.is_ok(), "Expected success: {result:?}");
	// In a workspace, each package gets its own CHANGELOG.md under its subdirectory.
	let alpha_changelog = std::fs::read_to_string(dir.path().join("alpha/CHANGELOG.md"))
		.expect("alpha/CHANGELOG.md should exist after prepare");
	assert!(
		alpha_changelog.contains(minor_message),
		"Expected Latin-accent message in alpha changelog, got:\n{alpha_changelog}"
	);
	let beta_changelog = std::fs::read_to_string(dir.path().join("beta/CHANGELOG.md"))
		.expect("beta/CHANGELOG.md should exist after prepare");
	assert!(
		beta_changelog.contains(patch_message),
		"Expected Cyrillic message in beta changelog, got:\n{beta_changelog}"
	);
}

#[test]
fn prepare_preserves_message_with_pr_like_pattern_and_unicode() {
	let dir = temp_git_repo_with_project(PackageManager::Cargo);
	// Multi-byte chars adjacent to a parenthetical PR-like pattern exercises
	// byte-offset arithmetic in extract_pr_number without corrupting neighbours.
	let message = "Fix for café (#42)";
	write_changeset(
		dir.path(),
		"change.md",
		&format!("+++\ntest-project = \"patch\"\n+++\n\n{message}\n"),
	);
	let result = run_cursus(["cursus", "--no-interactive", "prepare"], dir.path());
	assert!(result.is_ok(), "Expected success: {result:?}");
	let changelog = std::fs::read_to_string(dir.path().join("CHANGELOG.md"))
		.expect("CHANGELOG.md should exist");
	assert!(
		changelog.contains("café"),
		"Expected 'café' preserved in changelog near PR pattern, got:\n{changelog}"
	);
}

#[test]
fn prepare_preserves_multiline_unicode_message() {
	let dir = temp_git_repo_with_project(PackageManager::Cargo);
	let line1 = "First line: 新機能";
	let line2 = "Second line: Ändere";
	let line3 = "Third line: إضافة";
	let message = format!("{line1}\n{line2}\n{line3}");
	write_changeset(
		dir.path(),
		"change.md",
		&format!("+++\ntest-project = \"minor\"\n+++\n\n{message}\n"),
	);
	let result = run_cursus(["cursus", "--no-interactive", "prepare"], dir.path());
	assert!(result.is_ok(), "Expected success: {result:?}");
	let changelog = std::fs::read_to_string(dir.path().join("CHANGELOG.md"))
		.expect("CHANGELOG.md should exist");
	assert!(
		changelog.contains(line1),
		"Expected CJK first line in changelog, got:\n{changelog}"
	);
	assert!(
		changelog.contains(line2),
		"Expected Latin second line in changelog, got:\n{changelog}"
	);
	assert!(
		changelog.contains(line3),
		"Expected Arabic third line in changelog, got:\n{changelog}"
	);
}

// ── Category 3: Unicode in directory paths ─────────────────────────────────────

#[test]
fn change_in_unicode_directory_cjk() {
	// テスト = "test" in Japanese katakana
	let dir = temp_git_repo_with_project_in_unicode_dir("cursus-\u{30C6}\u{30B9}\u{30C8}-");
	let result = run_cursus(
		[
			"cursus",
			"--no-interactive",
			"change",
			"-t",
			"minor",
			"-m",
			"test change",
		],
		dir.path(),
	);
	assert!(
		result.is_ok(),
		"Expected success in CJK directory: {result:?}"
	);
	let files = read_changeset_files(dir.path());
	assert!(!files.is_empty(), "Expected a changeset file");
}

#[test]
fn change_in_unicode_directory_emoji() {
	// 🚀 = U+1F680 ROCKET
	let dir = temp_git_repo_with_project_in_unicode_dir("cursus-\u{1F680}-");
	let result = run_cursus(
		[
			"cursus",
			"--no-interactive",
			"change",
			"-t",
			"patch",
			"-m",
			"test change",
		],
		dir.path(),
	);
	assert!(
		result.is_ok(),
		"Expected success in emoji directory: {result:?}"
	);
	let files = read_changeset_files(dir.path());
	assert!(!files.is_empty(), "Expected a changeset file");
}

#[test]
fn prepare_in_unicode_directory() {
	// café — precomposed é (U+00E9)
	let dir = temp_git_repo_with_project_in_unicode_dir("cursus-caf\u{00E9}-");
	let message = "A feature for café users";
	write_changeset(
		dir.path(),
		"change.md",
		&format!("+++\ntest-project = \"minor\"\n+++\n\n{message}\n"),
	);
	let result = run_cursus(["cursus", "--no-interactive", "prepare"], dir.path());
	assert!(
		result.is_ok(),
		"Expected prepare to succeed in unicode directory: {result:?}"
	);
	let changelog = std::fs::read_to_string(dir.path().join("CHANGELOG.md"))
		.expect("CHANGELOG.md should exist");
	assert!(
		changelog.contains(message),
		"Expected message in changelog, got:\n{changelog}"
	);
	let cargo_toml =
		std::fs::read_to_string(dir.path().join("Cargo.toml")).expect("Cargo.toml should exist");
	assert!(
		cargo_toml.contains("0.2.0"),
		"Expected version bump to 0.2.0 in Cargo.toml, got:\n{cargo_toml}"
	);
}

// ── Category 4: Unicode via the config file ────────────────────────────────────

#[test]
fn config_with_unicode_ignore_pattern() {
	// "bibliothèque" as a package name exercices the ignore glob match path
	// with a multi-byte UTF-8 package name.
	let dir =
		temp_git_repo_with_cargo_workspace(&[("app", "0.1.0"), ("biblioth\u{00E8}que", "0.1.0")]);
	let global = GlobalConfig {
		ignore: vec!["biblioth\u{00E8}que".to_string()],
		..Default::default()
	};
	let config = Config::new(&AbsolutePath::new(dir.path()).unwrap())
		.with_global(global)
		.with_cargo(CargoConfig::enabled());
	config
		.with_env(common::test_env(dir.path()))
		.save()
		.unwrap();

	// Targeting "app" (non-ignored) must succeed.
	let result = run_cursus(
		[
			"cursus",
			"--no-interactive",
			"change",
			"-t",
			"minor",
			"-m",
			"test",
			"-p",
			"app",
		],
		dir.path(),
	);
	assert!(
		result.is_ok(),
		"Expected success targeting non-ignored package: {result:?}"
	);

	// Targeting the unicode-named ignored package must fail (it is not visible).
	let result = run_cursus(
		[
			"cursus",
			"--no-interactive",
			"change",
			"-t",
			"minor",
			"-m",
			"test",
			"-p",
			"biblioth\u{00E8}que",
		],
		dir.path(),
	);
	assert!(
		result.is_err(),
		"Expected error when targeting unicode-named ignored package"
	);
}

#[test]
fn config_with_unicode_subfolder_path() {
	// "données" as the cargo.path subfolder exercises AbsolutePath and safe_glob()
	// with a multi-byte directory segment.
	let dir = temp_git_repo();
	let subfolder = "donn\u{00E9}es";
	let mut config =
		Config::new(&AbsolutePath::new(dir.path()).unwrap()).with_cargo(CargoConfig::enabled());
	config.cargo.path = Some(subfolder.to_string());
	config
		.with_env(common::test_env(dir.path()))
		.save()
		.unwrap();
	let sub_path = dir.path().join(subfolder);
	std::fs::create_dir_all(&sub_path).unwrap();
	std::fs::write(
		sub_path.join("Cargo.toml"),
		"[package]\nname = \"test-project\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
	)
	.unwrap();

	let result = run_cursus(
		[
			"cursus",
			"--no-interactive",
			"change",
			"-t",
			"minor",
			"-m",
			"test",
		],
		dir.path(),
	);
	assert!(
		result.is_ok(),
		"Expected success with unicode subfolder path: {result:?}"
	);
}

#[test]
fn change_targets_unicode_project_name() {
	// "données" as a workspace package name exercises the -p flag string match
	// and the changeset frontmatter write path with a multi-byte package name.
	let dir = temp_git_repo_with_cargo_workspace(&[("donn\u{00E9}es", "0.1.0"), ("app", "0.1.0")]);

	let result = run_cursus(
		[
			"cursus",
			"--no-interactive",
			"change",
			"-t",
			"minor",
			"-m",
			"test",
			"-p",
			"donn\u{00E9}es",
		],
		dir.path(),
	);
	assert!(
		result.is_ok(),
		"Expected success targeting unicode package name: {result:?}"
	);
	let files = read_changeset_files(dir.path());
	assert!(
		files
			.iter()
			.any(|content| content.contains("donn\u{00E9}es")),
		"Expected unicode package name in changeset frontmatter, got: {files:?}"
	);
}

// ── Category 5: Unicode in git commit messages ─────────────────────────────────

#[test]
fn prepare_git_unicode_commit_message() {
	// "ci: mise à jour des versions 🚀" — Latin-with-accents + emoji commit message
	let commit_msg = "ci: mise \u{00E0} jour des versions \u{1F680}";
	let config = GitConfig::enabled_config().with_prepare_commit_message(commit_msg.to_string());
	let dir = temp_real_git_repo_with_config(PackageManager::Cargo, config);
	setup_single_cargo_package(dir.path(), "my-pkg", "1.0.0");
	write_changeset(
		dir.path(),
		"change.md",
		"+++\nmy-pkg = \"patch\"\n+++\n\nA fix\n",
	);
	git_commit_all(dir.path(), "chore: add changeset");
	let _remote = add_local_remote(dir.path());
	git_push_to_remote(dir.path());

	let result = run_cursus(["cursus", "--no-interactive", "prepare"], dir.path());
	assert!(result.is_ok(), "Expected success: {result:?}");

	let log = git_log(dir.path());
	assert!(
		log[0].contains(commit_msg),
		"Expected unicode commit message, got: {}",
		log[0]
	);
}

// ── Category 6: Unicode in GitHub config fields (TOML roundtrip) ───────────────

#[test]
fn github_config_unicode_pr_title_loads() {
	// "Mise à jour des versions 🎉" as pull_request_title verifies that
	// multi-byte values in the github config section survive TOML serialization
	// and deserialization via the Config builder API.
	let dir = temp_git_repo();
	let config = Config::new(&AbsolutePath::new(dir.path()).unwrap())
		.with_cargo(CargoConfig::enabled())
		.with_github(
			GitHubConfig::enabled_config()
				.with_pull_request_title("Mise \u{00E0} jour des versions \u{1F389}".to_string()),
		);
	config
		.with_env(common::test_env(dir.path()))
		.save()
		.unwrap();
	std::fs::write(
		dir.path().join("Cargo.toml"),
		"[package]\nname = \"my-app\"\nversion = \"0.1.0\"\n",
	)
	.unwrap();

	let result = run_cursus(
		["cursus", "publish", "--dry-run", "--no-interactive"],
		dir.path(),
	);
	assert!(
		result.is_ok(),
		"Expected config to load with unicode PR title: {result:?}"
	);
}

#[test]
fn github_config_unicode_artifact_name_loads() {
	// "binário-linux" as an artifact display name key verifies that Unicode
	// keys in the [github.artifacts] BTreeMap survive TOML deserialization.
	let dir = temp_git_repo();
	write_config(
		dir.path(),
		"[cargo]\nenabled = true\n[github]\nenabled = true\nowner = \"acme\"\nrepo = \"app\"\n[github.artifacts]\n\"bin\u{00E1}rio-linux\" = \"target/release/app\"\n",
	);
	std::fs::write(
		dir.path().join("Cargo.toml"),
		"[package]\nname = \"my-app\"\nversion = \"1.0.0\"\n",
	)
	.unwrap();
	std::fs::write(dir.path().join("CHANGELOG.md"), "# Changelog\n").unwrap();

	let result = run_cursus(
		["cursus", "publish", "--dry-run", "--no-interactive"],
		dir.path(),
	);
	assert!(
		result.is_ok(),
		"Expected config to load with unicode artifact name: {result:?}"
	);
}

// ── Category 7: Combined Unicode path + Unicode content ────────────────────────

#[test]
fn change_and_prepare_unicode_path_and_content() {
	// 🌍 = U+1F30D EARTH GLOBE EUROPE-AFRICA
	let dir = temp_git_repo_with_project_in_unicode_dir("cursus-\u{1F30D}-");
	let message = "🌍 Support for international users";
	write_changeset(
		dir.path(),
		"change.md",
		&format!("+++\ntest-project = \"minor\"\n+++\n\n{message}\n"),
	);
	let result = run_cursus(["cursus", "--no-interactive", "prepare"], dir.path());
	assert!(
		result.is_ok(),
		"Expected success with unicode path and content: {result:?}"
	);
	let changelog = std::fs::read_to_string(dir.path().join("CHANGELOG.md"))
		.expect("CHANGELOG.md should exist");
	assert!(
		changelog.contains(message),
		"Expected emoji message in changelog from unicode dir, got:\n{changelog}"
	);
}

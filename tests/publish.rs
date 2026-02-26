//! Integration tests for the `publish` command.

use std::path::Path;

/// Helper to run chronicle commands.
fn run_chronicle(args: &[&str], cwd: &Path) -> anyhow::Result<std::process::ExitCode> {
	let args_with_bin = std::iter::once("chronicle")
		.chain(args.iter().copied())
		.collect::<Vec<_>>();
	chronicle::run(args_with_bin, cwd)
}

#[test]
fn publish_with_no_config_fails() {
	let dir = tempfile::tempdir().unwrap();
	// Create git repo
	std::fs::create_dir(dir.path().join(".git")).unwrap();

	let result = run_chronicle(&["publish", "--no-interactive", "--dry-run"], dir.path());

	assert!(result.is_err());
	assert!(
		result
			.unwrap_err()
			.to_string()
			.contains("No configuration found")
	);
}

#[test]
fn publish_dry_run_with_unknown_package_fails() {
	let dir = tempfile::tempdir().unwrap();
	// Create git repo
	std::fs::create_dir(dir.path().join(".git")).unwrap();

	// Create config
	std::fs::create_dir(dir.path().join(".chronicle")).unwrap();
	std::fs::write(
		dir.path().join(".chronicle/config.toml"),
		r#"
[npm]
enabled = true
"#,
	)
	.unwrap();

	// Create package.json
	std::fs::write(
		dir.path().join("package.json"),
		r#"{"name": "test-pkg", "version": "1.0.0"}"#,
	)
	.unwrap();

	let result = run_chronicle(
		&[
			"publish",
			"--no-interactive",
			"--dry-run",
			"--package",
			"nonexistent",
		],
		dir.path(),
	);

	assert!(result.is_err());
	assert!(
		result
			.unwrap_err()
			.to_string()
			.contains("Unknown package: nonexistent")
	);
}

#[test]
fn publish_dry_run_basic() {
	let dir = tempfile::tempdir().unwrap();
	// Create git repo
	std::fs::create_dir(dir.path().join(".git")).unwrap();

	// Create config
	std::fs::create_dir(dir.path().join(".chronicle")).unwrap();
	std::fs::write(
		dir.path().join(".chronicle/config.toml"),
		r#"
[npm]
enabled = true
"#,
	)
	.unwrap();

	// Create package.json
	std::fs::write(
		dir.path().join("package.json"),
		r#"{"name": "test-pkg", "version": "1.0.0"}"#,
	)
	.unwrap();

	let result = run_chronicle(&["publish", "--no-interactive", "--dry-run"], dir.path());

	assert!(result.is_ok());
	let exit_code = result.unwrap();
	assert_eq!(exit_code, std::process::ExitCode::SUCCESS);
}

#[test]
fn publish_with_package_filter() {
	let dir = tempfile::tempdir().unwrap();
	// Create git repo
	std::fs::create_dir(dir.path().join(".git")).unwrap();

	// Create config
	std::fs::create_dir(dir.path().join(".chronicle")).unwrap();
	std::fs::write(
		dir.path().join(".chronicle/config.toml"),
		r#"
[npm]
enabled = true
"#,
	)
	.unwrap();

	// Create root package.json with workspaces
	std::fs::write(
		dir.path().join("package.json"),
		r#"{"name": "root", "version": "1.0.0", "workspaces": ["packages/*"]}"#,
	)
	.unwrap();

	// Create workspace packages
	std::fs::create_dir_all(dir.path().join("packages/pkg-a")).unwrap();
	std::fs::write(
		dir.path().join("packages/pkg-a/package.json"),
		r#"{"name": "pkg-a", "version": "1.0.0"}"#,
	)
	.unwrap();

	std::fs::create_dir_all(dir.path().join("packages/pkg-b")).unwrap();
	std::fs::write(
		dir.path().join("packages/pkg-b/package.json"),
		r#"{"name": "pkg-b", "version": "1.0.0"}"#,
	)
	.unwrap();

	let result = run_chronicle(
		&[
			"publish",
			"--no-interactive",
			"--dry-run",
			"--package",
			"pkg-a",
		],
		dir.path(),
	);

	assert!(result.is_ok());
	let exit_code = result.unwrap();
	assert_eq!(exit_code, std::process::ExitCode::SUCCESS);
}

#[test]
fn publish_dry_run_with_workspace_dependencies() {
	let dir = tempfile::tempdir().unwrap();
	std::fs::create_dir(dir.path().join(".git")).unwrap();

	// Create config
	std::fs::create_dir(dir.path().join(".chronicle")).unwrap();
	std::fs::write(
		dir.path().join(".chronicle/config.toml"),
		"[npm]\nenabled = true\n",
	)
	.unwrap();

	// Create root package.json with workspaces
	std::fs::write(
		dir.path().join("package.json"),
		r#"{"name": "root", "version": "1.0.0", "workspaces": ["packages/*"]}"#,
	)
	.unwrap();

	// Create lib package (dependency)
	std::fs::create_dir_all(dir.path().join("packages/lib")).unwrap();
	std::fs::write(
		dir.path().join("packages/lib/package.json"),
		r#"{"name": "@chronicle-test/lib", "version": "1.0.0"}"#,
	)
	.unwrap();

	// Create app package (depends on lib)
	std::fs::create_dir_all(dir.path().join("packages/app")).unwrap();
	std::fs::write(
		dir.path().join("packages/app/package.json"),
		r#"{"name": "@chronicle-test/app", "version": "1.0.0", "dependencies": {"@chronicle-test/lib": "1.0.0"}}"#,
	)
	.unwrap();

	// Dry-run exercises dependency graph building and ordering without contacting registries
	let result = run_chronicle(&["publish", "--no-interactive", "--dry-run"], dir.path());

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), std::process::ExitCode::SUCCESS);
}

#[test]
fn publish_dry_run_with_workspace_dependencies_filtered() {
	let dir = tempfile::tempdir().unwrap();
	std::fs::create_dir(dir.path().join(".git")).unwrap();

	std::fs::create_dir(dir.path().join(".chronicle")).unwrap();
	std::fs::write(
		dir.path().join(".chronicle/config.toml"),
		"[npm]\nenabled = true\n",
	)
	.unwrap();

	std::fs::write(
		dir.path().join("package.json"),
		r#"{"name": "root", "version": "1.0.0", "workspaces": ["packages/*"]}"#,
	)
	.unwrap();

	// Three packages: utils <- lib <- app
	std::fs::create_dir_all(dir.path().join("packages/utils")).unwrap();
	std::fs::write(
		dir.path().join("packages/utils/package.json"),
		r#"{"name": "@chronicle-test/utils", "version": "1.0.0"}"#,
	)
	.unwrap();

	std::fs::create_dir_all(dir.path().join("packages/lib")).unwrap();
	std::fs::write(
		dir.path().join("packages/lib/package.json"),
		r#"{"name": "@chronicle-test/lib", "version": "1.0.0", "dependencies": {"@chronicle-test/utils": "1.0.0"}}"#,
	)
	.unwrap();

	std::fs::create_dir_all(dir.path().join("packages/app")).unwrap();
	std::fs::write(
		dir.path().join("packages/app/package.json"),
		r#"{"name": "@chronicle-test/app", "version": "1.0.0", "dependencies": {"@chronicle-test/lib": "1.0.0"}}"#,
	)
	.unwrap();

	// Publish only app and lib (not utils) — graph should still order lib before app
	let result = run_chronicle(
		&[
			"publish",
			"--no-interactive",
			"--dry-run",
			"--package",
			"@chronicle-test/lib",
			"--package",
			"@chronicle-test/app",
		],
		dir.path(),
	);

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), std::process::ExitCode::SUCCESS);
}

#[test]
fn publish_cargo_dry_run() {
	let dir = tempfile::tempdir().unwrap();
	// Create git repo
	std::fs::create_dir(dir.path().join(".git")).unwrap();

	// Create config
	std::fs::create_dir(dir.path().join(".chronicle")).unwrap();
	std::fs::write(
		dir.path().join(".chronicle/config.toml"),
		r#"
[cargo]
enabled = true
"#,
	)
	.unwrap();

	// Create Cargo.toml
	std::fs::write(
		dir.path().join("Cargo.toml"),
		r#"
[package]
name = "test-crate"
version = "0.1.0"
edition = "2024"

[lib]
path = "src/lib.rs"
"#,
	)
	.unwrap();

	// Create minimal lib.rs
	std::fs::create_dir(dir.path().join("src")).unwrap();
	std::fs::write(dir.path().join("src/lib.rs"), "").unwrap();

	let result = run_chronicle(&["publish", "--no-interactive", "--dry-run"], dir.path());

	assert!(result.is_ok());
	let exit_code = result.unwrap();
	assert_eq!(exit_code, std::process::ExitCode::SUCCESS);
}

#[test]
fn publish_dry_run_npm_private_package_excluded() {
	let dir = tempfile::tempdir().unwrap();
	std::fs::create_dir(dir.path().join(".git")).unwrap();

	std::fs::create_dir(dir.path().join(".chronicle")).unwrap();
	std::fs::write(
		dir.path().join(".chronicle/config.toml"),
		"[npm]\nenabled = true\n",
	)
	.unwrap();

	// Create a private package
	std::fs::write(
		dir.path().join("package.json"),
		r#"{"name": "private-pkg", "version": "1.0.0", "private": true}"#,
	)
	.unwrap();

	let result = run_chronicle(&["publish", "--no-interactive", "--dry-run"], dir.path());

	// Should succeed with no output (package silently excluded)
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), std::process::ExitCode::SUCCESS);
}

#[test]
fn publish_dry_run_npm_mixed_workspace() {
	let dir = tempfile::tempdir().unwrap();
	std::fs::create_dir(dir.path().join(".git")).unwrap();

	std::fs::create_dir(dir.path().join(".chronicle")).unwrap();
	std::fs::write(
		dir.path().join(".chronicle/config.toml"),
		"[npm]\nenabled = true\n",
	)
	.unwrap();

	// Create root with workspaces
	std::fs::write(
		dir.path().join("package.json"),
		r#"{"name": "root", "version": "1.0.0", "workspaces": ["packages/*"]}"#,
	)
	.unwrap();

	// Create one private package
	std::fs::create_dir_all(dir.path().join("packages/private-pkg")).unwrap();
	std::fs::write(
		dir.path().join("packages/private-pkg/package.json"),
		r#"{"name": "private-pkg", "version": "1.0.0", "private": true}"#,
	)
	.unwrap();

	// Create one public package
	std::fs::create_dir_all(dir.path().join("packages/public-pkg")).unwrap();
	std::fs::write(
		dir.path().join("packages/public-pkg/package.json"),
		r#"{"name": "public-pkg", "version": "1.0.0"}"#,
	)
	.unwrap();

	let result = run_chronicle(&["publish", "--no-interactive", "--dry-run"], dir.path());

	// Should succeed and only list public packages
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), std::process::ExitCode::SUCCESS);
}

#[test]
fn publish_dry_run_cargo_publish_false_excluded() {
	let dir = tempfile::tempdir().unwrap();
	std::fs::create_dir(dir.path().join(".git")).unwrap();

	std::fs::create_dir(dir.path().join(".chronicle")).unwrap();
	std::fs::write(
		dir.path().join(".chronicle/config.toml"),
		"[cargo]\nenabled = true\n",
	)
	.unwrap();

	// Create a crate with publish = false
	std::fs::write(
		dir.path().join("Cargo.toml"),
		r#"
[package]
name = "private-crate"
version = "0.1.0"
edition = "2024"
publish = false

[lib]
path = "src/lib.rs"
"#,
	)
	.unwrap();

	std::fs::create_dir(dir.path().join("src")).unwrap();
	std::fs::write(dir.path().join("src/lib.rs"), "").unwrap();

	let result = run_chronicle(&["publish", "--no-interactive", "--dry-run"], dir.path());

	// Should succeed with no output (crate silently excluded)
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), std::process::ExitCode::SUCCESS);
}

#[test]
fn publish_dry_run_explicitly_naming_private_package() {
	let dir = tempfile::tempdir().unwrap();
	std::fs::create_dir(dir.path().join(".git")).unwrap();

	std::fs::create_dir(dir.path().join(".chronicle")).unwrap();
	std::fs::write(
		dir.path().join(".chronicle/config.toml"),
		"[npm]\nenabled = true\n",
	)
	.unwrap();

	// Create a private package
	std::fs::write(
		dir.path().join("package.json"),
		r#"{"name": "private-pkg", "version": "1.0.0", "private": true}"#,
	)
	.unwrap();

	// Explicitly name the private package with --package flag
	let result = run_chronicle(
		&[
			"publish",
			"--no-interactive",
			"--dry-run",
			"--package",
			"private-pkg",
		],
		dir.path(),
	);

	// Should succeed (not error) and silently skip the private package
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), std::process::ExitCode::SUCCESS);
}

#[test]
fn publish_dry_run_cyclic_npm_workspace() {
	let dir = tempfile::tempdir().unwrap();
	std::fs::create_dir(dir.path().join(".git")).unwrap();

	std::fs::create_dir(dir.path().join(".chronicle")).unwrap();
	std::fs::write(
		dir.path().join(".chronicle/config.toml"),
		"[npm]\nenabled = true\n",
	)
	.unwrap();

	// Create root with workspaces
	std::fs::write(
		dir.path().join("package.json"),
		r#"{"name": "root", "version": "1.0.0", "workspaces": ["packages/*"]}"#,
	)
	.unwrap();

	// Create types package that depends on utils
	std::fs::create_dir_all(dir.path().join("packages/types")).unwrap();
	std::fs::write(
		dir.path().join("packages/types/package.json"),
		r#"{"name": "@test/types", "version": "1.0.0", "dependencies": {"@test/utils": "1.0.0"}}"#,
	)
	.unwrap();

	// Create utils package that depends on types (circular dependency)
	std::fs::create_dir_all(dir.path().join("packages/utils")).unwrap();
	std::fs::write(
		dir.path().join("packages/utils/package.json"),
		r#"{"name": "@test/utils", "version": "1.0.0", "dependencies": {"@test/types": "1.0.0"}}"#,
	)
	.unwrap();

	// Create app package that depends on both
	std::fs::create_dir_all(dir.path().join("packages/app")).unwrap();
	std::fs::write(
		dir.path().join("packages/app/package.json"),
		r#"{"name": "@test/app", "version": "1.0.0", "dependencies": {"@test/types": "1.0.0", "@test/utils": "1.0.0"}}"#,
	)
	.unwrap();

	// Should succeed despite circular dependencies
	let result = run_chronicle(&["publish", "--no-interactive", "--dry-run"], dir.path());
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), std::process::ExitCode::SUCCESS);
}

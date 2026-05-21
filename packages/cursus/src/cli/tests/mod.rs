mod change;
mod ci;
mod verify;

use crate::cli::*;

#[test]
fn global_args_default() {
	let args = GlobalArgs::default();
	assert!(args.interactive);
	assert!(!args.no_interactive);
	assert_eq!(args.verbose, 0);
	assert!(!args.silent);
	assert!(!args.dry_run);
}

#[test]
fn dry_run_flag_sets_true() {
	let cli = Cli::try_parse_from(["cursus", "--dry-run"]).unwrap();
	assert!(cli.global.dry_run);
}

#[test]
fn dry_run_short_flag_sets_true() {
	let cli = Cli::try_parse_from(["cursus", "-n"]).unwrap();
	assert!(cli.global.dry_run);
}

#[test]
fn dry_run_flag_after_subcommand_sets_true() {
	let cli = Cli::try_parse_from(["cursus", "prepare", "--dry-run"]).unwrap();
	assert!(cli.global.dry_run);
}

#[test]
fn dry_run_short_flag_after_subcommand_sets_true() {
	let cli = Cli::try_parse_from(["cursus", "prepare", "-n"]).unwrap();
	assert!(cli.global.dry_run);
}

#[test]
fn verbose_flag_sets_count() {
	let cli = Cli::try_parse_from(["cursus", "-v"]).unwrap();
	assert_eq!(cli.global.verbose, 1);
}

#[test]
fn verbose_flag_stacks() {
	let cli = Cli::try_parse_from(["cursus", "-vv"]).unwrap();
	assert_eq!(cli.global.verbose, 2);
}

#[test]
fn silent_flag_sets_true() {
	let cli = Cli::try_parse_from(["cursus", "-s"]).unwrap();
	assert!(cli.global.silent);
}

#[test]
fn verbose_flag_three_sets_count_three() {
	// Three or more -v flags all map to Trace in determine_log_level; this
	// test documents that the u8 count saturates safely and keeps counting.
	let cli = Cli::try_parse_from(["cursus", "-vvv"]).unwrap();
	assert_eq!(cli.global.verbose, 3);
}

#[test]
fn verbose_and_silent_conflict() {
	let result = Cli::try_parse_from(["cursus", "-v", "-s"]);
	assert!(result.is_err());
}

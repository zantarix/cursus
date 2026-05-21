use crate::cli::ci::*;

#[test]
fn ci_args_default() {
	let args = CiArgs::default();
	assert!(args.packages.is_empty());
	assert!(args.branch.is_none());
	assert!(!args.no_git);
}

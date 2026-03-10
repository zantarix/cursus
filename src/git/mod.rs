//! Git primitives for Chronicle.

mod config;
mod operations;

pub use config::{DEFAULT_RELEASE_BRANCH_PREFIX, GitConfig, Strategy, TagFormat};
pub(crate) use operations::GitWorkdir;

#[cfg(test)]
mod tests {
	use semver::Version;

	use super::*;

	#[test]
	fn tag_format_auto_single_package() {
		let version: Version = "1.2.3".parse().unwrap();
		assert_eq!(TagFormat::Auto.tag("my-pkg", &version, false), "v1.2.3");
	}

	#[test]
	fn tag_format_auto_multi_package() {
		let version: Version = "1.2.3".parse().unwrap();
		assert_eq!(
			TagFormat::Auto.tag("my-pkg", &version, true),
			"my-pkg@1.2.3"
		);
	}

	#[test]
	fn tag_format_prefixed_single_package() {
		let version: Version = "1.2.3".parse().unwrap();
		assert_eq!(
			TagFormat::Prefixed.tag("my-pkg", &version, false),
			"my-pkg@1.2.3"
		);
	}

	#[test]
	fn tag_format_prefixed_multi_package() {
		let version: Version = "1.2.3".parse().unwrap();
		assert_eq!(
			TagFormat::Prefixed.tag("my-pkg", &version, true),
			"my-pkg@1.2.3"
		);
	}

	#[test]
	fn tag_format_simple_single_package() {
		let version: Version = "1.2.3".parse().unwrap();
		assert_eq!(TagFormat::Simple.tag("my-pkg", &version, false), "v1.2.3");
	}

	#[test]
	fn tag_format_simple_multi_package() {
		let version: Version = "1.2.3".parse().unwrap();
		assert_eq!(TagFormat::Simple.tag("my-pkg", &version, true), "v1.2.3");
	}
}

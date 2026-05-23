//! Git tag creation and pushing for published packages.

use log::{info, warn};

use crate::git::Git;
use crate::model::config::Config;

use super::PublishedPackage;

/// Creates and pushes an annotated git tag for each published package serially.
///
/// Tags that already exist in the repository are skipped (making the operation idempotent).
/// Each tag is pushed immediately after creation. If a push fails, the local tag is deleted
/// so that a retry can re-create and re-push it, then processing continues with the next tag.
///
/// Returns `(tags_created, tags_skipped, tags_push_failed)`.
pub(super) async fn create_and_push_tags(
	published: &[PublishedPackage],
	config: &Config,
	git: &dyn Git,
	is_multi_package: bool,
) -> anyhow::Result<(usize, usize, usize)> {
	let mut created = 0;
	let mut skipped = 0;
	let mut push_failed = 0;

	for pkg in published {
		let tag = config
			.git
			.tag_format
			.tag(&pkg.name, &pkg.version, is_multi_package);

		if git.tag_exists(&tag).await? {
			info!("Tag {tag} already exists, skipping");
			skipped += 1;
			continue;
		}

		let message = format!("Release {} version {}", pkg.name, pkg.version);
		// Tag creation failure is propagated as a hard error: an `Err` from `git tag -a`
		// typically indicates a deeper problem (corrupt repo, permission denied) that
		// cannot be resolved by retrying. Push failures, by contrast, are often transient
		// and are handled with best-effort cleanup so retries can succeed.
		git.tag(&tag, &message).await?;
		info!("Created tag {tag}");

		if let Err(e) = git.push_tag(&tag).await {
			warn!("Failed to push tag {tag}: {e:#}");
			if let Err(del_err) = git.delete_tag(&tag).await {
				warn!("Failed to delete local tag {tag} after push failure: {del_err:#}");
			}
			push_failed += 1;
		} else {
			created += 1;
		}
	}

	if created > 0 {
		info!(
			"Pushed {} tag{} to origin",
			created,
			if created == 1 { "" } else { "s" }
		);
	}

	Ok((created, skipped, push_failed))
}

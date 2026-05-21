use gitlab::api::ApiError;

use crate::forge::gitlab::client::*;
use crate::forge::gitlab::remote::GitLabProject;

#[test]
fn sanitize_package_version_replaces_at() {
	assert_eq!(sanitize_package_version("pkg@1.0.0"), "pkg-1.0.0");
}

#[test]
fn sanitize_package_version_keeps_allowed_chars() {
	assert_eq!(
		sanitize_package_version("v1.0.0-rc.1+build"),
		"v1.0.0-rc.1+build"
	);
}

#[test]
fn sanitize_file_name_replaces_disallowed() {
	assert_eq!(sanitize_file_name("name with spaces"), "name-with-spaces");
}

#[test]
fn sanitize_file_name_keeps_allowed_chars() {
	assert_eq!(
		sanitize_file_name("cursus-x86_64-linux.tar.gz"),
		"cursus-x86_64-linux.tar.gz"
	);
}

#[test]
fn project_path_uses_group_and_project_with_slash() {
	let project = GitLabProject::new("gitlab.example.com", "acme/sub", "app").unwrap();
	assert_eq!(
		format!("{}/{}", project.group, project.project),
		"acme/sub/app"
	);
}

#[test]
fn package_file_url_encodes_slashes_in_project_path() {
	let project = GitLabProject::new("gitlab.example.com", "acme/sub", "app").unwrap();
	let path = format!("{}/{}", project.group, project.project);
	let encoded = percent_encode_path(&path);
	assert_eq!(encoded, "acme%2Fsub%2Fapp");
}

#[test]
fn is_not_found_matches_404() {
	let err: ApiError<std::io::Error> = ApiError::GitlabWithStatus {
		status: 404u16.try_into().unwrap(),
		msg: "Not Found".to_string(),
	};
	assert!(is_not_found(&err));
}

#[test]
fn is_not_found_rejects_other_statuses() {
	let err: ApiError<std::io::Error> = ApiError::GitlabWithStatus {
		status: 500u16.try_into().unwrap(),
		msg: "boom".to_string(),
	};
	assert!(!is_not_found(&err));
}

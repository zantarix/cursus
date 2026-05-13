//! GitHub forge implementation.
//!
//! Provides remote URL parsing, the [`GitHubRepo`] identity type, and the
//! production [`OctocrabGitHubClient`] that implements
//! [`crate::forge::CodeForgeClient`].

mod octocrab_client;
pub mod remote;

pub use octocrab_client::OctocrabGitHubClient;
pub use remote::GitHubRepo;

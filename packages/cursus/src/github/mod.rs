//! GitHub Releases integration.
//!
//! Provides remote URL parsing and an abstract client trait
//! for creating GitHub Releases as a post-publish action.

pub mod client;
mod octocrab_client;
pub mod remote;

pub use client::PullRequest;
pub use octocrab_client::OctocrabGitHubClient;
pub use remote::GitHubRepo;

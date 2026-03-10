//! GitHub Releases integration.
//!
//! Provides configuration, remote URL parsing, and an abstract client trait
//! for creating GitHub Releases as a post-publish action.

pub mod client;
mod config;
pub mod remote;
pub mod rest;

pub use config::{DEFAULT_PR_TITLE, GitHubConfig};
pub use remote::GitHubRepo;
pub use rest::RestGitHubClient;

//! GitHub Releases integration.
//!
//! Provides remote URL parsing and an abstract client trait
//! for creating GitHub Releases as a post-publish action.

pub mod client;
pub mod remote;
pub mod rest;

pub use client::PullRequest;
pub use remote::GitHubRepo;
pub use rest::RestGitHubClient;

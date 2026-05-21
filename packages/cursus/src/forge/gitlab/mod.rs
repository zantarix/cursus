//! GitLab forge implementation.
//!
//! Provides remote URL parsing, the [`GitLabProject`] identity type, and the
//! production [`ReqwestGitLabClient`] that implements
//! [`crate::forge::CodeForgeClient`].
//!
//! Per ADR-056, user-visible error messages and log lines use GitLab's
//! native vocabulary ("merge request", "GitLab project", "group/project");
//! the trait, parameter names, and shared types stay forge-neutral.

mod client;
pub mod remote;

pub use client::{GitLabTokenKind, ReqwestGitLabClient};
pub use remote::GitLabProject;

#[cfg(test)]
mod tests;

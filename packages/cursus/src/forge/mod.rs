//! Code forge integration.
//!
//! Defines a forge-neutral [`CodeForgeClient`] trait and shared types
//! ([`PullRequest`], [`ExistingRelease`]) used by every concrete forge
//! implementation. Each per-forge submodule provides its own client struct,
//! remote URL parser, and identity type:
//!
//! * [`github`] — production GitHub client built on `octocrab`.
//!
//! User-facing vocabulary is the responsibility of each implementation
//! (per ADR-056). The trait, its parameter names, and shared types stay
//! forge-neutral; per-forge log lines and error messages translate at the
//! API boundary.

mod client;
pub mod github;
pub mod gitlab;

pub use client::{CodeForgeClient, ExistingRelease, PullRequest};

#[cfg(any(test, feature = "test-support"))]
pub use client::test_support;

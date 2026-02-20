//! Terminal user interface (TUI) wizards for interactive workflows.
//!
//! This module provides TUI screens for guided setup and change recording
//! using ratatui and crossterm. Each wizard follows a state machine pattern
//! with a `Screen` enum, pure `handle_key()` functions for state transitions,
//! and separate rendering functions.

pub mod change;
pub mod init;

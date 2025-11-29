//! CLI command implementations for smgrep.
//!
//! This module contains all subcommand implementations for the smgrep CLI tool.
//! Each module corresponds to a specific command available to users.

pub mod claude_install;
pub mod clean;
pub mod daemon;
pub mod doctor;
pub mod index;
pub mod list;
pub mod mcp;
pub mod search;
pub mod serve;
pub mod setup;
pub mod status;
pub mod stop;
pub mod stop_all;

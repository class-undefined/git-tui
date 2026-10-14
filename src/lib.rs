//! Git operations with a Git 1.8.3.1-compatible command surface.

#[cfg(not(unix))]
compile_error!("git-tui currently supports Linux and macOS");

pub mod git;

//! nv: a local, offline knowledge store for Claude Code.

pub mod cli;
pub mod clock;
pub mod commitments;
pub mod config;
pub mod db;
pub mod knowledge;
pub mod search;

#[cfg(test)]
mod test_support;

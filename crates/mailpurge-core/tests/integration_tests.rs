//! Consolidated integration test runner for `mailpurge-core`.
//!
//! Groups all integration test modules into a single test executable
//! to prevent multiplying linked binaries, saving gigabytes of disk space
//! and drastically reducing link times.

mod suite;

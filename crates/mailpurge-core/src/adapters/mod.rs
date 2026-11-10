//! Adapters layer (Concrete Implementations of Ports).
//!
//! Handles external infrastructure: IMAP networking, SQLite persistence,
//! HTTP RFC 8058 one-click unsubscribe, and OS Keyring secret storage.

/// DNS resolution adapter using Hickory resolver.
pub mod hickory_dns_adapter;

/// Community rules updater adapter.
pub mod community_rules_updater;

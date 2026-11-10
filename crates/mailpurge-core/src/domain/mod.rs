//! Domain layer (Pure core business logic).
//!
//! Contains entities, value objects, domain rules, and safety classifiers.
//! Strictly independent from I/O, network, database, or Tauri IPC dependencies.

/// Community rules definitions and built-in seeds for ESPs and suspicious TLDs.
pub mod community_rules;
/// Domain entities and value objects for orphan spam detection and intelligence.
pub mod orphan_spam;
/// Retention policy calculations.
pub mod retention;
/// Trusted domains allowlist and institutional validation.
pub mod trusted_domains;

//! `MailPurge` Core library.
//!
//! Provides the core domain logic, ports, and adapters for email analysis,
//! classification, and purge candidate management.

pub mod adapters;
/// `OAuth2` PKCE and OS Keyring authentication module.
pub mod auth;
pub mod domain;
pub mod ports;

pub use open;

// Legacy modules awaiting full hexagonal decoupling:
#[allow(missing_docs, clippy::all, clippy::pedantic, clippy::nursery)]
pub mod classifier;
#[allow(missing_docs, clippy::all, clippy::pedantic, clippy::nursery)]
pub mod domain_extractor;
#[allow(missing_docs, clippy::all, clippy::pedantic, clippy::nursery)]
pub mod folder_scanner;
#[allow(missing_docs, clippy::all, clippy::pedantic, clippy::nursery)]
pub mod imap_provider;
#[allow(missing_docs, clippy::all, clippy::pedantic, clippy::nursery)]
pub mod imap_sequence;
#[allow(missing_docs, clippy::all, clippy::pedantic, clippy::nursery)]
pub mod mock_provider;
#[allow(missing_docs, clippy::all, clippy::pedantic, clippy::nursery)]
pub mod personal_domains;
#[allow(missing_docs, clippy::all, clippy::pedantic, clippy::nursery)]
pub mod provider_port;
#[allow(missing_docs, clippy::all, clippy::pedantic, clippy::nursery)]
pub mod purge_executor;
#[allow(missing_docs, clippy::all, clippy::pedantic, clippy::nursery)]
pub mod spam_analyzer;
#[allow(missing_docs, clippy::all, clippy::pedantic, clippy::nursery)]
pub mod storage;
#[allow(missing_docs, clippy::all, clippy::pedantic, clippy::nursery)]
pub mod unsubscribe;

/// Returns the current core library version.
#[must_use]
pub const fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub mod classifier;
pub mod domain_extractor;
pub mod folder_scanner;
pub mod imap_provider;
pub mod imap_sequence;
pub mod mock_provider;
pub mod provider_port;
pub mod purge_executor;
pub mod storage;

pub fn version() -> &'static str {
    "0.1.0"
}

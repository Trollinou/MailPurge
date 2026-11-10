use crate::domain::orphan_spam::DnsStatus;
use async_trait::async_trait;
use std::collections::HashMap;

/// Abstract hexagonal port defining domain DNS resolution operations.
#[async_trait]
pub trait DnsResolverPort: Send + Sync {
    /// Checks the DNS status of a single root domain, returning its status and recommended TTL.
    async fn check_domain(&self, domain: &str) -> (DnsStatus, i64);

    /// Checks a batch of root domains in parallel, respecting concurrency limits.
    async fn batch_check_domains(&self, domains: &[String]) -> HashMap<String, (DnsStatus, i64)>;
}

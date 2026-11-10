use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::stream::{FuturesUnordered, StreamExt};
use hickory_resolver::TokioResolver;
use tokio::sync::Semaphore;

use crate::domain::orphan_spam::DnsStatus;
use crate::ports::dns_port::DnsResolverPort;

/// DNS resolver adapter using Hickory (formerly Trust-DNS) with OS system config.
pub struct HickoryDnsAdapter {
    resolver: TokioResolver,
    timeout: Duration,
    max_concurrency: usize,
}

impl HickoryDnsAdapter {
    /// Constructs a resolver using the host OS system DNS configuration.
    ///
    /// # Errors
    /// Returns error string if system DNS configuration cannot be read or initialized.
    pub fn from_system_conf() -> Result<Self, String> {
        let builder = TokioResolver::builder_tokio()
            .map_err(|e| format!("Impossible de lire la configuration DNS système: {e}"))?;
        let resolver = builder
            .build()
            .map_err(|e| format!("Impossible d'initialiser le résolveur DNS: {e}"))?;

        Ok(Self {
            resolver,
            timeout: Duration::from_millis(2000),
            max_concurrency: 25,
        })
    }

    /// Constructs a resolver with custom timeout and concurrency limits.
    #[must_use]
    pub fn with_settings(
        resolver: TokioResolver,
        timeout: Duration,
        max_concurrency: usize,
    ) -> Self {
        Self {
            resolver,
            timeout,
            max_concurrency: max_concurrency.max(1),
        }
    }
}

#[async_trait]
impl DnsResolverPort for HickoryDnsAdapter {
    async fn check_domain(&self, domain: &str) -> (DnsStatus, i64) {
        let trimmed = domain.trim().trim_end_matches('.');
        if trimmed.is_empty() {
            return (
                DnsStatus::DeadNxDomain,
                DnsStatus::DeadNxDomain.recommended_ttl_seconds(),
            );
        }

        let query_fut = async {
            // 1. Try MX record first
            match self.resolver.mx_lookup(trimmed).await {
                Ok(lookup) => {
                    if !lookup.answers().is_empty() {
                        return DnsStatus::Alive;
                    }
                }
                Err(e) => {
                    let err_str = e.to_string().to_lowercase();
                    if err_str.contains("nxdomain") || err_str.contains("no such host") {
                        return DnsStatus::DeadNxDomain;
                    }
                }
            }

            // 2. Fallback to A / AAAA lookup
            match self.resolver.ipv4_lookup(trimmed).await {
                Ok(lookup) => {
                    if lookup.answers().is_empty() {
                        DnsStatus::DeadNxDomain
                    } else {
                        DnsStatus::NoMailServer
                    }
                }
                Err(e) => {
                    let err_str = e.to_string().to_lowercase();
                    if err_str.contains("nxdomain") || err_str.contains("no such host") {
                        DnsStatus::DeadNxDomain
                    } else {
                        DnsStatus::TimeoutOrError
                    }
                }
            }
        };

        let status = tokio::time::timeout(self.timeout, query_fut)
            .await
            .unwrap_or_else(|_| {
                tracing::debug!(domain = trimmed, "DNS lookup timeout");
                DnsStatus::TimeoutOrError
            });

        let ttl = status.recommended_ttl_seconds();
        (status, ttl)
    }

    async fn batch_check_domains(&self, domains: &[String]) -> HashMap<String, (DnsStatus, i64)> {
        let semaphore = Arc::new(Semaphore::new(self.max_concurrency));
        let mut tasks = FuturesUnordered::new();

        for domain in domains {
            let sem = Arc::clone(&semaphore);
            let d = domain.clone();

            tasks.push(async move {
                let _permit = sem.acquire().await;
                let res = self.check_domain(&d).await;
                (d, res)
            });
        }

        let mut results = HashMap::with_capacity(domains.len());
        while let Some((d, res)) = tasks.next().await {
            results.insert(d, res);
        }

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_empty_domain_is_dead() {
        if let Ok(adapter) = HickoryDnsAdapter::from_system_conf() {
            let (status, _) = adapter.check_domain("").await;
            assert_eq!(status, DnsStatus::DeadNxDomain);
        }
    }
}

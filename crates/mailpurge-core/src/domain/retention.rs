//! Domain Value Object: Retention Policy.
//!
//! Enforces business invariants on email age thresholds before eligibility for purge.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Maximum allowed retention days (10 years).
pub const MAX_RETENTION_DAYS: u16 = 3650;

/// Number of seconds in a standard calendar day.
const SECONDS_PER_DAY: i64 = 86_400;

/// Errors that can occur when instantiating or computing with a [`RetentionPolicy`].
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RetentionPolicyError {
    /// Retention period cannot be zero days (risk of deleting active incoming emails).
    #[error("Retention days must be strictly greater than 0")]
    ZeroDays,

    /// Retention period exceeds the maximum allowable threshold.
    #[error("Retention days ({0}) exceeds maximum allowed ({MAX_RETENTION_DAYS})")]
    ExceedsMaximum(u16),

    /// Computation overflowed during timestamp arithmetic.
    #[error("Timestamp calculation overflowed")]
    TimestampOverflow,
}

/// Strongly typed number of retention days.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RetentionDays(pub u16);

impl RetentionDays {
    /// Returns the raw value in days.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// Domain Value Object representing a validated retention policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetentionPolicy {
    retention_days: RetentionDays,
}

impl RetentionPolicy {
    /// Creates a new validated `RetentionPolicy`.
    ///
    /// # Errors
    ///
    /// Returns [`RetentionPolicyError::ZeroDays`] if `days` is 0.
    /// Returns [`RetentionPolicyError::ExceedsMaximum`] if `days` exceeds [`MAX_RETENTION_DAYS`].
    pub const fn new(days: u16) -> Result<Self, RetentionPolicyError> {
        if days == 0 {
            return Err(RetentionPolicyError::ZeroDays);
        }
        if days > MAX_RETENTION_DAYS {
            return Err(RetentionPolicyError::ExceedsMaximum(days));
        }

        Ok(Self {
            retention_days: RetentionDays(days),
        })
    }

    /// Returns the configured retention threshold in days.
    #[must_use]
    pub const fn days(&self) -> RetentionDays {
        self.retention_days
    }

    /// Checks if a message of age `age_days` is expired according to this policy.
    ///
    /// An email is considered expired only if its age strictly exceeds the retention threshold.
    #[must_use]
    pub const fn is_expired(&self, age_days: u16) -> bool {
        age_days > self.retention_days.get()
    }

    /// Calculates the cutoff Unix timestamp (in seconds) for a given reference timestamp.
    /// Emails received before this timestamp are considered older than the retention threshold.
    ///
    /// # Errors
    ///
    /// Returns [`RetentionPolicyError::TimestampOverflow`] if the arithmetic computation overflows.
    pub fn cutoff_timestamp(&self, reference_timestamp: i64) -> Result<i64, RetentionPolicyError> {
        let retention_seconds = i64::from(self.retention_days.get())
            .checked_mul(SECONDS_PER_DAY)
            .ok_or(RetentionPolicyError::TimestampOverflow)?;

        reference_timestamp
            .checked_sub(retention_seconds)
            .ok_or(RetentionPolicyError::TimestampOverflow)
    }

    /// Calculates the cutoff [`DateTime<Utc>`] for a given reference date.
    /// Emails received before this date are considered older than the retention threshold.
    ///
    /// # Errors
    ///
    /// Returns [`RetentionPolicyError::TimestampOverflow`] if date subtraction fails.
    pub fn cutoff_date(
        &self,
        reference_date: DateTime<Utc>,
    ) -> Result<DateTime<Utc>, RetentionPolicyError> {
        let retention_days = i64::from(self.retention_days.get());
        let duration =
            Duration::try_days(retention_days).ok_or(RetentionPolicyError::TimestampOverflow)?;

        reference_date
            .checked_sub_signed(duration)
            .ok_or(RetentionPolicyError::TimestampOverflow)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use proptest::prelude::*;
    use rstest::rstest;

    #[rstest]
    #[case(30, 31, true)]
    #[case(30, 30, false)]
    #[case(30, 29, false)]
    #[case(90, 91, true)]
    #[case(365, 366, true)]
    fn test_retention_expiry(#[case] threshold: u16, #[case] age: u16, #[case] expected: bool) {
        // In unit tests, unwrap() is permitted and validated via clippy.toml allow-unwrap-in-tests
        let policy = RetentionPolicy::new(threshold).unwrap();
        assert_eq!(policy.is_expired(age), expected);
    }

    #[test]
    fn test_zero_retention_days_rejected() {
        let err = RetentionPolicy::new(0).unwrap_err();
        assert_eq!(err, RetentionPolicyError::ZeroDays);
    }

    #[test]
    fn test_exceeding_max_retention_rejected() {
        let err = RetentionPolicy::new(MAX_RETENTION_DAYS + 1).unwrap_err();
        assert_eq!(
            err,
            RetentionPolicyError::ExceedsMaximum(MAX_RETENTION_DAYS + 1)
        );
    }

    #[rstest]
    #[case(1_000_000_000, 10, 1_000_000_000 - (10 * SECONDS_PER_DAY))]
    #[case(1_700_000_000, 30, 1_700_000_000 - (30 * SECONDS_PER_DAY))]
    fn test_cutoff_timestamp_calculation(
        #[case] reference: i64,
        #[case] days: u16,
        #[case] expected: i64,
    ) {
        let policy = RetentionPolicy::new(days).unwrap();
        let cutoff = policy.cutoff_timestamp(reference).unwrap();
        assert_eq!(cutoff, expected);
    }

    #[test]
    fn test_cutoff_date_calculation() {
        let policy = RetentionPolicy::new(30).unwrap();
        let reference = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let cutoff = policy.cutoff_date(reference).unwrap();
        assert_eq!(cutoff.timestamp(), 1_700_000_000 - (30 * SECONDS_PER_DAY));
    }

    proptest! {
        #[test]
        fn prop_valid_days_always_constructs(days in 1..=MAX_RETENTION_DAYS) {
            let policy = RetentionPolicy::new(days);
            prop_assert!(policy.is_ok());
            let policy = policy.unwrap();
            prop_assert_eq!(policy.days().get(), days);
        }

        #[test]
        fn prop_cutoff_timestamp_is_strictly_smaller(
            days in 1..=MAX_RETENTION_DAYS,
            ref_ts in 1_000_000_000i64..2_000_000_000i64
        ) {
            let policy = RetentionPolicy::new(days).unwrap();
            let cutoff = policy.cutoff_timestamp(ref_ts).unwrap();
            prop_assert!(cutoff < ref_ts);
        }
    }
}

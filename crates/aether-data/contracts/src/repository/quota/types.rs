use async_trait::async_trait;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StoredProviderQuotaSnapshot {
    pub provider_id: String,
    pub billing_type: String,
    pub monthly_quota_usd: Option<f64>,
    pub monthly_used_usd: f64,
    pub quota_reset_day: Option<u64>,
    pub quota_last_reset_at_unix_secs: Option<u64>,
    pub quota_expires_at_unix_secs: Option<u64>,
    pub is_active: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ApplyRemoteProviderQuotaPatch {
    pub provider_id: String,
    pub billing_type: String,
    pub monthly_quota_usd: Option<f64>,
    pub remote_monthly_used_usd: f64,
    pub remote_window_start_unix_secs: u64,
    pub remote_window_end_unix_secs: u64,
    pub quota_reset_day: Option<u64>,
    pub quota_expires_at_unix_secs: Option<u64>,
    /// Preserve the provider's current local usage when only quota state changes.
    pub preserve_local_used_usd: bool,
}

impl ApplyRemoteProviderQuotaPatch {
    pub fn validate(&self) -> Result<(), crate::DataLayerError> {
        if self.provider_id.trim().is_empty() {
            return Err(crate::DataLayerError::InvalidInput(
                "remote provider quota provider_id is empty".to_string(),
            ));
        }
        if !matches!(
            self.billing_type.as_str(),
            "monthly_quota" | "pay_as_you_go"
        ) {
            return Err(crate::DataLayerError::InvalidInput(
                "remote provider quota billing_type is unsupported".to_string(),
            ));
        }
        if self.billing_type == "monthly_quota" && self.monthly_quota_usd.is_none() {
            return Err(crate::DataLayerError::InvalidInput(
                "remote monthly quota limit is missing".to_string(),
            ));
        }
        if self.billing_type == "pay_as_you_go" && self.monthly_quota_usd.is_some() {
            return Err(crate::DataLayerError::InvalidInput(
                "remote unlimited quota must not set a monthly limit".to_string(),
            ));
        }
        if !self.remote_monthly_used_usd.is_finite()
            || self.remote_monthly_used_usd < 0.0
            || self
                .monthly_quota_usd
                .is_some_and(|value| !value.is_finite() || value < 0.0)
        {
            return Err(crate::DataLayerError::InvalidInput(
                "remote provider quota values must be finite and non-negative".to_string(),
            ));
        }
        if self.remote_window_start_unix_secs == 0
            || self.remote_window_end_unix_secs <= self.remote_window_start_unix_secs
        {
            return Err(crate::DataLayerError::InvalidInput(
                "remote provider quota window is invalid".to_string(),
            ));
        }
        if self
            .quota_reset_day
            .is_some_and(|days| !(1..=365).contains(&days))
        {
            return Err(crate::DataLayerError::InvalidInput(
                "remote provider quota reset interval is invalid".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ApplyRemoteProviderQuotaOutcome {
    Applied(StoredProviderQuotaSnapshot),
    StaleWindow(StoredProviderQuotaSnapshot),
    ProviderNotFound,
}

impl ApplyRemoteProviderQuotaOutcome {
    /// Classify a zero-row remote quota UPDATE without mistaking an idempotent
    /// write for success. A row is already applied only when all state fields
    /// match and the usage monotonicity rule is satisfied.
    pub fn from_unapplied_row(
        stored: Option<StoredProviderQuotaSnapshot>,
        patch: &ApplyRemoteProviderQuotaPatch,
    ) -> Result<Self, crate::DataLayerError> {
        let Some(snapshot) = stored else {
            return Ok(Self::ProviderNotFound);
        };
        if snapshot
            .quota_last_reset_at_unix_secs
            .is_some_and(|start| start >= patch.remote_window_end_unix_secs)
        {
            return Ok(Self::StaleWindow(snapshot));
        }

        let already_applied = snapshot.quota_last_reset_at_unix_secs
            == Some(patch.remote_window_start_unix_secs)
            && snapshot.billing_type == patch.billing_type
            && snapshot.monthly_quota_usd == patch.monthly_quota_usd
            && snapshot.quota_reset_day == patch.quota_reset_day
            && snapshot.quota_expires_at_unix_secs == patch.quota_expires_at_unix_secs
            && (patch.preserve_local_used_usd
                || snapshot.monthly_used_usd >= patch.remote_monthly_used_usd);
        if already_applied {
            return Ok(Self::Applied(snapshot));
        }

        Err(crate::DataLayerError::UnexpectedValue(
            "remote provider quota update matched no row without a newer local window".to_string(),
        ))
    }
}

impl StoredProviderQuotaSnapshot {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        provider_id: String,
        billing_type: String,
        monthly_quota_usd: Option<f64>,
        monthly_used_usd: f64,
        quota_reset_day: Option<i32>,
        quota_last_reset_at_unix_secs: Option<i64>,
        quota_expires_at_unix_secs: Option<i64>,
        is_active: bool,
    ) -> Result<Self, crate::DataLayerError> {
        if provider_id.trim().is_empty() || billing_type.trim().is_empty() {
            return Err(crate::DataLayerError::UnexpectedValue(
                "provider quota identity is empty".to_string(),
            ));
        }
        if !monthly_used_usd.is_finite() || monthly_quota_usd.is_some_and(|v| !v.is_finite()) {
            return Err(crate::DataLayerError::UnexpectedValue(
                "provider quota value is not finite".to_string(),
            ));
        }
        Ok(Self {
            provider_id,
            billing_type,
            monthly_quota_usd,
            monthly_used_usd,
            quota_reset_day: quota_reset_day.map(|value| value as u64),
            quota_last_reset_at_unix_secs: quota_last_reset_at_unix_secs.map(|value| value as u64),
            quota_expires_at_unix_secs: quota_expires_at_unix_secs.map(|value| value as u64),
            is_active,
        })
    }
}

#[async_trait]
pub trait ProviderQuotaReadRepository: Send + Sync {
    async fn find_by_provider_id(
        &self,
        provider_id: &str,
    ) -> Result<Option<StoredProviderQuotaSnapshot>, crate::DataLayerError>;

    async fn find_by_provider_ids(
        &self,
        provider_ids: &[String],
    ) -> Result<Vec<StoredProviderQuotaSnapshot>, crate::DataLayerError>;
}

#[async_trait]
pub trait ProviderQuotaWriteRepository: Send + Sync {
    async fn reset_due(&self, now_unix_secs: u64) -> Result<usize, crate::DataLayerError>;

    async fn apply_remote_provider_quota(
        &self,
        patch: &ApplyRemoteProviderQuotaPatch,
    ) -> Result<ApplyRemoteProviderQuotaOutcome, crate::DataLayerError>;
}

pub trait ProviderQuotaRepository:
    ProviderQuotaReadRepository + ProviderQuotaWriteRepository + Send + Sync
{
}

impl<T> ProviderQuotaRepository for T where
    T: ProviderQuotaReadRepository + ProviderQuotaWriteRepository + Send + Sync
{
}

#[cfg(test)]
mod tests {
    use super::{
        ApplyRemoteProviderQuotaOutcome, ApplyRemoteProviderQuotaPatch, StoredProviderQuotaSnapshot,
    };

    fn sample_quota(last_reset: Option<i64>) -> StoredProviderQuotaSnapshot {
        StoredProviderQuotaSnapshot::new(
            "provider-1".to_string(),
            "monthly_quota".to_string(),
            Some(100.0),
            4.0,
            Some(30),
            last_reset,
            None,
            true,
        )
        .expect("quota should build")
    }

    fn patch() -> ApplyRemoteProviderQuotaPatch {
        ApplyRemoteProviderQuotaPatch {
            provider_id: "provider-1".to_string(),
            billing_type: "monthly_quota".to_string(),
            monthly_quota_usd: Some(100.0),
            remote_monthly_used_usd: 4.0,
            remote_window_start_unix_secs: 7_000,
            remote_window_end_unix_secs: 8_000,
            quota_reset_day: Some(30),
            quota_expires_at_unix_secs: None,
            preserve_local_used_usd: false,
        }
    }

    #[test]
    fn unapplied_row_classifies_stale_missing_idempotent_and_failed() {
        assert!(matches!(
            ApplyRemoteProviderQuotaOutcome::from_unapplied_row(None, &patch())
                .expect("missing provider is an outcome"),
            ApplyRemoteProviderQuotaOutcome::ProviderNotFound
        ));
        assert!(matches!(
            ApplyRemoteProviderQuotaOutcome::from_unapplied_row(
                Some(sample_quota(Some(8_000))),
                &patch(),
            )
            .expect("newer-or-equal local window is stale"),
            ApplyRemoteProviderQuotaOutcome::StaleWindow(_)
        ));
        assert!(matches!(
            ApplyRemoteProviderQuotaOutcome::from_unapplied_row(
                Some(sample_quota(Some(7_000))),
                &patch(),
            )
            .expect("an idempotent row is already applied"),
            ApplyRemoteProviderQuotaOutcome::Applied(_)
        ));
        let error = ApplyRemoteProviderQuotaOutcome::from_unapplied_row(
            Some(sample_quota(Some(1_000))),
            &patch(),
        )
        .expect_err("non-stale zero-row must not look applied");
        assert!(
            error
                .to_string()
                .contains("matched no row without a newer local window"),
            "unexpected error: {error}"
        );
    }
}

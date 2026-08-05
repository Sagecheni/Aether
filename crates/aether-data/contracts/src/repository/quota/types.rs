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

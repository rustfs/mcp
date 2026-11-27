// Copyright 2024 RustFS Team
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Bucket Object Lock configuration and per-version retention and legal hold.

mod get_bucket_lock_configuration;
mod get_object_legal_hold;
mod get_object_retention;
mod put_bucket_lock_configuration;
mod put_object_legal_hold;
mod put_object_retention;

pub use get_bucket_lock_configuration::BucketObjectLockConfig;
pub use get_object_legal_hold::ObjectLegalHold;
pub use get_object_retention::ObjectRetentionInfo;

use anyhow::{Context, Result};
use aws_sdk_s3::error::{ProvideErrorMetadata, SdkError};
use aws_sdk_s3::primitives::{DateTime, DateTimeFormat};
use serde::{Deserialize, Serialize};

use super::S3Client;

/// Error code every backend answers with when an object version has no
/// retention period or legal hold to report.
const UNSET_OBJECT_LOCK_ERROR_CODE: &str = "NoSuchObjectLockConfiguration";

/// Error code every backend answers with when an object-level lock setting
/// is read from a bucket that has Object Lock disabled. The backends word
/// the message differently, and RustFS uses two wordings for two states, so
/// the code is all that is matched: whether the bucket really has Object
/// Lock disabled is then confirmed by reading its configuration.
const LOCK_DISABLED_ERROR_CODE: &str = "InvalidRequest";

/// Why no retention period or legal hold applies to an object version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnsetLockReason {
    /// The version carries no such setting.
    NotSet,
    /// The bucket has Object Lock disabled, so no version of it can carry
    /// one.
    BucketNotEnabled,
}

/// The answer to an object-level lock read.
#[derive(Debug)]
enum ObjectLockRead<T> {
    /// The backend answered the request; its body may still be empty.
    Answered(T),
    /// Nothing applies to the version, which the backend said with an error
    /// instead of an empty body.
    Unset(UnsetLockReason),
}

/// What a failed object-level lock read means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LockReadFailure {
    /// The version carries no such setting.
    Unset,
    /// The backend refused the read. A bucket without Object Lock is the
    /// one reason for this answer that means "nothing to report".
    MaybeBucketNotEnabled,
    /// Any other failure, which stays one.
    Other,
}

/// Classify a failed object-level lock read by its error code alone.
fn classify_lock_read_failure<E, R>(err: &SdkError<E, R>) -> LockReadFailure
where
    E: ProvideErrorMetadata,
{
    match err.code() {
        Some(UNSET_OBJECT_LOCK_ERROR_CODE) => LockReadFailure::Unset,
        Some(LOCK_DISABLED_ERROR_CODE) => LockReadFailure::MaybeBucketNotEnabled,
        _ => LockReadFailure::Other,
    }
}

/// Whether the Object Lock configuration of a bucket, as
/// [`S3Client::get_bucket_lock_configuration`] reports it, shows Object Lock
/// disabled: `Some(false)` when it is read and disabled, `Some(true)` when
/// it is read and enabled, and `None` when it could not be read at all.
fn object_lock_enabled(config: &Result<BucketObjectLockConfig>) -> Option<bool> {
    config
        .as_ref()
        .ok()
        .map(|config| config.object_lock_enabled())
}

impl S3Client {
    /// Read an object-level lock setting (retention or legal hold), mapping
    /// the answers that mean "nothing applies to this version" to
    /// [`ObjectLockRead::Unset`] instead of a failure. `failure` starts the
    /// message of every error that stays one.
    ///
    /// Every backend answers `NoSuchObjectLockConfiguration` for a version
    /// that carries no such setting. For a bucket that has Object Lock
    /// disabled they all answer `InvalidRequest`, worded differently: AWS S3
    /// says "Bucket is missing Object Lock Configuration", MinIO and RustFS
    /// say "Bucket is missing ObjectLockConfiguration", and RustFS also says
    /// "Object Lock is not enabled for this bucket" for a stored
    /// configuration that is not enabled (rustfs/rustfs#9654). So the error
    /// code decides what to check and the message is never matched: the
    /// bucket's Object Lock configuration is read, and only a bucket that is
    /// confirmed to have Object Lock disabled turns the refusal into
    /// "nothing applies".
    async fn read_object_lock_setting<E, R, T>(
        &self,
        result: Result<T, SdkError<E, R>>,
        bucket_name: &str,
        failure: &str,
    ) -> Result<ObjectLockRead<T>>
    where
        E: ProvideErrorMetadata + std::error::Error + Send + Sync + 'static,
        R: std::fmt::Debug + Send + Sync + 'static,
    {
        let err = match result {
            Ok(response) => return Ok(ObjectLockRead::Answered(response)),
            Err(err) => err,
        };
        // Owned, so that it can head an error chain that outlives the call.
        let failure = failure.to_owned();

        match classify_lock_read_failure(&err) {
            LockReadFailure::Unset => Ok(ObjectLockRead::Unset(UnsetLockReason::NotSet)),
            LockReadFailure::MaybeBucketNotEnabled => {
                let config = self.get_bucket_lock_configuration(bucket_name).await;
                match object_lock_enabled(&config) {
                    Some(false) => Ok(ObjectLockRead::Unset(UnsetLockReason::BucketNotEnabled)),
                    // The bucket has Object Lock, so the refusal means
                    // something else and stays an error.
                    Some(true) => Err(err).context(failure),
                    // The Object Lock state of the bucket stays unknown, so
                    // the refusal stays an error; why it could not be read
                    // is part of it.
                    None => Err(err).context(format!(
                        "{failure}. The storage backend refused the read, and the Object Lock \
                         configuration of bucket '{bucket_name}' could not be read to tell why: {}",
                        config.unwrap_err()
                    )),
                }
            }
            LockReadFailure::Other => Err(err).context(failure),
        }
    }
}

/// ISO 8601 form of an optional timestamp, e.g. `2030-01-01T00:00:00Z`.
pub(super) fn iso8601(date: Option<&DateTime>) -> Option<String> {
    date.map(|date| date.fmt(DateTimeFormat::DateTime).unwrap())
}

/// Retention mode of an object version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum ObjectLockRetentionMode {
    /// Users with the `s3:BypassGovernanceRetention` permission can shorten or remove the retention.
    #[serde(rename = "GOVERNANCE")]
    Governance,
    /// Nobody can shorten or remove the retention, not even the root account: it ends only when it expires.
    #[serde(rename = "COMPLIANCE")]
    Compliance,
}

impl ObjectLockRetentionMode {
    /// Upper-case name used in requests and log lines.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Governance => "GOVERNANCE",
            Self::Compliance => "COMPLIANCE",
        }
    }

    /// Who can end the retention before it expires.
    pub fn effect(&self) -> &'static str {
        match self {
            Self::Governance => {
                "Users with the s3:BypassGovernanceRetention permission can shorten or remove a \
                 GOVERNANCE retention with put_object_retention (bypass_governance_retention)."
            }
            Self::Compliance => {
                "Nobody can shorten or remove a COMPLIANCE retention, not even the root account: \
                 it ends only when it expires."
            }
        }
    }

    /// Map the SDK enum, dropping values this crate does not model.
    fn from_sdk(mode: &aws_sdk_s3::types::ObjectLockRetentionMode) -> Option<Self> {
        match mode {
            aws_sdk_s3::types::ObjectLockRetentionMode::Governance => Some(Self::Governance),
            aws_sdk_s3::types::ObjectLockRetentionMode::Compliance => Some(Self::Compliance),
            _ => None,
        }
    }

    /// Map the mode of a HEAD response, which the SDK models with an enum
    /// of its own.
    pub(super) fn from_sdk_head(mode: &aws_sdk_s3::types::ObjectLockMode) -> Option<Self> {
        match mode {
            aws_sdk_s3::types::ObjectLockMode::Governance => Some(Self::Governance),
            aws_sdk_s3::types::ObjectLockMode::Compliance => Some(Self::Compliance),
            _ => None,
        }
    }

    fn to_sdk(self) -> aws_sdk_s3::types::ObjectLockRetentionMode {
        match self {
            Self::Governance => aws_sdk_s3::types::ObjectLockRetentionMode::Governance,
            Self::Compliance => aws_sdk_s3::types::ObjectLockRetentionMode::Compliance,
        }
    }
}

/// Legal hold status of an object version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum ObjectLockLegalHoldStatus {
    /// The version cannot be deleted or overwritten until the hold is removed.
    #[serde(rename = "ON")]
    On,
    /// The version can be deleted or overwritten.
    #[serde(rename = "OFF")]
    Off,
}

impl ObjectLockLegalHoldStatus {
    /// Upper-case name used in requests and log lines.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::On => "ON",
            Self::Off => "OFF",
        }
    }

    /// What the status means for the object version.
    pub fn effect(&self) -> &'static str {
        match self {
            Self::On => {
                "While the legal hold is on, this version cannot be deleted or overwritten. \
                 Remove it with put_object_legal_hold (status OFF)."
            }
            Self::Off => {
                "This version can be deleted or overwritten unless an active retention \
                 period still protects it (get_object_retention)."
            }
        }
    }

    /// Map the SDK enum, dropping values this crate does not model.
    pub(super) fn from_sdk(status: &aws_sdk_s3::types::ObjectLockLegalHoldStatus) -> Option<Self> {
        match status {
            aws_sdk_s3::types::ObjectLockLegalHoldStatus::On => Some(Self::On),
            aws_sdk_s3::types::ObjectLockLegalHoldStatus::Off => Some(Self::Off),
            _ => None,
        }
    }

    fn to_sdk(self) -> aws_sdk_s3::types::ObjectLockLegalHoldStatus {
        match self {
            Self::On => aws_sdk_s3::types::ObjectLockLegalHoldStatus::On,
            Self::Off => aws_sdk_s3::types::ObjectLockLegalHoldStatus::Off,
        }
    }
}

/// Retention period applied to every new object version of a bucket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DefaultRetention {
    #[schemars(
        description = "Retention mode applied to every new object version: 'GOVERNANCE' (users with s3:BypassGovernanceRetention can shorten or remove it) or 'COMPLIANCE' (cannot be shortened or removed until it expires)"
    )]
    pub mode: ObjectLockRetentionMode,
    /// Retention period in days; exactly one of `days` and `years` is set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Retention period in days (at least 1); set exactly one of days and years"
    )]
    pub days: Option<i32>,
    /// Retention period in years; exactly one of `days` and `years` is set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Retention period in years (at least 1); set exactly one of days and years"
    )]
    pub years: Option<i32>,
}

impl DefaultRetention {
    /// The retention described by three optional fields: `Ok(None)` when all
    /// three are absent, which asks to remove the default retention. Fields
    /// that describe no complete retention are an error.
    pub fn from_optional_fields(
        mode: Option<ObjectLockRetentionMode>,
        days: Option<i32>,
        years: Option<i32>,
    ) -> Result<Option<Self>> {
        match (mode, days, years) {
            (None, None, None) => Ok(None),
            (Some(mode), days, years) => {
                let retention = Self { mode, days, years };
                validate_default_retention(&retention)?;
                Ok(Some(retention))
            }
            (None, _, _) => {
                anyhow::bail!(
                    "Default retention needs a mode together with its period: pass mode as well, or omit mode, days, and years to remove the default retention"
                );
            }
        }
    }

    /// Map the SDK values; `None` when the mode or the period is missing.
    fn from_sdk(retention: &aws_sdk_s3::types::DefaultRetention) -> Option<Self> {
        Some(Self {
            mode: retention
                .mode()
                .and_then(ObjectLockRetentionMode::from_sdk)?,
            days: retention.days(),
            years: retention.years(),
        })
    }

    /// Build the SDK value for a valid retention period.
    fn to_sdk(&self) -> Result<aws_sdk_s3::types::DefaultRetention> {
        validate_default_retention(self)?;

        Ok(aws_sdk_s3::types::DefaultRetention::builder()
            .mode(self.mode.to_sdk())
            .set_days(self.days)
            .set_years(self.years)
            .build())
    }

    /// Readable retention period for tool output, e.g. `30 day(s)`.
    pub fn period_label(&self) -> String {
        match (self.days, self.years) {
            (Some(days), _) => format!("{days} day(s)"),
            (_, Some(years)) => format!("{years} year(s)"),
            _ => "no period".to_string(),
        }
    }

    /// Readable retention period for tool output, e.g. `COMPLIANCE for 30 day(s)`.
    pub fn label(&self) -> String {
        match (self.days, self.years) {
            (Some(_), _) | (_, Some(_)) => {
                format!("{} for {}", self.mode.as_str(), self.period_label())
            }
            _ => self.mode.as_str().to_string(),
        }
    }
}

/// Reject a default retention S3 cannot apply: no period, both periods, or a
/// period shorter than one unit.
pub fn validate_default_retention(retention: &DefaultRetention) -> Result<()> {
    match (retention.days, retention.years) {
        (None, None) => {
            anyhow::bail!("Default retention needs a period: set days or years");
        }
        (Some(days), Some(years)) => {
            anyhow::bail!(
                "Default retention cannot use both days ({days}) and years ({years}); set exactly one of them"
            );
        }
        (Some(days), None) if days < 1 => {
            anyhow::bail!("Default retention period must be at least 1 day; got {days}");
        }
        (None, Some(years)) if years < 1 => {
            anyhow::bail!("Default retention period must be at least 1 year; got {years}");
        }
        _ => Ok(()),
    }
}

/// Object Lock configuration of a bucket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ObjectLockConfiguration {
    #[schemars(
        description = "Whether Object Lock is enabled for the bucket; it cannot be turned off once enabled"
    )]
    pub object_lock_enabled: bool,
    /// Retention applied to every new object version; absent when new
    /// versions are not retained by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Retention period applied to every new object version; absent when new versions are not retained by default"
    )]
    pub default_retention: Option<DefaultRetention>,
}

impl ObjectLockConfiguration {
    /// Copy the SDK values, dropping members this crate does not model.
    fn from_sdk(configuration: &aws_sdk_s3::types::ObjectLockConfiguration) -> Self {
        Self {
            object_lock_enabled: matches!(
                configuration.object_lock_enabled(),
                Some(aws_sdk_s3::types::ObjectLockEnabled::Enabled)
            ),
            default_retention: configuration
                .rule()
                .and_then(|rule| rule.default_retention())
                .and_then(DefaultRetention::from_sdk),
        }
    }
}

/// Retention setting of one object version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ObjectRetention {
    #[schemars(
        description = "Retention mode: 'GOVERNANCE' (users with s3:BypassGovernanceRetention can shorten or remove it) or 'COMPLIANCE' (cannot be shortened or removed until it expires)"
    )]
    pub mode: ObjectLockRetentionMode,
    /// ISO 8601 UTC timestamp until which the version stays retained.
    #[schemars(
        description = "ISO 8601 UTC timestamp until which the version stays retained, e.g. \"2030-01-01T00:00:00Z\""
    )]
    pub retain_until: String,
}

impl ObjectRetention {
    /// Map the SDK values; `None` when the mode or the date is missing,
    /// which is how some S3 implementations report a version without retention.
    fn from_sdk(retention: &aws_sdk_s3::types::ObjectLockRetention) -> Option<Self> {
        Some(Self {
            mode: retention
                .mode()
                .and_then(ObjectLockRetentionMode::from_sdk)?,
            retain_until: iso8601(retention.retain_until_date())?,
        })
    }

    /// Build the SDK value for a well-formed retention that has not ended yet.
    fn to_sdk(&self, now: DateTime) -> Result<aws_sdk_s3::types::ObjectLockRetention> {
        let retain_until = parse_retain_until(&self.retain_until)?;
        if retain_until <= now {
            anyhow::bail!(
                "retain_until '{}' is not in the future; S3 only accepts a retain-until date that lies ahead",
                self.retain_until
            );
        }

        Ok(aws_sdk_s3::types::ObjectLockRetention::builder()
            .mode(self.mode.to_sdk())
            .retain_until_date(retain_until)
            .build())
    }

    /// Whether the version is still retained at `now`. A timestamp that
    /// cannot be parsed counts as not retained.
    pub fn is_active_at(&self, now: DateTime) -> bool {
        matches!(parse_retain_until(&self.retain_until), Ok(until) if until > now)
    }

    /// Whether the version is still retained at the moment of the call.
    pub fn is_active(&self) -> bool {
        self.is_active_at(now())
    }

    /// Readable retention for tool output, e.g. `COMPLIANCE until 2030-01-01T00:00:00Z`.
    pub fn label(&self) -> String {
        format!("{} until {}", self.mode.as_str(), self.retain_until)
    }
}

/// The current moment, to the millisecond.
pub(super) fn now() -> DateTime {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since_epoch| since_epoch.as_millis() as i64)
        // A clock before the epoch leaves the check to the storage backend.
        .unwrap_or_default();
    DateTime::from_millis(millis)
}

/// Parse a retain-until timestamp.
fn parse_retain_until(value: &str) -> Result<DateTime> {
    DateTime::from_str(value, DateTimeFormat::DateTime).with_context(|| {
        format!(
            "Invalid retain_until '{value}': pass an ISO 8601 UTC timestamp such as \"2030-01-01T00:00:00Z\""
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_object_lock_enabled_reads_the_bucket_probe_outcome() {
        let config = |enabled: bool| BucketObjectLockConfig {
            bucket: "b".to_string(),
            configuration: enabled.then_some(ObjectLockConfiguration {
                object_lock_enabled: true,
                default_retention: None,
            }),
        };

        // A bucket without Object Lock, one with it, and one whose
        // configuration could not be read at all.
        assert_eq!(object_lock_enabled(&Ok(config(false))), Some(false));
        assert_eq!(object_lock_enabled(&Ok(config(true))), Some(true));
        assert_eq!(
            object_lock_enabled(&Err(anyhow::anyhow!("AccessDenied"))),
            None
        );

        // A stored configuration that is not enabled counts as disabled.
        let disabled = BucketObjectLockConfig {
            bucket: "b".to_string(),
            configuration: Some(ObjectLockConfiguration {
                object_lock_enabled: false,
                default_retention: None,
            }),
        };
        assert_eq!(object_lock_enabled(&Ok(disabled)), Some(false));
    }

    fn retention(mode: ObjectLockRetentionMode, retain_until: &str) -> ObjectRetention {
        ObjectRetention {
            mode,
            retain_until: retain_until.to_string(),
        }
    }

    fn default_retention(
        mode: ObjectLockRetentionMode,
        days: Option<i32>,
        years: Option<i32>,
    ) -> DefaultRetention {
        DefaultRetention { mode, days, years }
    }

    fn timestamp(value: &str) -> DateTime {
        DateTime::from_str(value, DateTimeFormat::DateTime).unwrap()
    }

    #[test]
    fn test_object_lock_enums_serialization() {
        assert_eq!(
            serde_json::to_string(&ObjectLockRetentionMode::Governance).unwrap(),
            r#""GOVERNANCE""#
        );
        assert_eq!(
            serde_json::to_string(&ObjectLockRetentionMode::Compliance).unwrap(),
            r#""COMPLIANCE""#
        );
        assert_eq!(
            serde_json::to_string(&ObjectLockLegalHoldStatus::On).unwrap(),
            r#""ON""#
        );
        assert_eq!(
            serde_json::to_string(&ObjectLockLegalHoldStatus::Off).unwrap(),
            r#""OFF""#
        );

        let mode: ObjectLockRetentionMode = serde_json::from_str(r#""COMPLIANCE""#).unwrap();
        let status: ObjectLockLegalHoldStatus = serde_json::from_str(r#""OFF""#).unwrap();
        assert_eq!(mode, ObjectLockRetentionMode::Compliance);
        assert_eq!(status, ObjectLockLegalHoldStatus::Off);

        // S3 compares both values exactly, so other spellings are rejected.
        for value in [r#""governance""#, r#""Governance""#, r#""STRONG""#] {
            assert!(serde_json::from_str::<ObjectLockRetentionMode>(value).is_err());
        }
        for value in [r#""on""#, r#""Off""#, r#""MAYBE""#] {
            assert!(serde_json::from_str::<ObjectLockLegalHoldStatus>(value).is_err());
        }
    }

    #[test]
    fn test_object_lock_enums_sdk_round_trip() {
        for mode in [
            ObjectLockRetentionMode::Governance,
            ObjectLockRetentionMode::Compliance,
        ] {
            assert_eq!(
                ObjectLockRetentionMode::from_sdk(&mode.to_sdk()),
                Some(mode)
            );
        }
        for status in [
            ObjectLockLegalHoldStatus::On,
            ObjectLockLegalHoldStatus::Off,
        ] {
            assert_eq!(
                ObjectLockLegalHoldStatus::from_sdk(&status.to_sdk()),
                Some(status)
            );
        }

        // A HEAD response carries the mode as the SDK's other mode enum.
        assert_eq!(
            ObjectLockRetentionMode::from_sdk_head(&aws_sdk_s3::types::ObjectLockMode::Governance),
            Some(ObjectLockRetentionMode::Governance)
        );
        assert_eq!(
            ObjectLockRetentionMode::from_sdk_head(&aws_sdk_s3::types::ObjectLockMode::Compliance),
            Some(ObjectLockRetentionMode::Compliance)
        );

        // Values the SDK does not model arrive as their `Unknown` variant.
        let unknown_mode = aws_sdk_s3::types::ObjectLockRetentionMode::from("future-mode");
        assert_eq!(ObjectLockRetentionMode::from_sdk(&unknown_mode), None);
        let unknown_head_mode = aws_sdk_s3::types::ObjectLockMode::from("future-mode");
        assert_eq!(
            ObjectLockRetentionMode::from_sdk_head(&unknown_head_mode),
            None
        );
        let unknown_status = aws_sdk_s3::types::ObjectLockLegalHoldStatus::from("future-status");
        assert_eq!(ObjectLockLegalHoldStatus::from_sdk(&unknown_status), None);
    }

    #[test]
    fn test_object_lock_enums_text() {
        assert_eq!(ObjectLockRetentionMode::Governance.as_str(), "GOVERNANCE");
        assert_eq!(ObjectLockRetentionMode::Compliance.as_str(), "COMPLIANCE");
        assert_eq!(ObjectLockLegalHoldStatus::On.as_str(), "ON");
        assert_eq!(ObjectLockLegalHoldStatus::Off.as_str(), "OFF");

        let governance = ObjectLockRetentionMode::Governance.effect();
        assert!(governance.contains("BypassGovernanceRetention"));
        assert!(governance.contains("put_object_retention"));
        let compliance = ObjectLockRetentionMode::Compliance.effect();
        assert!(compliance.contains("Nobody can shorten or remove"));

        let on = ObjectLockLegalHoldStatus::On.effect();
        assert!(on.contains("cannot be deleted or overwritten"));
        assert!(on.contains("put_object_legal_hold"));
        let off = ObjectLockLegalHoldStatus::Off.effect();
        assert!(off.contains("can be deleted or overwritten unless"));
        assert!(off.contains("retention") && off.contains("get_object_retention"));
    }

    #[test]
    fn test_default_retention_serialization() {
        let original = default_retention(ObjectLockRetentionMode::Compliance, Some(30), None);
        let json = serde_json::to_string(&original).unwrap();
        assert_eq!(json, r#"{"mode":"COMPLIANCE","days":30}"#);
        let deserialized: DefaultRetention = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, original);

        // Leaving a period out and sending it as null mean the same.
        let years = default_retention(ObjectLockRetentionMode::Governance, None, Some(7));
        assert_eq!(
            serde_json::to_string(&years).unwrap(),
            r#"{"mode":"GOVERNANCE","years":7}"#
        );
        let deserialized: DefaultRetention =
            serde_json::from_str(r#"{"mode":"GOVERNANCE","days":null,"years":7}"#).unwrap();
        assert_eq!(deserialized, years);
    }

    #[test]
    fn test_default_retention_sdk_round_trip() {
        for expected in [
            default_retention(ObjectLockRetentionMode::Governance, Some(1), None),
            default_retention(ObjectLockRetentionMode::Compliance, None, Some(100)),
        ] {
            assert_eq!(
                DefaultRetention::from_sdk(&expected.to_sdk().unwrap()),
                Some(expected)
            );
        }

        // A retention without a mode cannot be represented here.
        let modeless = aws_sdk_s3::types::DefaultRetention::builder()
            .days(10)
            .build();
        assert_eq!(DefaultRetention::from_sdk(&modeless), None);

        // The setter refuses an invalid period instead of building it.
        let error = default_retention(ObjectLockRetentionMode::Governance, None, None)
            .to_sdk()
            .unwrap_err();
        assert!(error.to_string().contains("needs a period"), "{error}");
    }

    #[test]
    fn test_default_retention_from_optional_fields() {
        // A complete set of fields builds the retention; an invalid one is refused.
        let retention = default_retention(ObjectLockRetentionMode::Governance, Some(30), None);
        assert_eq!(
            DefaultRetention::from_optional_fields(
                Some(ObjectLockRetentionMode::Governance),
                Some(30),
                None
            )
            .unwrap(),
            Some(retention)
        );
        let error = DefaultRetention::from_optional_fields(
            Some(ObjectLockRetentionMode::Compliance),
            None,
            None,
        )
        .unwrap_err();
        assert!(error.to_string().contains("needs a period"), "{error}");

        // Nothing given at all asks to remove the default retention.
        assert_eq!(
            DefaultRetention::from_optional_fields(None, None, None).unwrap(),
            None
        );

        // A period without a mode describes no retention.
        for (days, years) in [(Some(30), None), (None, Some(1))] {
            let error = DefaultRetention::from_optional_fields(None, days, years).unwrap_err();
            assert!(error.to_string().contains("needs a mode"), "{error}");
        }
    }

    #[test]
    fn test_validate_default_retention_accepts_valid_periods() {
        for valid in [
            default_retention(ObjectLockRetentionMode::Governance, Some(1), None),
            default_retention(ObjectLockRetentionMode::Compliance, None, Some(1)),
            default_retention(ObjectLockRetentionMode::Compliance, Some(365), None),
        ] {
            assert!(validate_default_retention(&valid).is_ok(), "{valid:?}");
        }
    }

    #[test]
    fn test_validate_default_retention_rejects_periods_shorter_than_one_unit() {
        let error = validate_default_retention(&default_retention(
            ObjectLockRetentionMode::Governance,
            Some(0),
            None,
        ))
        .unwrap_err();
        assert!(
            error.to_string().contains("must be at least 1 day; got 0"),
            "{error}"
        );

        let error = validate_default_retention(&default_retention(
            ObjectLockRetentionMode::Compliance,
            None,
            Some(-5),
        ))
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("must be at least 1 year; got -5"),
            "{error}"
        );
    }

    #[test]
    fn test_validate_default_retention_rejects_no_or_both_periods() {
        let error = validate_default_retention(&default_retention(
            ObjectLockRetentionMode::Governance,
            Some(30),
            Some(1),
        ))
        .unwrap_err();
        assert!(
            error.to_string().contains("days (30) and years (1)"),
            "{error}"
        );

        let error = validate_default_retention(&default_retention(
            ObjectLockRetentionMode::Compliance,
            None,
            None,
        ))
        .unwrap_err();
        assert!(error.to_string().contains("needs a period"), "{error}");
    }

    #[test]
    fn test_default_retention_label() {
        let days = default_retention(ObjectLockRetentionMode::Governance, Some(30), None);
        assert_eq!(days.label(), "GOVERNANCE for 30 day(s)");
        let years = default_retention(ObjectLockRetentionMode::Compliance, None, Some(1));
        assert_eq!(years.label(), "COMPLIANCE for 1 year(s)");
        // An invalid retention still renders its mode.
        let bare = default_retention(ObjectLockRetentionMode::Compliance, None, None);
        assert_eq!(bare.label(), "COMPLIANCE");

        assert_eq!(days.period_label(), "30 day(s)");
        assert_eq!(years.period_label(), "1 year(s)");
        assert_eq!(bare.period_label(), "no period");
    }

    #[test]
    fn test_object_lock_configuration_from_sdk() {
        let with_rule = aws_sdk_s3::types::ObjectLockConfiguration::builder()
            .object_lock_enabled(aws_sdk_s3::types::ObjectLockEnabled::Enabled)
            .rule(
                aws_sdk_s3::types::ObjectLockRule::builder()
                    .default_retention(
                        aws_sdk_s3::types::DefaultRetention::builder()
                            .mode(aws_sdk_s3::types::ObjectLockRetentionMode::Compliance)
                            .years(1)
                            .build(),
                    )
                    .build(),
            )
            .build();
        assert_eq!(
            ObjectLockConfiguration::from_sdk(&with_rule),
            ObjectLockConfiguration {
                object_lock_enabled: true,
                default_retention: Some(default_retention(
                    ObjectLockRetentionMode::Compliance,
                    None,
                    Some(1)
                )),
            }
        );

        // A bucket without a default retention reports a rule-free body.
        let without_rule = aws_sdk_s3::types::ObjectLockConfiguration::builder()
            .object_lock_enabled(aws_sdk_s3::types::ObjectLockEnabled::Enabled)
            .build();
        assert_eq!(
            ObjectLockConfiguration::from_sdk(&without_rule),
            ObjectLockConfiguration {
                object_lock_enabled: true,
                default_retention: None,
            }
        );

        // An enabled flag the SDK does not model does not count as enabled.
        let unknown = aws_sdk_s3::types::ObjectLockConfiguration::builder()
            .object_lock_enabled(aws_sdk_s3::types::ObjectLockEnabled::from("future-state"))
            .build();
        assert!(!ObjectLockConfiguration::from_sdk(&unknown).object_lock_enabled);
    }

    #[test]
    fn test_object_lock_configuration_serialization() {
        let original = ObjectLockConfiguration {
            object_lock_enabled: true,
            default_retention: Some(default_retention(
                ObjectLockRetentionMode::Governance,
                Some(30),
                None,
            )),
        };
        let json = serde_json::to_string(&original).unwrap();
        assert_eq!(
            json,
            r#"{"object_lock_enabled":true,"default_retention":{"mode":"GOVERNANCE","days":30}}"#
        );
        let deserialized: ObjectLockConfiguration = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, original);

        let bare = serde_json::to_string(&ObjectLockConfiguration {
            object_lock_enabled: true,
            default_retention: None,
        })
        .unwrap();
        assert_eq!(bare, r#"{"object_lock_enabled":true}"#);
        let deserialized: ObjectLockConfiguration = serde_json::from_str(&bare).unwrap();
        assert_eq!(deserialized.default_retention, None);
    }

    #[test]
    fn test_object_retention_sdk_round_trip() {
        let now = timestamp("2026-11-26T00:00:00Z");
        for mode in [
            ObjectLockRetentionMode::Governance,
            ObjectLockRetentionMode::Compliance,
        ] {
            let expected = retention(mode, "2030-01-01T00:00:00Z");
            assert_eq!(
                ObjectRetention::from_sdk(&expected.to_sdk(now).unwrap()),
                Some(expected)
            );
        }

        // An empty body is how some S3 implementations report "no retention".
        let empty = aws_sdk_s3::types::ObjectLockRetention::builder().build();
        assert_eq!(ObjectRetention::from_sdk(&empty), None);
        // So is a body carrying only one of the two fields.
        let dateless = aws_sdk_s3::types::ObjectLockRetention::builder()
            .mode(aws_sdk_s3::types::ObjectLockRetentionMode::Compliance)
            .build();
        assert_eq!(ObjectRetention::from_sdk(&dateless), None);
    }

    #[test]
    fn test_object_retention_to_sdk_rejects_unusable_dates() {
        let now = timestamp("2026-11-26T00:00:00Z");

        // S3 only accepts a retain-until date that lies ahead.
        for value in ["2026-11-26T00:00:00Z", "2020-01-01T00:00:00Z"] {
            let error = retention(ObjectLockRetentionMode::Governance, value)
                .to_sdk(now)
                .unwrap_err();
            assert!(
                error.to_string().contains("is not in the future"),
                "{value}: {error}"
            );
        }

        for value in ["", "not-a-date", "2030-01-01", "2030-01-01 00:00:00"] {
            let error = retention(ObjectLockRetentionMode::Compliance, value)
                .to_sdk(now)
                .unwrap_err();
            assert!(
                error.to_string().contains("Invalid retain_until"),
                "{value}: {error}"
            );
        }
    }

    #[test]
    fn test_object_retention_is_active_at_follows_the_retain_until_timestamp() {
        let now = timestamp("2026-11-26T00:00:00Z");
        assert!(
            retention(ObjectLockRetentionMode::Compliance, "2030-01-01T00:00:00Z")
                .is_active_at(now)
        );
        // The retention ends the moment the timestamp is reached.
        assert!(
            !retention(ObjectLockRetentionMode::Compliance, "2026-11-26T00:00:00Z")
                .is_active_at(now)
        );
        assert!(
            !retention(ObjectLockRetentionMode::Compliance, "2020-01-01T00:00:00Z")
                .is_active_at(now)
        );
        // A timestamp that cannot be parsed is not a retention to rely on.
        assert!(!retention(ObjectLockRetentionMode::Compliance, "not-a-date").is_active_at(now));
    }

    #[test]
    fn test_object_retention_label() {
        assert_eq!(
            retention(ObjectLockRetentionMode::Compliance, "2030-01-01T00:00:00Z").label(),
            "COMPLIANCE until 2030-01-01T00:00:00Z"
        );
    }
}

/// The lock reads against an endpoint that answers the way the three
/// backends do, so that the real SDK delivers their error codes.
#[cfg(test)]
mod backend_tests {
    use super::*;
    use crate::s3_client::fake_s3::{Answer, FakeS3, OK, RecordedRequest};
    use crate::s3_client::{ObjectLockLegalHoldStatus, ObjectLockRetentionMode};

    /// AWS S3 words its refusal of a bucket without Object Lock with spaces.
    const AWS_LOCK_DISABLED: &str = "Bucket is missing Object Lock Configuration";
    /// MinIO and RustFS word theirs without.
    const MINIO_LOCK_DISABLED: &str = "Bucket is missing ObjectLockConfiguration";
    /// RustFS's wording for a stored configuration that is not enabled.
    const RUSTFS_LOCK_NOT_ENABLED: &str = "Object Lock is not enabled for this bucket";

    /// The refusal of an object-level lock read in a bucket without Object
    /// Lock.
    fn lock_disabled(message: &str) -> Answer {
        Answer::error(400, "InvalidRequest", message)
    }

    /// What every backend answers for a version that carries no setting.
    fn unset(status: u16) -> Answer {
        Answer::error(
            status,
            "NoSuchObjectLockConfiguration",
            "The specified object does not have a ObjectLock configuration",
        )
    }

    /// An endpoint with one bucket that has Object Lock (`locked`), one that
    /// has it disabled (`plain`), and one that does not exist (`absent`).
    /// For a bucket without Object Lock the backends word their refusal
    /// differently, so the object's name picks the wording.
    fn endpoint() -> FakeS3 {
        FakeS3::start(
            |request| match (request.method.as_str(), request.bucket.as_str()) {
                ("HEAD", "absent") => {
                    Answer::error(404, "NoSuchBucket", "The specified bucket does not exist")
                }
                ("HEAD", _) => OK,
                (_, "absent") | (_, "plain") if request.has("object-lock") => Answer::error(
                    404,
                    "ObjectLockConfigurationNotFoundError",
                    "Object Lock configuration does not exist for this bucket",
                ),
                (_, "locked") if request.has("object-lock") => Answer::ok(
                    "<ObjectLockConfiguration><ObjectLockEnabled>Enabled</ObjectLockEnabled>\
                     </ObjectLockConfiguration>",
                ),
                _ if request.has("retention") => retention_answer(request),
                _ if request.has("legal-hold") => legal_hold_answer(request),
                _ => Answer::error(501, "NotImplemented", "A request without a script"),
            },
        )
    }

    fn retention_answer(request: &RecordedRequest) -> Answer {
        match (request.bucket.as_str(), request.key.as_str()) {
            ("locked", "retained") => Answer::ok(
                "<Retention><Mode>COMPLIANCE</Mode>\
                 <RetainUntilDate>2030-01-01T00:00:00.000Z</RetainUntilDate></Retention>",
            ),
            // Older RustFS answered an unretained version with an empty
            // element instead of an error.
            ("locked", "legacy-empty") => Answer::ok("<Retention></Retention>"),
            ("locked", "bare") => unset(404),
            ("plain", "aws") => lock_disabled(AWS_LOCK_DISABLED),
            ("plain", "minio") => lock_disabled(MINIO_LOCK_DISABLED),
            ("plain", "rustfs") => lock_disabled(RUSTFS_LOCK_NOT_ENABLED),
            // A refusal that has nothing to do with Object Lock.
            ("locked", "other-refusal") => {
                lock_disabled("The retain-until date must be in the future")
            }
            ("locked", "missing") => {
                Answer::error(404, "NoSuchKey", "The specified key does not exist.")
            }
            ("locked", "broken") => Answer::error(
                500,
                "InternalError",
                "We encountered an internal error. Please try again.",
            ),
            // MinIO refuses the read of a missing bucket with the same
            // answer as for a bucket without Object Lock, before it ever
            // looks the object up.
            ("absent", "any") => lock_disabled(MINIO_LOCK_DISABLED),
            _ => Answer::error(501, "NotImplemented", "A request without a script"),
        }
    }

    fn legal_hold_answer(request: &RecordedRequest) -> Answer {
        match (request.bucket.as_str(), request.key.as_str()) {
            ("locked", "held") => Answer::ok("<LegalHold><Status>ON</Status></LegalHold>"),
            ("locked", "released") => Answer::ok("<LegalHold><Status>OFF</Status></LegalHold>"),
            // MinIO answers this code with a 400 rather than a 404.
            ("locked", "bare") => unset(400),
            ("plain", "aws") => lock_disabled(AWS_LOCK_DISABLED),
            ("plain", "minio") => lock_disabled(MINIO_LOCK_DISABLED),
            ("plain", "rustfs") => lock_disabled(RUSTFS_LOCK_NOT_ENABLED),
            ("locked", "missing") => {
                Answer::error(404, "NoSuchKey", "The specified key does not exist.")
            }
            ("absent", "any") => lock_disabled(MINIO_LOCK_DISABLED),
            _ => Answer::error(501, "NotImplemented", "A request without a script"),
        }
    }

    /// The error and everything behind it, the way a tool response shows it.
    fn shown(error: anyhow::Error) -> String {
        format!("{error:#}")
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_retention_read_reports_what_each_backend_answers() {
        let endpoint = endpoint();
        let client = S3Client::for_endpoint(endpoint.endpoint());

        let retained = client
            .get_object_retention("locked", "retained", Some("v1"))
            .await
            .unwrap();
        assert_eq!(
            retained
                .retention
                .map(|retention| (retention.mode, retention.retain_until)),
            Some((
                ObjectLockRetentionMode::Compliance,
                "2030-01-01T00:00:00Z".to_string()
            ))
        );
        assert_eq!(retained.unset_reason, None);
        // The version travels as a query parameter.
        assert_eq!(endpoint.requests()[0].query("versionId"), Some("v1"));

        // A version that carries no retention period, by error or by an
        // empty answer.
        for key in ["bare", "legacy-empty"] {
            let read = client
                .get_object_retention("locked", key, None)
                .await
                .unwrap();
            assert_eq!(
                (read.retention, read.unset_reason),
                (None, Some(UnsetLockReason::NotSet)),
                "{key}"
            );
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_retention_read_reports_a_bucket_without_object_lock_for_every_wording() {
        let endpoint = endpoint();
        let client = S3Client::for_endpoint(endpoint.endpoint());

        // The three wordings of the refusal, whatever their status.
        for key in ["aws", "minio", "rustfs"] {
            let read = client
                .get_object_retention("plain", key, None)
                .await
                .unwrap();
            assert_eq!(
                (read.retention, read.unset_reason),
                (None, Some(UnsetLockReason::BucketNotEnabled)),
                "{key}"
            );
        }

        // Each of those reads confirmed the bucket by reading its Object
        // Lock configuration, which the "not found" answer and the HEAD
        // that follows it turned into "disabled".
        let requests = endpoint.requests();
        assert_eq!(requests.len(), 9);
        for read in requests.chunks(3) {
            let shape: Vec<(&str, &str)> = read
                .iter()
                .map(|request| (request.method.as_str(), request.bucket.as_str()))
                .collect();
            assert_eq!(
                shape,
                [("GET", "plain"), ("GET", "plain"), ("HEAD", "plain")]
            );
            assert!(read[0].has("retention"));
            assert!(read[1].has("object-lock"));
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_retention_read_keeps_other_failures() {
        let endpoint = endpoint();
        let client = S3Client::for_endpoint(endpoint.endpoint());

        // A refusal from a bucket that has Object Lock means something else.
        let error = shown(
            client
                .get_object_retention("locked", "other-refusal", None)
                .await
                .unwrap_err(),
        );
        assert!(
            error.starts_with("Failed to get the retention of s3://locked/other-refusal"),
            "{error}"
        );
        assert!(error.contains("InvalidRequest"), "{error}");
        assert!(error.contains("must be in the future"), "{error}");

        // A missing object and a server failure are never "no retention".
        let error = shown(
            client
                .get_object_retention("locked", "missing", None)
                .await
                .unwrap_err(),
        );
        assert!(error.contains("NoSuchKey"), "{error}");
        assert!(
            client
                .get_object_retention("locked", "broken", None)
                .await
                .is_err()
        );
        // Only the refusal made the client look at the bucket, and the
        // bucket's Object Lock configuration then confirmed that the
        // refusal meant something else.
        let requests = endpoint.requests();
        let shape: Vec<(&str, &str, bool)> = requests
            .iter()
            .map(|request| {
                (
                    request.method.as_str(),
                    request.key.as_str(),
                    request.has("object-lock"),
                )
            })
            .collect();
        assert_eq!(
            shape,
            [
                ("GET", "other-refusal", false),
                ("GET", "", true),
                ("GET", "missing", false),
                ("GET", "broken", false),
            ]
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_retention_read_of_a_missing_bucket_stays_an_error() {
        let endpoint = endpoint();
        let client = S3Client::for_endpoint(endpoint.endpoint());

        // MinIO's refusal for a missing bucket is the same InvalidRequest as
        // for a bucket without Object Lock. The bucket cannot be confirmed
        // either way, so the refusal stays an error that also tells why.
        let error = shown(
            client
                .get_object_retention("absent", "any", None)
                .await
                .unwrap_err(),
        );
        assert!(
            error.starts_with("Failed to get the retention of s3://absent/any"),
            "{error}"
        );
        assert!(error.contains("InvalidRequest"), "{error}");
        assert!(
            error.contains("the Object Lock configuration of bucket 'absent' could not be read"),
            "{error}"
        );
        assert!(
            error.contains("the bucket does not exist or is not accessible"),
            "{error}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_legal_hold_read_reports_what_each_backend_answers() {
        let endpoint = endpoint();
        let client = S3Client::for_endpoint(endpoint.endpoint());

        let held = client
            .get_object_legal_hold("locked", "held", None)
            .await
            .unwrap();
        assert_eq!(held.status, Some(ObjectLockLegalHoldStatus::On));
        assert_eq!(held.unset_reason, None);

        // An explicit OFF is a status, not the absence of one.
        let released = client
            .get_object_legal_hold("locked", "released", None)
            .await
            .unwrap();
        assert_eq!(released.status, Some(ObjectLockLegalHoldStatus::Off));
        assert_eq!(released.unset_reason, None);

        // A version that never had a hold.
        let bare = client
            .get_object_legal_hold("locked", "bare", None)
            .await
            .unwrap();
        assert_eq!(
            (bare.status, bare.unset_reason),
            (None, Some(UnsetLockReason::NotSet))
        );

        // A bucket without Object Lock, in every wording.
        for key in ["aws", "minio", "rustfs"] {
            let read = client
                .get_object_legal_hold("plain", key, None)
                .await
                .unwrap();
            assert_eq!(
                (read.status, read.unset_reason),
                (None, Some(UnsetLockReason::BucketNotEnabled)),
                "{key}"
            );
        }

        // A missing object stays an error.
        assert!(
            client
                .get_object_legal_hold("locked", "missing", None)
                .await
                .is_err()
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_writes_send_what_the_backends_read() {
        let endpoint = FakeS3::start(|_| OK);
        let client = S3Client::for_endpoint(endpoint.endpoint());

        client
            .put_object_retention(
                "locked",
                "doc",
                Some("v1"),
                &ObjectRetention {
                    mode: ObjectLockRetentionMode::Governance,
                    retain_until: "2030-01-01T00:00:00Z".to_string(),
                },
                true,
            )
            .await
            .unwrap();
        client
            .put_object_legal_hold("locked", "doc", None, ObjectLockLegalHoldStatus::On)
            .await
            .unwrap();

        let requests = endpoint.requests();
        assert_eq!(requests.len(), 2);

        // The retention, its version, and the bypass all reach the wire.
        let retention = &requests[0];
        assert_eq!(
            (
                retention.method.as_str(),
                retention.bucket.as_str(),
                retention.key.as_str()
            ),
            ("PUT", "locked", "doc")
        );
        assert!(retention.has("retention"));
        assert_eq!(retention.query("versionId"), Some("v1"));
        assert_eq!(
            retention.header("x-amz-bypass-governance-retention"),
            Some("true")
        );
        let body = retention.body_text();
        assert!(body.contains("<Mode>GOVERNANCE</Mode>"), "{body}");
        assert!(
            body.contains("<RetainUntilDate>2030-01-01T00:00:00Z</RetainUntilDate>"),
            "{body}"
        );

        let hold = &requests[1];
        assert_eq!((hold.method.as_str(), hold.key.as_str()), ("PUT", "doc"));
        assert!(hold.has("legal-hold"));
        assert!(hold.body_text().contains("<Status>ON</Status>"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_bucket_lock_configuration_read_distinguishes_disabled_from_missing() {
        let endpoint = endpoint();
        let client = S3Client::for_endpoint(endpoint.endpoint());

        let locked = client
            .get_bucket_lock_configuration("locked")
            .await
            .unwrap();
        assert!(locked.object_lock_enabled());
        assert_eq!(endpoint.requests().len(), 1);

        // ObjectLockConfigurationNotFoundError is what AWS S3, MinIO, and
        // RustFS answer for a bucket without Object Lock.
        let plain = client.get_bucket_lock_configuration("plain").await.unwrap();
        assert_eq!(plain.configuration, None);
        assert!(!plain.object_lock_enabled());
        // The answer was confirmed by a HEAD of the bucket.
        let requests = endpoint.requests();
        let shape: Vec<(&str, &str)> = requests[1..]
            .iter()
            .map(|request| (request.method.as_str(), request.bucket.as_str()))
            .collect();
        assert_eq!(shape, [("GET", "plain"), ("HEAD", "plain")]);

        // The same answer for a bucket that does not exist stays an error.
        let error = shown(
            client
                .get_bucket_lock_configuration("absent")
                .await
                .unwrap_err(),
        );
        assert!(
            error.contains("the bucket does not exist or is not accessible"),
            "{error}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_bucket_lock_configuration_read_keeps_other_failures() {
        let endpoint = FakeS3::start(|request| {
            if request.method == "HEAD" {
                return OK;
            }
            match request.bucket.as_str() {
                "object-level-code" => Answer::error(
                    404,
                    "NoSuchObjectLockConfiguration",
                    "The specified object does not have a ObjectLock configuration",
                ),
                "denied" => Answer::error(403, "AccessDenied", "Access Denied"),
                _ => Answer::error(500, "InternalError", "We encountered an internal error."),
            }
        });
        let client = S3Client::for_endpoint(endpoint.endpoint());

        // An implementation answering the object-level code for the bucket
        // does not turn "not enabled" into a failure.
        let config = client
            .get_bucket_lock_configuration("object-level-code")
            .await
            .unwrap();
        assert_eq!(config.configuration, None);

        // A denial and a server failure are reported as such, without a
        // probe that could not tell more.
        let error = shown(
            client
                .get_bucket_lock_configuration("denied")
                .await
                .unwrap_err(),
        );
        assert!(error.contains("AccessDenied"), "{error}");
        assert!(
            client
                .get_bucket_lock_configuration("broken")
                .await
                .is_err()
        );
        let requests = endpoint.requests();
        let shape: Vec<(&str, &str)> = requests
            .iter()
            .map(|request| (request.method.as_str(), request.bucket.as_str()))
            .collect();
        assert_eq!(
            shape,
            [
                ("GET", "object-level-code"),
                ("HEAD", "object-level-code"),
                ("GET", "denied"),
                ("GET", "broken"),
            ]
        );
    }
}

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
use aws_sdk_s3::primitives::{DateTime, DateTimeFormat};
use serde::{Deserialize, Serialize};

/// Error code S3 answers with when an object version has no retention period
/// or legal hold to report.
const UNSET_OBJECT_LOCK_ERROR_CODE: &str = "NoSuchObjectLockConfiguration";

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

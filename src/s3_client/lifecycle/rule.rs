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

use anyhow::{Context, Result};
use aws_sdk_s3::primitives::{DateTime, DateTimeFormat};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Whether a lifecycle rule is applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum LifecycleRuleStatus {
    #[serde(rename = "enabled")]
    Enabled,
    #[serde(rename = "disabled")]
    Disabled,
}

impl LifecycleRuleStatus {
    fn to_sdk(self) -> aws_sdk_s3::types::ExpirationStatus {
        match self {
            Self::Enabled => aws_sdk_s3::types::ExpirationStatus::Enabled,
            Self::Disabled => aws_sdk_s3::types::ExpirationStatus::Disabled,
        }
    }

    /// Display name for summaries.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Enabled => "enabled",
            Self::Disabled => "disabled",
        }
    }
}

/// When the current version of an object stops being kept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LifecycleExpiration {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Expire the object this many days after creation (at least 1)")]
    pub days: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Expire the object on this ISO 8601 date, e.g. \"2026-12-31T00:00:00Z\"; use either days or date"
    )]
    pub date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Remove leftover delete markers once every older version has expired (versioned buckets only); cannot be combined with days or date"
    )]
    pub expired_object_delete_marker: Option<bool>,
}

/// When the current version of an object moves to another storage class.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LifecycleTransition {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Move the object this many days after creation (0 or more)")]
    pub days: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Move the object on this ISO 8601 date; use either days or date")]
    pub date: Option<String>,
    #[schemars(
        description = "Storage class to move to, e.g. GLACIER, DEEP_ARCHIVE, or a class the storage backend offers"
    )]
    pub storage_class: String,
}

/// When a noncurrent (older) version of an object moves to another storage class.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NoncurrentVersionTransition {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Apply only once this many newer versions exist (at least 1)")]
    pub newer_noncurrent_versions: Option<i32>,
    #[schemars(
        description = "Move the version this many days after it became noncurrent (at least 1)"
    )]
    pub days: i32,
    #[schemars(description = "Storage class to move to")]
    pub storage_class: String,
}

/// When a noncurrent (older) version of an object stops being kept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NoncurrentVersionExpiration {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Keep at least this many noncurrent versions (at least 1)")]
    pub newer_noncurrent_versions: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Expire the version this many days after it became noncurrent (at least 1)"
    )]
    pub days: Option<i32>,
}

/// One lifecycle rule: which objects it applies to and what happens to them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LifecycleRule {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Optional unique identifier of the rule (max 255 characters)")]
    pub id: Option<String>,
    #[schemars(description = "Whether the rule is applied: enabled or disabled")]
    pub status: LifecycleRuleStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Optional key prefix the rule applies to, e.g. \"logs/\"; omit it to match every object in the bucket"
    )]
    pub prefix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Optional tags; the object must carry all of them for the rule to apply"
    )]
    pub tags: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Optional size filter: only objects larger than this many bytes")]
    pub object_size_greater_than: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Optional size filter: only objects smaller than this many bytes")]
    pub object_size_less_than: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Expire the current version of matching objects")]
    pub expiration: Option<LifecycleExpiration>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(
        description = "Move the current version of matching objects to other storage classes"
    )]
    pub transitions: Vec<LifecycleTransition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(
        description = "Move noncurrent versions of matching objects to other storage classes"
    )]
    pub noncurrent_version_transitions: Vec<NoncurrentVersionTransition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Expire noncurrent versions of matching objects")]
    pub noncurrent_version_expiration: Option<NoncurrentVersionExpiration>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Discard multipart uploads that were started but never completed this many days after they began (at least 1)"
    )]
    pub abort_incomplete_multipart_upload_after_days: Option<i32>,
}

/// Format an SDK timestamp the way the rest of the crate does.
fn format_timestamp(date: &DateTime) -> String {
    date.fmt(DateTimeFormat::DateTime).unwrap()
}

/// An ISO 8601 date from a request, as the SDK type.
fn parse_date(date: &str, what: &str) -> Result<DateTime> {
    DateTime::from_str(date, DateTimeFormat::DateTime)
        .with_context(|| format!("Failed to parse the {what} '{date}'"))
}

/// A prefix that selects something, ignoring S3's empty "no prefix" value.
fn selective_prefix(prefix: Option<&str>) -> Option<String> {
    prefix
        .filter(|prefix| !prefix.is_empty())
        .map(str::to_string)
}

impl LifecycleExpiration {
    fn to_sdk(&self) -> Result<aws_sdk_s3::types::LifecycleExpiration> {
        let mut builder = aws_sdk_s3::types::LifecycleExpiration::builder()
            .set_days(self.days)
            .set_expired_object_delete_marker(self.expired_object_delete_marker);
        if let Some(date) = &self.date {
            builder = builder.set_date(Some(parse_date(date, "expiration date")?));
        }
        Ok(builder.build())
    }

    fn from_sdk(expiration: &aws_sdk_s3::types::LifecycleExpiration) -> Self {
        Self {
            days: expiration.days(),
            date: expiration.date().map(format_timestamp),
            expired_object_delete_marker: expiration.expired_object_delete_marker(),
        }
    }
}

impl LifecycleTransition {
    fn to_sdk(&self) -> Result<aws_sdk_s3::types::Transition> {
        let mut builder = aws_sdk_s3::types::Transition::builder()
            .set_days(self.days)
            .storage_class(self.storage_class.as_str().into());
        if let Some(date) = &self.date {
            builder = builder.set_date(Some(parse_date(date, "transition date")?));
        }
        Ok(builder.build())
    }

    fn from_sdk(transition: &aws_sdk_s3::types::Transition) -> Self {
        Self {
            days: transition.days(),
            date: transition.date().map(format_timestamp),
            storage_class: transition
                .storage_class()
                .map(|class| class.as_str().to_string())
                .unwrap_or_default(),
        }
    }
}

impl NoncurrentVersionTransition {
    fn to_sdk(&self) -> aws_sdk_s3::types::NoncurrentVersionTransition {
        aws_sdk_s3::types::NoncurrentVersionTransition::builder()
            .set_noncurrent_days(Some(self.days))
            .set_newer_noncurrent_versions(self.newer_noncurrent_versions)
            .storage_class(self.storage_class.as_str().into())
            .build()
    }

    fn from_sdk(transition: &aws_sdk_s3::types::NoncurrentVersionTransition) -> Self {
        Self {
            newer_noncurrent_versions: transition.newer_noncurrent_versions(),
            // S3 reports the days of every valid transition; a missing value
            // (0) fails validation if the rule is submitted again.
            days: transition.noncurrent_days().unwrap_or_default(),
            storage_class: transition
                .storage_class()
                .map(|class| class.as_str().to_string())
                .unwrap_or_default(),
        }
    }
}

impl NoncurrentVersionExpiration {
    fn to_sdk(&self) -> aws_sdk_s3::types::NoncurrentVersionExpiration {
        aws_sdk_s3::types::NoncurrentVersionExpiration::builder()
            .set_noncurrent_days(self.days)
            .set_newer_noncurrent_versions(self.newer_noncurrent_versions)
            .build()
    }

    fn from_sdk(expiration: &aws_sdk_s3::types::NoncurrentVersionExpiration) -> Self {
        Self {
            newer_noncurrent_versions: expiration.newer_noncurrent_versions(),
            days: expiration.noncurrent_days(),
        }
    }
}

impl LifecycleRule {
    /// Build the SDK rule. Call `validate_lifecycle_rules` first: the
    /// conversion assumes a rule that passed validation.
    pub(super) fn to_sdk(&self) -> Result<aws_sdk_s3::types::LifecycleRule> {
        let mut builder = aws_sdk_s3::types::LifecycleRule::builder()
            .status(self.status.to_sdk())
            .set_filter(self.filter_to_sdk())
            .set_id(self.id.clone());

        if let Some(expiration) = &self.expiration {
            builder = builder.expiration(expiration.to_sdk()?);
        }

        builder = builder.set_transitions(Some(
            self.transitions
                .iter()
                .map(LifecycleTransition::to_sdk)
                .collect::<Result<Vec<_>>>()?,
        ));

        builder = builder.set_noncurrent_version_transitions(Some(
            self.noncurrent_version_transitions
                .iter()
                .map(NoncurrentVersionTransition::to_sdk)
                .collect(),
        ));

        builder = builder.set_noncurrent_version_expiration(
            self.noncurrent_version_expiration
                .as_ref()
                .map(NoncurrentVersionExpiration::to_sdk),
        );

        builder = builder.set_abort_incomplete_multipart_upload(
            self.abort_incomplete_multipart_upload_after_days
                .map(|days| {
                    aws_sdk_s3::types::AbortIncompleteMultipartUpload::builder()
                        .days_after_initiation(days)
                        .build()
                }),
        );

        builder
            .build()
            .context("Failed to build the lifecycle rule")
    }

    /// Convert a rule as S3 returned it.
    pub(super) fn from_sdk(rule: &aws_sdk_s3::types::LifecycleRule) -> Self {
        let (prefix, tags, object_size_greater_than, object_size_less_than) = match rule.filter() {
            Some(filter) => match filter.and() {
                Some(and) => (
                    selective_prefix(and.prefix()),
                    tags_from_sdk(and.tags()),
                    and.object_size_greater_than(),
                    and.object_size_less_than(),
                ),
                None => (
                    selective_prefix(filter.prefix()),
                    filter.tag().map(|tag| {
                        BTreeMap::from([(tag.key().to_string(), tag.value().to_string())])
                    }),
                    filter.object_size_greater_than(),
                    filter.object_size_less_than(),
                ),
            },
            // Older configurations put the prefix next to the rule instead of
            // in a filter.
            None => (legacy_prefix(rule), None, None, None),
        };

        Self {
            id: rule.id().filter(|id| !id.is_empty()).map(str::to_string),
            status: match rule.status() {
                aws_sdk_s3::types::ExpirationStatus::Enabled => LifecycleRuleStatus::Enabled,
                aws_sdk_s3::types::ExpirationStatus::Disabled => LifecycleRuleStatus::Disabled,
                // A status this crate does not model is reported as the safe
                // choice: not applied.
                _ => LifecycleRuleStatus::Disabled,
            },
            prefix,
            tags,
            object_size_greater_than,
            object_size_less_than,
            expiration: rule.expiration().map(LifecycleExpiration::from_sdk),
            transitions: rule
                .transitions()
                .iter()
                .map(LifecycleTransition::from_sdk)
                .collect(),
            noncurrent_version_transitions: rule
                .noncurrent_version_transitions()
                .iter()
                .map(NoncurrentVersionTransition::from_sdk)
                .collect(),
            noncurrent_version_expiration: rule
                .noncurrent_version_expiration()
                .map(NoncurrentVersionExpiration::from_sdk),
            abort_incomplete_multipart_upload_after_days: rule
                .abort_incomplete_multipart_upload()
                .and_then(|abort| abort.days_after_initiation()),
        }
    }

    /// The rule's filter: S3 accepts a single predicate directly and requires
    /// the `And` operator once a rule selects on more than one of them. Every
    /// tag and every size bound is a predicate of its own.
    fn filter_to_sdk(&self) -> Option<aws_sdk_s3::types::LifecycleRuleFilter> {
        let prefix = selective_prefix(self.prefix.as_deref());
        let mut predicates = 0;
        if prefix.is_some() {
            predicates += 1;
        }
        if let Some(tags) = &self.tags {
            // Each tag is its own predicate: a rule with several tags must
            // go through `And`, or S3 would apply it to objects carrying
            // just the one tag that a plain Tag filter can name.
            predicates += tags.len();
        }
        if self.object_size_greater_than.is_some() {
            predicates += 1;
        }
        if self.object_size_less_than.is_some() {
            predicates += 1;
        }

        let mut builder = aws_sdk_s3::types::LifecycleRuleFilter::builder();
        if predicates > 1 {
            builder = builder.and(
                aws_sdk_s3::types::LifecycleRuleAndOperator::builder()
                    .set_prefix(prefix)
                    .set_tags(self.tags.as_ref().map(tags_to_sdk))
                    .set_object_size_greater_than(self.object_size_greater_than)
                    .set_object_size_less_than(self.object_size_less_than)
                    .build(),
            );
        } else if let Some(prefix) = prefix {
            builder = builder.prefix(prefix);
        } else if let Some(tags) = &self.tags {
            // Exactly one tag; a plain Tag filter names it directly.
            builder = builder.tag(tags_to_sdk(tags).into_iter().next()?);
        } else if let Some(minimum) = self.object_size_greater_than {
            builder = builder.object_size_greater_than(minimum);
        } else if let Some(maximum) = self.object_size_less_than {
            builder = builder.object_size_less_than(maximum);
        } else {
            // A whole-bucket rule still needs an explicit filter: AWS
            // rejects a rule without one unless it carries the deprecated
            // rule-level Prefix, so select the whole bucket with an empty
            // prefix.
            builder = builder.prefix(String::new());
        }

        Some(builder.build())
    }
}

/// Read the deprecated `Prefix` element next to a rule. S3 still returns
/// it for configurations that were written before filters existed.
#[allow(deprecated)]
fn legacy_prefix(rule: &aws_sdk_s3::types::LifecycleRule) -> Option<String> {
    selective_prefix(rule.prefix())
}

/// Tags as a sorted map, or `None` when there are none.
fn tags_from_sdk(tags: &[aws_sdk_s3::types::Tag]) -> Option<BTreeMap<String, String>> {
    if tags.is_empty() {
        return None;
    }
    Some(
        tags.iter()
            .map(|tag| (tag.key().to_string(), tag.value().to_string()))
            .collect(),
    )
}

/// Tags as the SDK list.
fn tags_to_sdk(tags: &BTreeMap<String, String>) -> Vec<aws_sdk_s3::types::Tag> {
    tags.iter()
        .map(|(key, value)| {
            aws_sdk_s3::types::Tag::builder()
                .key(key)
                .value(value)
                .build()
                .context("Failed to build a lifecycle tag filter")
        })
        .collect::<Result<Vec<_>>>()
        .expect("a tag built from a key and a value is always valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule_with_everything() -> LifecycleRule {
        LifecycleRule {
            id: Some("cleanup".to_string()),
            status: LifecycleRuleStatus::Enabled,
            prefix: Some("logs/".to_string()),
            tags: Some(BTreeMap::from([(
                "team".to_string(),
                "storage".to_string(),
            )])),
            object_size_greater_than: Some(1024),
            object_size_less_than: None,
            expiration: Some(LifecycleExpiration {
                days: Some(30),
                date: None,
                expired_object_delete_marker: None,
            }),
            transitions: vec![
                LifecycleTransition {
                    days: Some(7),
                    date: None,
                    storage_class: "GLACIER".to_string(),
                },
                LifecycleTransition {
                    days: None,
                    date: Some("2026-12-31T00:00:00Z".to_string()),
                    storage_class: "DEEP_ARCHIVE".to_string(),
                },
            ],
            noncurrent_version_transitions: vec![NoncurrentVersionTransition {
                newer_noncurrent_versions: Some(3),
                days: 7,
                storage_class: "GLACIER".to_string(),
            }],
            noncurrent_version_expiration: Some(NoncurrentVersionExpiration {
                newer_noncurrent_versions: Some(3),
                days: Some(30),
            }),
            abort_incomplete_multipart_upload_after_days: Some(7),
        }
    }

    #[test]
    fn test_rule_round_trips_through_sdk_types() {
        let rule = rule_with_everything();

        let sdk = rule.to_sdk().unwrap();
        // Prefix, tags, and a size bound share one `And` filter.
        let and = sdk.filter().unwrap().and().unwrap();
        assert_eq!(and.prefix(), Some("logs/"));
        assert_eq!(and.tags().len(), 1);
        assert_eq!(and.object_size_greater_than(), Some(1024));

        assert_eq!(LifecycleRule::from_sdk(&sdk), rule);
    }

    #[test]
    fn test_single_dimension_filters_round_trip() {
        let base = LifecycleRule {
            id: None,
            status: LifecycleRuleStatus::Enabled,
            prefix: None,
            tags: None,
            object_size_greater_than: None,
            object_size_less_than: None,
            expiration: Some(LifecycleExpiration {
                days: Some(1),
                date: None,
                expired_object_delete_marker: None,
            }),
            transitions: Vec::new(),
            noncurrent_version_transitions: Vec::new(),
            noncurrent_version_expiration: None,
            abort_incomplete_multipart_upload_after_days: None,
        };

        // A whole-bucket rule carries an explicit empty-prefix filter: AWS
        // rejects a rule without a filter unless it has the deprecated
        // rule-level Prefix, and the empty prefix selects every object.
        let sdk = base.to_sdk().unwrap();
        assert_eq!(sdk.filter().unwrap().prefix(), Some(""));
        assert!(sdk.filter().unwrap().and().is_none());
        assert_eq!(LifecycleRule::from_sdk(&sdk), base);

        // A prefix alone is sent as a plain prefix filter.
        let prefixed = LifecycleRule {
            prefix: Some("logs/".to_string()),
            ..base.clone()
        };
        let sdk = prefixed.to_sdk().unwrap();
        assert_eq!(sdk.filter().unwrap().prefix(), Some("logs/"));
        assert!(sdk.filter().unwrap().and().is_none());
        assert_eq!(LifecycleRule::from_sdk(&sdk), prefixed);

        // A lone tag is sent as a plain tag filter.
        let tagged = LifecycleRule {
            tags: Some(BTreeMap::from([("env".to_string(), "prod".to_string())])),
            ..base.clone()
        };
        let sdk = tagged.to_sdk().unwrap();
        assert_eq!(sdk.filter().unwrap().tag().unwrap().key(), "env");
        assert!(sdk.filter().unwrap().and().is_none());
        assert_eq!(LifecycleRule::from_sdk(&sdk), tagged);

        // A single size bound is sent directly, too: the `And` operator
        // needs at least two predicates, and wrapping one bound in it makes
        // S3 reject the rule.
        let large = LifecycleRule {
            object_size_greater_than: Some(1024),
            ..base.clone()
        };
        let sdk = large.to_sdk().unwrap();
        let filter = sdk.filter().unwrap();
        assert_eq!(filter.object_size_greater_than(), Some(1024));
        assert!(filter.and().is_none());
        assert_eq!(LifecycleRule::from_sdk(&sdk), large);

        let small = LifecycleRule {
            object_size_less_than: Some(1024),
            ..base.clone()
        };
        let sdk = small.to_sdk().unwrap();
        let filter = sdk.filter().unwrap();
        assert_eq!(filter.object_size_less_than(), Some(1024));
        assert!(filter.and().is_none());
        assert_eq!(LifecycleRule::from_sdk(&sdk), small);

        // An empty prefix is the same as no prefix.
        let unprefixed = LifecycleRule {
            prefix: Some(String::new()),
            ..base.clone()
        };
        let sdk = unprefixed.to_sdk().unwrap();
        assert_eq!(sdk.filter().unwrap().prefix(), Some(""));
        assert_eq!(LifecycleRule::from_sdk(&sdk).prefix, None);
    }

    #[test]
    fn test_tags_only_rule_with_several_tags_requires_and_and_keeps_all_of_them() {
        // A tags-only rule with several tags: each tag is its own predicate,
        // so the rule goes through `And` and keeps every tag. Reducing it to
        // the first tag would expire objects outside the requested scope.
        let multi_tagged = LifecycleRule {
            id: None,
            status: LifecycleRuleStatus::Enabled,
            prefix: None,
            tags: Some(BTreeMap::from([
                ("team".to_string(), "storage".to_string()),
                ("env".to_string(), "prod".to_string()),
                ("tier".to_string(), "cold".to_string()),
            ])),
            object_size_greater_than: None,
            object_size_less_than: None,
            expiration: Some(LifecycleExpiration {
                days: Some(30),
                date: None,
                expired_object_delete_marker: None,
            }),
            transitions: Vec::new(),
            noncurrent_version_transitions: Vec::new(),
            noncurrent_version_expiration: None,
            abort_incomplete_multipart_upload_after_days: None,
        };

        let sdk = multi_tagged.to_sdk().unwrap();
        let and = sdk.filter().unwrap().and().unwrap();
        assert!(and.prefix().is_none());
        assert_eq!(and.tags().len(), 3);
        let keys: Vec<&str> = and.tags().iter().map(|tag| tag.key()).collect();
        assert_eq!(keys, ["env", "team", "tier"]);
        assert!(and.object_size_greater_than().is_none());
        assert!(and.object_size_less_than().is_none());
        assert_eq!(LifecycleRule::from_sdk(&sdk), multi_tagged);
    }

    #[test]
    fn test_combined_filters_use_the_and_operator() {
        let sized = LifecycleRule {
            id: None,
            status: LifecycleRuleStatus::Disabled,
            prefix: None,
            tags: None,
            object_size_greater_than: Some(0),
            object_size_less_than: Some(1024),
            expiration: None,
            transitions: Vec::new(),
            noncurrent_version_transitions: Vec::new(),
            noncurrent_version_expiration: None,
            abort_incomplete_multipart_upload_after_days: Some(7),
        };
        let sdk = sized.to_sdk().unwrap();
        let and = sdk.filter().unwrap().and().unwrap();
        assert_eq!(and.object_size_greater_than(), Some(0));
        assert_eq!(and.object_size_less_than(), Some(1024));
        assert_eq!(LifecycleRule::from_sdk(&sdk), sized);

        // Several tags on top of size bounds are more predicates still, so
        // they share the same `And` operator.
        let multi_tagged = LifecycleRule {
            tags: Some(BTreeMap::from([
                ("a".to_string(), "1".to_string()),
                ("b".to_string(), "2".to_string()),
            ])),
            ..sized.clone()
        };
        let sdk = multi_tagged.to_sdk().unwrap();
        let and = sdk.filter().unwrap().and().unwrap();
        assert_eq!(and.tags().len(), 2);
        assert!(and.object_size_greater_than().is_some());
        assert_eq!(LifecycleRule::from_sdk(&sdk), multi_tagged);
    }

    #[test]
    #[allow(deprecated)]
    fn test_legacy_prefix_next_to_the_rule_is_read() {
        let sdk = aws_sdk_s3::types::LifecycleRule::builder()
            .status(aws_sdk_s3::types::ExpirationStatus::Enabled)
            .prefix("old/")
            .expiration(
                aws_sdk_s3::types::LifecycleExpiration::builder()
                    .days(1)
                    .build(),
            )
            .build()
            .unwrap();

        assert_eq!(
            LifecycleRule::from_sdk(&sdk).prefix.as_deref(),
            Some("old/")
        );
    }

    #[test]
    fn test_rule_json_round_trip() {
        let json = r#"{
            "id": "expire-logs",
            "status": "enabled",
            "prefix": "logs/",
            "expiration": {"days": 30},
            "transitions": [{"days": 7, "storage_class": "GLACIER"}],
            "noncurrent_version_transitions": [{"days": 7, "storage_class": "GLACIER"}],
            "noncurrent_version_expiration": {"newer_noncurrent_versions": 3},
            "abort_incomplete_multipart_upload_after_days": 7
        }"#;

        let rule: LifecycleRule = serde_json::from_str(json).unwrap();
        assert_eq!(rule.id.as_deref(), Some("expire-logs"));
        assert_eq!(rule.status, LifecycleRuleStatus::Enabled);
        assert_eq!(rule.transitions.len(), 1);
        assert!(rule.tags.is_none());
        assert!(rule.object_size_greater_than.is_none());

        // Unset fields are left out when serialized, so the output of a get
        // can be fed back into a put unchanged.
        let serialized = serde_json::to_string(&rule).unwrap();
        assert!(!serialized.contains("null"));
        assert!(!serialized.contains("object_size"));
        let reparsed: LifecycleRule = serde_json::from_str(&serialized).unwrap();
        assert_eq!(reparsed, rule);
    }

    #[test]
    fn test_rule_status_serialization() {
        assert_eq!(
            serde_json::to_string(&LifecycleRuleStatus::Disabled).unwrap(),
            r#""disabled""#
        );
        let parsed: LifecycleRuleStatus = serde_json::from_str(r#""enabled""#).unwrap();
        assert_eq!(parsed, LifecycleRuleStatus::Enabled);
        assert!(serde_json::from_str::<LifecycleRuleStatus>(r#""on""#).is_err());
        assert_eq!(LifecycleRuleStatus::Enabled.label(), "enabled");
        assert_eq!(LifecycleRuleStatus::Disabled.label(), "disabled");
    }
}

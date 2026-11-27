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
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Whether a replication rule is applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum ReplicationRuleStatus {
    #[serde(rename = "enabled")]
    Enabled,
    #[serde(rename = "disabled")]
    Disabled,
}

impl ReplicationRuleStatus {
    fn to_sdk(self) -> aws_sdk_s3::types::ReplicationRuleStatus {
        match self {
            Self::Enabled => aws_sdk_s3::types::ReplicationRuleStatus::Enabled,
            Self::Disabled => aws_sdk_s3::types::ReplicationRuleStatus::Disabled,
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

/// Where a replication rule copies matching object versions to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ReplicationDestination {
    #[schemars(
        description = "Destination bucket as a name or an ARN such as \"arn:aws:s3:::backup\"; AWS S3 requires the ARN form. It must differ from the source bucket, and both buckets need versioning enabled"
    )]
    pub bucket: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Optional storage class of the copies, e.g. STANDARD_IA, GLACIER, or a class the storage backend offers; copies keep the source class when unset"
    )]
    pub storage_class: Option<String>,
}

impl ReplicationDestination {
    /// Build the SDK destination; S3 requires the bucket, so
    /// `validate_replication_rules` must have run first.
    fn to_sdk(&self) -> Result<aws_sdk_s3::types::Destination> {
        aws_sdk_s3::types::Destination::builder()
            .bucket(&self.bucket)
            .set_storage_class(self.storage_class.as_deref().map(Into::into))
            .build()
            .context("Failed to build the replication destination")
    }

    /// Convert a destination as S3 returned it, plus the names of the
    /// settings this crate does not model (empty for a fully modeled one).
    fn from_sdk(destination: &aws_sdk_s3::types::Destination) -> (Self, Vec<String>) {
        let mut warnings = Vec::new();
        let mut unmodeled = Vec::new();
        if destination.account().is_some() {
            unmodeled.push("the destination account");
        }
        if destination.access_control_translation().is_some() {
            unmodeled.push("the destination access control translation");
        }
        if destination.metrics().is_some() {
            unmodeled.push("replication metrics");
        }
        if destination.encryption_configuration().is_some() {
            unmodeled.push("the destination encryption configuration");
        }
        if !unmodeled.is_empty() {
            warnings.push(format!(
                "the destination carries {} this tool does not model; they are dropped when the configuration is written back",
                unmodeled.join(" and ")
            ));
        }

        (
            Self {
                bucket: destination.bucket().to_string(),
                storage_class: destination
                    .storage_class()
                    .map(|class| class.as_str().to_string()),
            },
            warnings,
        )
    }
}

/// One replication rule: which object versions of the bucket are copied to
/// the destination.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ReplicationRule {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Optional unique identifier of the rule (max 255 characters)")]
    pub id: Option<String>,
    #[schemars(description = "Whether the rule is applied: enabled or disabled")]
    pub status: ReplicationRuleStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Optional precedence among rules whose prefixes overlap; S3 keeps the copy of the enabled rule with the highest priority (at least 0, unique within the configuration)"
    )]
    pub priority: Option<i32>,
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
    #[schemars(
        description = "Whether deleting a source object also places a delete marker on the copy (S3 needs it set to true for deletes to replicate)"
    )]
    pub delete_marker_replication: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Whether object versions that already exist are copied once when the rule is created; without it only new versions are copied"
    )]
    pub existing_object_replication: Option<bool>,
    #[schemars(description = "Where matching object versions are copied to")]
    pub destination: ReplicationDestination,
}

/// A prefix that selects something, ignoring S3's empty "no prefix" value.
fn selective_prefix(prefix: Option<&str>) -> Option<String> {
    prefix
        .filter(|prefix| !prefix.is_empty())
        .map(str::to_string)
}

/// The rule's filter: S3 accepts a single predicate directly and requires
/// the `And` operator once a rule selects on a prefix and tags together.
fn filter_to_sdk(rule: &ReplicationRule) -> aws_sdk_s3::types::ReplicationRuleFilter {
    let prefix = selective_prefix(rule.prefix.as_deref());
    let predicates = usize::from(prefix.is_some()) + rule.tags.as_ref().map_or(0, |t| t.len());

    let mut builder = aws_sdk_s3::types::ReplicationRuleFilter::builder();
    if predicates > 1 {
        builder = builder.and(
            aws_sdk_s3::types::ReplicationRuleAndOperator::builder()
                .set_prefix(prefix)
                .set_tags(rule.tags.as_ref().map(tags_to_sdk))
                .build(),
        );
    } else if let Some(prefix) = prefix {
        builder = builder.prefix(prefix);
    } else if let Some(tags) = &rule.tags {
        // Exactly one tag; a plain Tag filter names it directly.
        builder = builder.tag(tags_to_sdk(tags).into_iter().next().expect("one tag"));
    } else {
        // A whole-bucket rule still needs an explicit filter: S3 rejects a
        // rule without one unless it carries the deprecated rule-level
        // Prefix, so select the whole bucket with an empty prefix.
        builder = builder.prefix(String::new());
    }

    builder.build()
}

/// Which objects a rule applies to, read back from an SDK filter.
fn filter_from_sdk(
    rule: &aws_sdk_s3::types::ReplicationRule,
) -> (Option<String>, Option<BTreeMap<String, String>>) {
    match rule.filter() {
        Some(filter) => match filter.and() {
            Some(and) => (selective_prefix(and.prefix()), tags_from_sdk(and.tags())),
            None => (
                selective_prefix(filter.prefix()),
                filter
                    .tag()
                    .map(|tag| BTreeMap::from([(tag.key().to_string(), tag.value().to_string())])),
            ),
        },
        // Older configurations put the prefix next to the rule instead of
        // in a filter.
        None => (legacy_prefix(rule), None),
    }
}

/// Read the deprecated `Prefix` element next to a rule. S3 still returns
/// it for configurations that were written before filters existed.
#[allow(deprecated)]
fn legacy_prefix(rule: &aws_sdk_s3::types::ReplicationRule) -> Option<String> {
    selective_prefix(rule.prefix())
}

/// Tags as the SDK list.
fn tags_to_sdk(tags: &BTreeMap<String, String>) -> Vec<aws_sdk_s3::types::Tag> {
    tags.iter()
        .map(|(key, value)| {
            aws_sdk_s3::types::Tag::builder()
                .key(key)
                .value(value)
                .build()
                .expect("a tag built from a key and a value is always valid")
        })
        .collect()
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

impl ReplicationRule {
    /// Build the SDK rule. Call `validate_replication_rules` first: the
    /// conversion assumes a rule that passed validation.
    pub(super) fn to_sdk(&self) -> Result<aws_sdk_s3::types::ReplicationRule> {
        use aws_sdk_s3::types::{
            DeleteMarkerReplication, DeleteMarkerReplicationStatus, ExistingObjectReplication,
            ExistingObjectReplicationStatus,
        };

        // `Some(true)` for enabled, `Some(false)` for disabled, `None` keeps
        // the element off the wire.
        let delete_marker_replication = self.delete_marker_replication.map(|enabled| {
            DeleteMarkerReplication::builder()
                .status(if enabled {
                    DeleteMarkerReplicationStatus::Enabled
                } else {
                    DeleteMarkerReplicationStatus::Disabled
                })
                .build()
        });

        let existing_object_replication = self
            .existing_object_replication
            .map(|enabled| {
                ExistingObjectReplication::builder()
                    .status(if enabled {
                        ExistingObjectReplicationStatus::Enabled
                    } else {
                        ExistingObjectReplicationStatus::Disabled
                    })
                    .build()
                    .context("Failed to build the existing object replication setting")
            })
            .transpose()?;

        aws_sdk_s3::types::ReplicationRule::builder()
            .status(self.status.to_sdk())
            .set_filter(Some(filter_to_sdk(self)))
            .set_id(self.id.clone())
            .set_priority(self.priority)
            .set_delete_marker_replication(delete_marker_replication)
            .set_existing_object_replication(existing_object_replication)
            .destination(self.destination.to_sdk()?)
            .build()
            .context("Failed to build the replication rule")
    }

    /// Convert a rule as S3 returned it, plus the names of the settings this
    /// crate does not model (empty for a fully modeled rule).
    pub(super) fn from_sdk(rule: &aws_sdk_s3::types::ReplicationRule) -> (Self, Vec<String>) {
        let (prefix, tags) = filter_from_sdk(rule);

        let mut warnings = Vec::new();
        let (destination, destination_warnings) = match rule.destination() {
            Some(destination) => ReplicationDestination::from_sdk(destination),
            None => {
                warnings.push("the rule carries no destination".to_string());
                (
                    ReplicationDestination {
                        bucket: String::new(),
                        storage_class: None,
                    },
                    Vec::new(),
                )
            }
        };
        warnings.extend(destination_warnings);

        let mapped = Self {
            id: rule.id().filter(|id| !id.is_empty()).map(str::to_string),
            status: match rule.status() {
                aws_sdk_s3::types::ReplicationRuleStatus::Enabled => ReplicationRuleStatus::Enabled,
                aws_sdk_s3::types::ReplicationRuleStatus::Disabled => {
                    ReplicationRuleStatus::Disabled
                }
                // A status this crate does not model is reported as the safe
                // choice: not applied.
                _ => ReplicationRuleStatus::Disabled,
            },
            priority: rule.priority(),
            prefix,
            tags,
            delete_marker_replication: rule.delete_marker_replication().map(|setting| {
                matches!(
                    setting.status(),
                    Some(aws_sdk_s3::types::DeleteMarkerReplicationStatus::Enabled)
                )
            }),
            existing_object_replication: rule.existing_object_replication().map(|setting| {
                matches!(
                    setting.status(),
                    aws_sdk_s3::types::ExistingObjectReplicationStatus::Enabled
                )
            }),
            destination,
        };

        (mapped, warnings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule() -> ReplicationRule {
        ReplicationRule {
            id: Some("backup".to_string()),
            status: ReplicationRuleStatus::Enabled,
            priority: Some(1),
            prefix: None,
            tags: None,
            delete_marker_replication: Some(true),
            existing_object_replication: Some(false),
            destination: ReplicationDestination {
                bucket: "arn:aws:s3:::backup".to_string(),
                storage_class: Some("STANDARD_IA".to_string()),
            },
        }
    }

    #[test]
    fn test_rule_round_trips_through_sdk_types() {
        let expected = rule();
        let sdk = expected.to_sdk().unwrap();

        assert_eq!(sdk.id().map(str::to_string), expected.id);
        assert_eq!(sdk.priority(), expected.priority);
        // A whole-bucket rule carries an explicit empty-prefix filter.
        assert_eq!(sdk.filter().unwrap().prefix(), Some(""));
        assert_eq!(
            sdk.delete_marker_replication().unwrap().status(),
            Some(&aws_sdk_s3::types::DeleteMarkerReplicationStatus::Enabled)
        );
        assert_eq!(
            sdk.existing_object_replication().unwrap().status(),
            &aws_sdk_s3::types::ExistingObjectReplicationStatus::Disabled
        );
        assert_eq!(sdk.destination().unwrap().bucket(), "arn:aws:s3:::backup");
        assert_eq!(
            sdk.destination().unwrap().storage_class().unwrap().as_str(),
            "STANDARD_IA"
        );

        let (mapped, warnings) = ReplicationRule::from_sdk(&sdk);
        assert_eq!(mapped, expected);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_single_dimension_filters_round_trip() {
        // A prefix alone is sent as a plain prefix filter.
        let prefixed = ReplicationRule {
            prefix: Some("logs/".to_string()),
            tags: None,
            ..rule()
        };
        let sdk = prefixed.to_sdk().unwrap();
        assert_eq!(sdk.filter().unwrap().prefix(), Some("logs/"));
        assert!(sdk.filter().unwrap().and().is_none());
        let (mapped, warnings) = ReplicationRule::from_sdk(&sdk);
        assert_eq!(mapped, prefixed);
        assert!(warnings.is_empty());

        // A lone tag is sent as a plain tag filter.
        let tagged = ReplicationRule {
            prefix: None,
            tags: Some(BTreeMap::from([("env".to_string(), "prod".to_string())])),
            ..rule()
        };
        let sdk = tagged.to_sdk().unwrap();
        assert_eq!(sdk.filter().unwrap().tag().unwrap().key(), "env");
        assert!(sdk.filter().unwrap().and().is_none());
        assert_eq!(ReplicationRule::from_sdk(&sdk).0, tagged);

        // A prefix and tags together share the `And` operator.
        let combined = ReplicationRule {
            prefix: Some("logs/".to_string()),
            tags: Some(BTreeMap::from([
                ("env".to_string(), "prod".to_string()),
                ("team".to_string(), "storage".to_string()),
            ])),
            ..rule()
        };
        let sdk = combined.to_sdk().unwrap();
        let and = sdk.filter().unwrap().and().unwrap();
        assert_eq!(and.prefix(), Some("logs/"));
        assert_eq!(and.tags().len(), 2);
        assert_eq!(ReplicationRule::from_sdk(&sdk).0, combined);

        // The optional sub-settings can stay unset, which keeps them off the
        // wire instead of sending an explicit disabled.
        let minimal = ReplicationRule {
            delete_marker_replication: None,
            existing_object_replication: None,
            ..rule()
        };
        let sdk = minimal.to_sdk().unwrap();
        assert!(sdk.delete_marker_replication().is_none());
        assert!(sdk.existing_object_replication().is_none());
        assert_eq!(ReplicationRule::from_sdk(&sdk).0, minimal);
    }

    #[test]
    fn test_rule_json_round_trip() {
        let json = r#"{
            "id": "backup",
            "status": "enabled",
            "priority": 1,
            "prefix": "logs/",
            "delete_marker_replication": true,
            "destination": {"bucket": "arn:aws:s3:::backup", "storage_class": "STANDARD_IA"}
        }"#;

        let expected: ReplicationRule = serde_json::from_str(json).unwrap();
        assert_eq!(expected.destination.bucket, "arn:aws:s3:::backup");
        assert_eq!(expected.delete_marker_replication, Some(true));
        assert_eq!(expected.existing_object_replication, None);

        // Unset fields are left out when serialized, so the output of a get
        // can be fed back into a put unchanged.
        let serialized = serde_json::to_string(&expected).unwrap();
        assert!(!serialized.contains("null"));
        let reparsed: ReplicationRule = serde_json::from_str(&serialized).unwrap();
        assert_eq!(reparsed, expected);
    }

    #[test]
    fn test_rule_status_serialization() {
        assert_eq!(
            serde_json::to_string(&ReplicationRuleStatus::Disabled).unwrap(),
            r#""disabled""#
        );
        let parsed: ReplicationRuleStatus = serde_json::from_str(r#""enabled""#).unwrap();
        assert_eq!(parsed, ReplicationRuleStatus::Enabled);
        assert!(serde_json::from_str::<ReplicationRuleStatus>(r#""on""#).is_err());
        assert_eq!(ReplicationRuleStatus::Enabled.label(), "enabled");
        assert_eq!(ReplicationRuleStatus::Disabled.label(), "disabled");
    }
}

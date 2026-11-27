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

use crate::s3_client::{ReplicationDestination, ReplicationRemoval, ReplicationRule};

/// Human-readable listing of replication rules, one block per rule.
pub(super) fn summarize_rules(rules: &[ReplicationRule]) -> String {
    let mut text = String::new();

    for (index, rule) in rules.iter().enumerate() {
        let name = match &rule.id {
            Some(id) => format!("**{id}**"),
            None => format!("rule #{}", index + 1),
        };
        text.push_str(&format!(
            "{}. {name} — {}\n   - Applies to: {}\n   - Copies to: {}\n",
            index + 1,
            rule.status.label(),
            describe_scope(rule),
            describe_destination(&rule.destination),
        ));

        if let Some(priority) = rule.priority {
            text.push_str(&format!("   - Priority: {priority}\n"));
        }
        if let Some(enabled) = rule.delete_marker_replication {
            text.push_str(&format!(
                "   - Delete marker replication: {}\n",
                if enabled { "on" } else { "off" }
            ));
        }
        if let Some(enabled) = rule.existing_object_replication {
            text.push_str(&format!(
                "   - Existing object replication: {}\n",
                if enabled { "on" } else { "off" }
            ));
        }

        text.push('\n');
    }

    text
}

/// Which objects a rule applies to.
fn describe_scope(rule: &ReplicationRule) -> String {
    let mut parts = Vec::new();

    if let Some(prefix) = rule.prefix.as_deref().filter(|prefix| !prefix.is_empty()) {
        parts.push(format!("prefix \"{prefix}\""));
    }

    if let Some(tags) = &rule.tags {
        let tags = tags
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join(", ");
        parts.push(format!("tags {tags}"));
    }

    if parts.is_empty() {
        "the whole bucket".to_string()
    } else {
        parts.join(", ")
    }
}

/// Where a rule copies to, including the storage class when one is set.
fn describe_destination(destination: &ReplicationDestination) -> String {
    match destination.storage_class.as_deref() {
        Some(class) => format!("{} (storage class {class})", destination.bucket),
        None => destination.bucket.clone(),
    }
}

/// The warning about settings the tool does not model, or an empty string
/// when there are none. A replacement of the configuration would drop them,
/// so `put_bucket_replication` refuses such a bucket.
pub(super) fn unmodeled_settings_warning(settings: &[String]) -> String {
    if settings.is_empty() {
        return String::new();
    }
    format!(
        "⚠️ **Settings this tool does not model** (put_bucket_replication refuses to replace this \
         configuration, because its request would drop them; change them with the storage \
         backend's own tooling):\n{}\n\n",
        settings
            .iter()
            .map(|setting| format!("- {setting}"))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

/// The outcome of a removal, by what the storage backend reports of the
/// bucket afterwards.
pub(super) fn removal_text(removal: &ReplicationRemoval, bucket: &str) -> String {
    match removal {
        ReplicationRemoval::Removed => format!(
            "✅ **Bucket replication removed!**\n\n\
             **Bucket:** {bucket}\n\n\
             ℹ️ **Note:** New object versions of this bucket are no longer copied anywhere; \
             copies already in destination buckets stay there."
        ),
        ReplicationRemoval::RulesRemain(config) => format!(
            "⚠️ **Replication rules removed, but the bucket still replicates**\n\n\
             **Bucket:** {bucket}\n\n\
             The removal request was accepted, and the storage backend still reports {} rule(s) \
             that the request does not remove - on RustFS, the rules a site replication manages:\n\n{}\
             {}\
             ⚠️ **Note:** Those rules stay in effect: new object versions keep being copied as \
             they say. Change them with the storage backend's own tooling.",
            config.rules.len(),
            summarize_rules(&config.rules),
            unmodeled_settings_warning(&config.unmodeled_settings)
        ),
        ReplicationRemoval::Unconfirmed(reason) => format!(
            "⚠️ **Replication removal requested, but not confirmed**\n\n\
             **Bucket:** {bucket}\n\n\
             The removal request was accepted, and the replication configuration of the bucket \
             could not be read back afterwards:\n\n\
             {reason}\n\n\
             **Troubleshooting steps:**\n\
             1. Read the configuration with get_bucket_replication\n\
             2. Check your AWS credentials and permissions"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::s3_client::{BucketReplicationConfig, ReplicationRuleStatus};
    use std::collections::BTreeMap;

    #[test]
    fn test_summarize_rules_lists_every_setting() {
        let rules = vec![
            ReplicationRule {
                id: Some("backup".to_string()),
                status: ReplicationRuleStatus::Enabled,
                priority: Some(2),
                prefix: Some("logs/".to_string()),
                tags: Some(BTreeMap::from([(
                    "team".to_string(),
                    "storage".to_string(),
                )])),
                delete_marker_replication: Some(true),
                existing_object_replication: Some(false),
                destination: ReplicationDestination {
                    bucket: "arn:aws:s3:::backup".to_string(),
                    storage_class: Some("STANDARD_IA".to_string()),
                },
            },
            ReplicationRule {
                id: None,
                status: ReplicationRuleStatus::Disabled,
                priority: None,
                prefix: None,
                tags: None,
                delete_marker_replication: None,
                existing_object_replication: None,
                destination: ReplicationDestination {
                    bucket: "mirror".to_string(),
                    storage_class: None,
                },
            },
        ];

        let summary = summarize_rules(&rules);

        assert_eq!(
            summary,
            "1. **backup** — enabled\n\
             \x20  - Applies to: prefix \"logs/\", tags team=storage\n\
             \x20  - Copies to: arn:aws:s3:::backup (storage class STANDARD_IA)\n\
             \x20  - Priority: 2\n\
             \x20  - Delete marker replication: on\n\
             \x20  - Existing object replication: off\n\
             \n\
             2. rule #2 — disabled\n\
             \x20  - Applies to: the whole bucket\n\
             \x20  - Copies to: mirror\n\
             \n"
        );
    }

    #[test]
    fn test_empty_summary_is_empty() {
        assert_eq!(summarize_rules(&[]), "");
    }

    #[test]
    fn test_unmodeled_settings_warning() {
        // Nothing to warn about is nothing at all.
        assert_eq!(unmodeled_settings_warning(&[]), "");

        let warning = unmodeled_settings_warning(&[
            "rule 'backup': replication metrics".to_string(),
            "rule #2: the destination account".to_string(),
        ]);
        assert!(warning.starts_with("⚠️ **Settings this tool does not model**"));
        // The warning says what happens to such a bucket instead of letting
        // a write-back drop the settings.
        assert!(warning.contains("put_bucket_replication refuses to replace this configuration"));
        assert!(warning.ends_with(
            "- rule 'backup': replication metrics\n- rule #2: the destination account\n\n"
        ));
    }

    fn config(bucket: &str, id: &str) -> BucketReplicationConfig {
        BucketReplicationConfig {
            bucket: bucket.to_string(),
            role: Some("arn:rustfs:replication::peer:source".to_string()),
            rules: vec![ReplicationRule {
                id: Some(id.to_string()),
                status: ReplicationRuleStatus::Enabled,
                priority: Some(1),
                prefix: Some("docs/".to_string()),
                tags: None,
                delete_marker_replication: None,
                existing_object_replication: None,
                destination: ReplicationDestination {
                    bucket: "arn:rustfs:replication::peer:source".to_string(),
                    storage_class: None,
                },
            }],
            unmodeled_settings: Vec::new(),
        }
    }

    #[test]
    fn test_removal_text_reports_only_what_the_backend_confirms() {
        // Nothing is left.
        let removed = removal_text(&ReplicationRemoval::Removed, "data");
        assert!(removed.starts_with("✅ **Bucket replication removed!**"));
        assert!(removed.contains("no longer copied anywhere"));
        assert!(!removed.contains("⚠️"));

        // Rules survived the request: the bucket still replicates, and the
        // text never claims otherwise.
        let remains = removal_text(
            &ReplicationRemoval::RulesRemain(config("data", "site-repl-peer")),
            "data",
        );
        assert!(
            remains
                .starts_with("⚠️ **Replication rules removed, but the bucket still replicates**")
        );
        assert!(remains.contains("still reports 1 rule(s)"));
        assert!(remains.contains("**site-repl-peer**"));
        assert!(remains.contains("Those rules stay in effect"));
        assert!(!remains.contains("✅"));
        assert!(!remains.contains("no longer copied"));

        // The read-back failed: the request was accepted, nothing was
        // confirmed, and the reason is shown.
        let unconfirmed = removal_text(
            &ReplicationRemoval::Unconfirmed("service error: InternalError".to_string()),
            "data",
        );
        assert!(unconfirmed.starts_with("⚠️ **Replication removal requested, but not confirmed**"));
        assert!(unconfirmed.contains("service error: InternalError"));
        assert!(unconfirmed.contains("get_bucket_replication"));
        assert!(!unconfirmed.contains("✅"));
    }
}

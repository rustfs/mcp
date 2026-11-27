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

use crate::s3_client::{ReplicationDestination, ReplicationRule};

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::s3_client::ReplicationRuleStatus;
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
}

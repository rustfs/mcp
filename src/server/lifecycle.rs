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

use crate::s3_client::{LifecycleExpiration, LifecycleRule, NoncurrentVersionExpiration};

/// Human-readable listing of lifecycle rules, one block per rule.
pub(super) fn summarize_rules(rules: &[LifecycleRule]) -> String {
    let mut text = String::new();

    for (index, rule) in rules.iter().enumerate() {
        let name = match &rule.id {
            Some(id) => format!("**{id}**"),
            None => format!("rule #{}", index + 1),
        };
        text.push_str(&format!(
            "{}. {name} — {}\n   - Applies to: {}\n",
            index + 1,
            rule.status.label(),
            describe_scope(rule)
        ));

        if let Some(expiration) = &rule.expiration {
            text.push_str(&format!("   - {}\n", describe_expiration(expiration)));
        }

        for transition in &rule.transitions {
            text.push_str(&format!(
                "   - Move to {}: {}\n",
                transition.storage_class,
                describe_when(transition.days, transition.date.as_deref(), "creation")
            ));
        }

        for transition in &rule.noncurrent_version_transitions {
            text.push_str(&format!(
                "   - Move noncurrent versions to {}: {}{}\n",
                transition.storage_class,
                describe_when(Some(transition.days), None, "becoming noncurrent"),
                describe_newer_versions(transition.newer_noncurrent_versions)
            ));
        }

        if let Some(expiration) = &rule.noncurrent_version_expiration {
            text.push_str(&format!(
                "   - Expire noncurrent versions: {}\n",
                describe_noncurrent_expiration(expiration)
            ));
        }

        if let Some(days) = rule.abort_incomplete_multipart_upload_after_days {
            text.push_str(&format!(
                "   - Abort incomplete multipart uploads: {days} day(s) after they start\n"
            ));
        }

        text.push('\n');
    }

    text
}

/// Which objects a rule applies to.
fn describe_scope(rule: &LifecycleRule) -> String {
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

    if let Some(minimum) = rule.object_size_greater_than {
        parts.push(format!("objects larger than {minimum} bytes"));
    }
    if let Some(maximum) = rule.object_size_less_than {
        parts.push(format!("objects smaller than {maximum} bytes"));
    }

    if parts.is_empty() {
        "the whole bucket".to_string()
    } else {
        parts.join(", ")
    }
}

/// When something happens: a number of days after an event, or on a date.
fn describe_when(days: Option<i32>, date: Option<&str>, event: &str) -> String {
    match (days, date) {
        (Some(days), _) => format!("{days} day(s) after {event}"),
        (None, Some(date)) => format!("on {date}"),
        (None, None) => String::new(),
    }
}

/// The condition that a number of newer versions must exist, if there is one.
fn describe_newer_versions(newer_noncurrent_versions: Option<i32>) -> String {
    match newer_noncurrent_versions {
        Some(count) => format!(", once {count} newer version(s) exist"),
        None => String::new(),
    }
}

fn describe_expiration(expiration: &LifecycleExpiration) -> String {
    if expiration.expired_object_delete_marker == Some(true) {
        return "Remove expired delete markers once no older version remains".to_string();
    }
    let when = describe_when(expiration.days, expiration.date.as_deref(), "creation");
    format!("Expire {when}")
}

fn describe_noncurrent_expiration(expiration: &NoncurrentVersionExpiration) -> String {
    match (expiration.days, expiration.newer_noncurrent_versions) {
        (Some(days), newer) => {
            let newer = match newer {
                Some(count) => format!(", but only once {count} newer version(s) exist"),
                None => String::new(),
            };
            format!("{days} day(s) after becoming noncurrent{newer}")
        }
        (None, Some(count)) => format!("keep only the newest {count} noncurrent version(s)"),
        (None, None) => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::s3_client::{LifecycleRuleStatus, LifecycleTransition, NoncurrentVersionTransition};
    use std::collections::BTreeMap;

    #[test]
    fn test_describe_scope() {
        let mut rule = LifecycleRule {
            id: None,
            status: LifecycleRuleStatus::Enabled,
            prefix: None,
            tags: None,
            object_size_greater_than: None,
            object_size_less_than: None,
            expiration: None,
            transitions: Vec::new(),
            noncurrent_version_transitions: Vec::new(),
            noncurrent_version_expiration: None,
            abort_incomplete_multipart_upload_after_days: None,
        };
        assert_eq!(describe_scope(&rule), "the whole bucket");

        rule.prefix = Some(String::new());
        assert_eq!(describe_scope(&rule), "the whole bucket");

        rule.prefix = Some("logs/".to_string());
        rule.tags = Some(BTreeMap::from([(
            "team".to_string(),
            "storage".to_string(),
        )]));
        rule.object_size_greater_than = Some(1024);
        rule.object_size_less_than = Some(1_048_576);
        assert_eq!(
            describe_scope(&rule),
            "prefix \"logs/\", tags team=storage, objects larger than 1024 bytes, objects smaller than 1048576 bytes"
        );
    }

    #[test]
    fn test_describe_when_and_conditions() {
        assert_eq!(
            describe_when(Some(30), None, "creation"),
            "30 day(s) after creation"
        );
        assert_eq!(
            describe_when(None, Some("2026-12-31T00:00:00Z"), "creation"),
            "on 2026-12-31T00:00:00Z"
        );
        assert_eq!(describe_when(None, None, "creation"), "");
        assert_eq!(describe_newer_versions(None), "");
        assert_eq!(
            describe_newer_versions(Some(3)),
            ", once 3 newer version(s) exist"
        );
    }

    #[test]
    fn test_describe_expirations() {
        let by_days = LifecycleExpiration {
            days: Some(30),
            date: None,
            expired_object_delete_marker: None,
        };
        assert_eq!(
            describe_expiration(&by_days),
            "Expire 30 day(s) after creation"
        );

        let markers = LifecycleExpiration {
            days: None,
            date: None,
            expired_object_delete_marker: Some(true),
        };
        assert_eq!(
            describe_expiration(&markers),
            "Remove expired delete markers once no older version remains"
        );

        assert_eq!(
            describe_noncurrent_expiration(&NoncurrentVersionExpiration {
                newer_noncurrent_versions: None,
                days: Some(30),
            }),
            "30 day(s) after becoming noncurrent"
        );
        assert_eq!(
            describe_noncurrent_expiration(&NoncurrentVersionExpiration {
                newer_noncurrent_versions: Some(3),
                days: None,
            }),
            "keep only the newest 3 noncurrent version(s)"
        );
        assert_eq!(
            describe_noncurrent_expiration(&NoncurrentVersionExpiration {
                newer_noncurrent_versions: Some(3),
                days: Some(30),
            }),
            "30 day(s) after becoming noncurrent, but only once 3 newer version(s) exist"
        );
    }

    #[test]
    fn test_summarize_rules_lists_every_action() {
        let rules = vec![
            LifecycleRule {
                id: Some("cleanup".to_string()),
                status: LifecycleRuleStatus::Enabled,
                prefix: Some("logs/".to_string()),
                tags: None,
                object_size_greater_than: None,
                object_size_less_than: None,
                expiration: Some(LifecycleExpiration {
                    days: Some(30),
                    date: None,
                    expired_object_delete_marker: None,
                }),
                transitions: vec![LifecycleTransition {
                    days: Some(7),
                    date: None,
                    storage_class: "GLACIER".to_string(),
                }],
                noncurrent_version_transitions: vec![NoncurrentVersionTransition {
                    newer_noncurrent_versions: Some(3),
                    days: 7,
                    storage_class: "GLACIER".to_string(),
                }],
                noncurrent_version_expiration: Some(NoncurrentVersionExpiration {
                    newer_noncurrent_versions: None,
                    days: Some(30),
                }),
                abort_incomplete_multipart_upload_after_days: Some(7),
            },
            LifecycleRule {
                id: None,
                status: LifecycleRuleStatus::Disabled,
                prefix: None,
                tags: None,
                object_size_greater_than: None,
                object_size_less_than: None,
                expiration: None,
                transitions: Vec::new(),
                noncurrent_version_transitions: Vec::new(),
                noncurrent_version_expiration: None,
                abort_incomplete_multipart_upload_after_days: Some(1),
            },
        ];

        let summary = summarize_rules(&rules);

        assert_eq!(
            summary,
            "1. **cleanup** — enabled\n\
             \x20  - Applies to: prefix \"logs/\"\n\
             \x20  - Expire 30 day(s) after creation\n\
             \x20  - Move to GLACIER: 7 day(s) after creation\n\
             \x20  - Move noncurrent versions to GLACIER: 7 day(s) after becoming noncurrent, once 3 newer version(s) exist\n\
             \x20  - Expire noncurrent versions: 30 day(s) after becoming noncurrent\n\
             \x20  - Abort incomplete multipart uploads: 7 day(s) after they start\n\
             \n\
             2. rule #2 — disabled\n\
             \x20  - Applies to: the whole bucket\n\
             \x20  - Abort incomplete multipart uploads: 1 day(s) after they start\n\
             \n"
        );
    }
}

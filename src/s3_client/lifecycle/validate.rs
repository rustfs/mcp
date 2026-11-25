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

use super::rule::{
    LifecycleExpiration, LifecycleRule, LifecycleTransition, NoncurrentVersionExpiration,
    NoncurrentVersionTransition,
};
use super::{MAX_LIFECYCLE_RULES, MAX_RULE_ID_LENGTH};

/// Validate a lifecycle configuration before it is sent to S3, so that
/// structural problems surface as clear messages instead of an opaque
/// `MalformedXML` error.
pub(super) fn validate_lifecycle_rules(rules: &[LifecycleRule]) -> Result<()> {
    if rules.is_empty() {
        anyhow::bail!("At least one lifecycle rule is required");
    }
    if rules.len() > MAX_LIFECYCLE_RULES {
        anyhow::bail!(
            "Too many lifecycle rules: {} given, S3 allows at most {MAX_LIFECYCLE_RULES}",
            rules.len()
        );
    }

    let mut ids = std::collections::HashSet::new();
    for (index, rule) in rules.iter().enumerate() {
        let label = rule_label(rule, index);
        if let Some(id) = &rule.id {
            if id.is_empty() {
                anyhow::bail!("{label}: the id must not be empty");
            }
            let length = id.chars().count();
            if length > MAX_RULE_ID_LENGTH {
                anyhow::bail!(
                    "{label}: the id is {length} characters long; S3 allows at most {MAX_RULE_ID_LENGTH}"
                );
            }
            if !ids.insert(id.as_str()) {
                anyhow::bail!("{label}: the id is used by more than one rule");
            }
        }
        validate_lifecycle_rule(rule, &label)?;
    }

    Ok(())
}

/// Name a rule in messages: by ID when it has one, by position otherwise.
fn rule_label(rule: &LifecycleRule, index: usize) -> String {
    match &rule.id {
        Some(id) => format!("rule '{id}'"),
        None => format!("rule #{}", index + 1),
    }
}

fn validate_lifecycle_rule(rule: &LifecycleRule, label: &str) -> Result<()> {
    let has_action = rule.expiration.is_some()
        || !rule.transitions.is_empty()
        || !rule.noncurrent_version_transitions.is_empty()
        || rule.noncurrent_version_expiration.is_some()
        || rule.abort_incomplete_multipart_upload_after_days.is_some();
    if !has_action {
        anyhow::bail!(
            "{label}: at least one action is required (expiration, transitions, noncurrent_version_transitions, noncurrent_version_expiration, or abort_incomplete_multipart_upload_after_days)"
        );
    }

    if let Some(tags) = &rule.tags
        && tags.is_empty()
    {
        anyhow::bail!("{label}: tags must contain at least one tag");
    }

    if let Some(minimum) = rule.object_size_greater_than
        && minimum < 0
    {
        anyhow::bail!("{label}: object_size_greater_than must not be negative");
    }
    if let Some(maximum) = rule.object_size_less_than
        && maximum < 1
    {
        anyhow::bail!("{label}: object_size_less_than must be at least 1");
    }
    if let (Some(minimum), Some(maximum)) =
        (rule.object_size_greater_than, rule.object_size_less_than)
        && minimum >= maximum
    {
        anyhow::bail!(
            "{label}: object_size_greater_than ({minimum}) must be smaller than object_size_less_than ({maximum})"
        );
    }

    if let Some(expiration) = &rule.expiration {
        validate_expiration(expiration, label)?;
    }

    for transition in &rule.transitions {
        validate_transition(transition, label)?;
    }

    for transition in &rule.noncurrent_version_transitions {
        validate_noncurrent_transition(transition, label)?;
    }

    if let Some(expiration) = &rule.noncurrent_version_expiration {
        validate_noncurrent_expiration(expiration, label)?;
    }

    if let Some(days) = rule.abort_incomplete_multipart_upload_after_days
        && days < 1
    {
        anyhow::bail!(
            "{label}: abort_incomplete_multipart_upload_after_days must be at least 1, got {days}"
        );
    }

    Ok(())
}

fn validate_expiration(expiration: &LifecycleExpiration, label: &str) -> Result<()> {
    let chosen = usize::from(expiration.days.is_some())
        + usize::from(expiration.date.is_some())
        + usize::from(expiration.expired_object_delete_marker.is_some());
    if chosen != 1 {
        anyhow::bail!(
            "{label}: expiration needs exactly one of days, date, or expired_object_delete_marker"
        );
    }

    if let Some(days) = expiration.days
        && days < 1
    {
        anyhow::bail!("{label}: expiration days must be at least 1, got {days}");
    }

    if let Some(date) = &expiration.date {
        check_date(date, label, "expiration date")?;
    }

    Ok(())
}

fn validate_transition(transition: &LifecycleTransition, label: &str) -> Result<()> {
    if transition.days.is_some() == transition.date.is_some() {
        anyhow::bail!("{label}: every transition needs exactly one of days or date");
    }

    if let Some(days) = transition.days
        && days < 0
    {
        anyhow::bail!("{label}: transition days must not be negative, got {days}");
    }

    if transition.storage_class.is_empty() {
        anyhow::bail!("{label}: every transition needs a storage_class");
    }

    if let Some(date) = &transition.date {
        check_date(date, label, "transition date")?;
    }

    Ok(())
}

fn validate_noncurrent_transition(
    transition: &NoncurrentVersionTransition,
    label: &str,
) -> Result<()> {
    if transition.days < 1 {
        anyhow::bail!(
            "{label}: noncurrent version transition days must be at least 1, got {}",
            transition.days
        );
    }

    check_newer_noncurrent_versions(transition.newer_noncurrent_versions, label)?;

    if transition.storage_class.is_empty() {
        anyhow::bail!("{label}: every noncurrent version transition needs a storage_class");
    }

    Ok(())
}

fn validate_noncurrent_expiration(
    expiration: &NoncurrentVersionExpiration,
    label: &str,
) -> Result<()> {
    if expiration.days.is_none() && expiration.newer_noncurrent_versions.is_none() {
        anyhow::bail!(
            "{label}: noncurrent_version_expiration needs days, newer_noncurrent_versions, or both"
        );
    }

    if let Some(days) = expiration.days
        && days < 1
    {
        anyhow::bail!("{label}: noncurrent version expiration days must be at least 1, got {days}");
    }

    check_newer_noncurrent_versions(expiration.newer_noncurrent_versions, label)?;

    Ok(())
}

fn check_newer_noncurrent_versions(count: Option<i32>, label: &str) -> Result<()> {
    if let Some(count) = count
        && count < 1
    {
        anyhow::bail!("{label}: newer_noncurrent_versions must be at least 1, got {count}");
    }

    Ok(())
}

/// Accept only timestamps S3 can parse.
fn check_date(date: &str, label: &str, what: &str) -> Result<()> {
    DateTime::from_str(date, DateTimeFormat::DateTime).with_context(|| {
        format!(
            "{label}: the {what} '{date}' is not an ISO 8601 timestamp such as 2026-12-31T00:00:00Z"
        )
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// A minimal valid rule: expire after 30 days.
    fn rule(id: &str) -> LifecycleRule {
        LifecycleRule {
            id: Some(id.to_string()),
            status: super::super::LifecycleRuleStatus::Enabled,
            prefix: None,
            tags: None,
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
        }
    }

    fn error_of(rules: &[LifecycleRule]) -> String {
        validate_lifecycle_rules(rules).unwrap_err().to_string()
    }

    #[test]
    fn test_valid_configurations_pass() {
        assert!(validate_lifecycle_rules(&[rule("a"), rule("b")]).is_ok());

        let mut dated = rule("dated");
        dated.expiration = Some(LifecycleExpiration {
            days: None,
            date: Some("2026-12-31T00:00:00Z".to_string()),
            expired_object_delete_marker: None,
        });
        assert!(validate_lifecycle_rules(&[dated]).is_ok());

        let mut everything = rule("everything");
        everything.prefix = Some("logs/".to_string());
        everything.tags = Some(BTreeMap::from([(
            "team".to_string(),
            "storage".to_string(),
        )]));
        everything.object_size_greater_than = Some(0);
        everything.object_size_less_than = Some(1024);
        everything.transitions = vec![
            LifecycleTransition {
                days: Some(0),
                date: None,
                storage_class: "GLACIER".to_string(),
            },
            LifecycleTransition {
                days: None,
                date: Some("2026-12-31T00:00:00.000Z".to_string()),
                storage_class: "DEEP_ARCHIVE".to_string(),
            },
        ];
        everything.noncurrent_version_transitions = vec![NoncurrentVersionTransition {
            newer_noncurrent_versions: Some(3),
            days: 7,
            storage_class: "GLACIER".to_string(),
        }];
        everything.noncurrent_version_expiration = Some(NoncurrentVersionExpiration {
            newer_noncurrent_versions: Some(3),
            days: Some(30),
        });
        everything.abort_incomplete_multipart_upload_after_days = Some(7);
        assert!(validate_lifecycle_rules(&[everything]).is_ok());

        // Rules without an ID are fine.
        let mut anonymous = rule("x");
        anonymous.id = None;
        assert!(validate_lifecycle_rules(&[anonymous]).is_ok());
    }

    #[test]
    fn test_rejects_empty_and_oversized_configurations() {
        assert!(error_of(&[]).contains("At least one lifecycle rule"));

        let too_many = vec![rule("same"); MAX_LIFECYCLE_RULES + 1];
        assert!(error_of(&too_many).contains("at most 1000"));
    }

    #[test]
    fn test_rejects_invalid_ids() {
        let empty = rule("");
        assert!(error_of(std::slice::from_ref(&empty)).contains("the id must not be empty"));

        let mut long = rule(&"i".repeat(MAX_RULE_ID_LENGTH + 1));
        // Keep the rule's own ID as the one under test.
        assert!(error_of(&[long.clone()]).contains("256 characters"));

        // IDs only need to be unique per configuration.
        assert!(error_of(&[rule("dup"), rule("dup")]).contains("used by more than one rule"));

        // The length limit counts characters, not bytes.
        long.id = Some("文".repeat(MAX_RULE_ID_LENGTH));
        assert!(validate_lifecycle_rules(&[long]).is_ok());
    }

    #[test]
    fn test_rejects_rule_without_action() {
        let mut no_action = rule("idle");
        no_action.expiration = None;
        let error = error_of(&[no_action.clone()]);
        assert!(error.contains("rule 'idle': at least one action is required"));

        // Without an ID the position names the rule.
        let mut anonymous = no_action.clone();
        anonymous.id = None;
        assert!(error_of(&[anonymous]).contains("rule #1: at least one action"));
    }

    #[test]
    fn test_rejects_invalid_expiration() {
        let mut none = rule("x");
        none.expiration = Some(LifecycleExpiration {
            days: None,
            date: None,
            expired_object_delete_marker: None,
        });
        assert!(error_of(&[none]).contains("needs exactly one of days, date"));

        let mut both = rule("x");
        both.expiration = Some(LifecycleExpiration {
            days: Some(1),
            date: Some("2026-12-31T00:00:00Z".to_string()),
            expired_object_delete_marker: None,
        });
        assert!(error_of(&[both]).contains("needs exactly one of days, date"));

        let mut zero = rule("x");
        zero.expiration = Some(LifecycleExpiration {
            days: Some(0),
            date: None,
            expired_object_delete_marker: None,
        });
        assert!(error_of(&[zero]).contains("expiration days must be at least 1, got 0"));

        let mut dated = rule("x");
        dated.expiration = Some(LifecycleExpiration {
            days: None,
            date: Some("31/12/2026".to_string()),
            expired_object_delete_marker: None,
        });
        let error = error_of(&[dated]);
        assert!(error.contains("the expiration date '31/12/2026' is not an ISO 8601 timestamp"));
    }

    #[test]
    fn test_rejects_invalid_transitions() {
        let neither = LifecycleTransition {
            days: None,
            date: None,
            storage_class: "GLACIER".to_string(),
        };
        let mut config = rule("x");
        config.transitions = vec![neither];
        assert!(error_of(&[config]).contains("every transition needs exactly one of days or date"));

        let mut config = rule("x");
        config.transitions = vec![LifecycleTransition {
            days: Some(1),
            date: Some("2026-12-31T00:00:00Z".to_string()),
            storage_class: "GLACIER".to_string(),
        }];
        assert!(error_of(&[config]).contains("every transition needs exactly one of days or date"));

        let mut config = rule("x");
        config.transitions = vec![LifecycleTransition {
            days: Some(-1),
            date: None,
            storage_class: "GLACIER".to_string(),
        }];
        assert!(error_of(&[config]).contains("transition days must not be negative, got -1"));

        let mut config = rule("x");
        config.transitions = vec![LifecycleTransition {
            days: Some(1),
            date: None,
            storage_class: String::new(),
        }];
        assert!(error_of(&[config]).contains("every transition needs a storage_class"));

        let mut config = rule("x");
        config.transitions = vec![LifecycleTransition {
            days: None,
            date: Some("soon".to_string()),
            storage_class: "GLACIER".to_string(),
        }];
        assert!(
            error_of(&[config]).contains("the transition date 'soon' is not an ISO 8601 timestamp")
        );
    }

    #[test]
    fn test_rejects_invalid_noncurrent_actions() {
        let mut config = rule("x");
        config.noncurrent_version_transitions = vec![NoncurrentVersionTransition {
            newer_noncurrent_versions: None,
            days: 0,
            storage_class: "GLACIER".to_string(),
        }];
        assert!(
            error_of(&[config]).contains("noncurrent version transition days must be at least 1")
        );

        let mut config = rule("x");
        config.noncurrent_version_transitions = vec![NoncurrentVersionTransition {
            newer_noncurrent_versions: Some(0),
            days: 7,
            storage_class: "GLACIER".to_string(),
        }];
        assert!(
            error_of(&[config]).contains("newer_noncurrent_versions must be at least 1, got 0")
        );

        let mut config = rule("x");
        config.noncurrent_version_expiration = Some(NoncurrentVersionExpiration {
            newer_noncurrent_versions: None,
            days: None,
        });
        assert!(error_of(&[config]).contains("noncurrent_version_expiration needs days"));

        let mut config = rule("x");
        config.noncurrent_version_expiration = Some(NoncurrentVersionExpiration {
            newer_noncurrent_versions: None,
            days: Some(0),
        });
        assert!(
            error_of(&[config])
                .contains("noncurrent version expiration days must be at least 1, got 0")
        );
    }

    #[test]
    fn test_rejects_invalid_filters_and_abort_days() {
        let mut config = rule("x");
        config.tags = Some(BTreeMap::new());
        assert!(error_of(&[config]).contains("tags must contain at least one tag"));

        let mut config = rule("x");
        config.object_size_greater_than = Some(-1);
        assert!(error_of(&[config]).contains("object_size_greater_than must not be negative"));

        let mut config = rule("x");
        config.object_size_less_than = Some(0);
        assert!(error_of(&[config]).contains("object_size_less_than must be at least 1"));

        let mut config = rule("x");
        config.object_size_greater_than = Some(1024);
        config.object_size_less_than = Some(1024);
        assert!(error_of(&[config]).contains("must be smaller than object_size_less_than (1024)"));

        let mut config = rule("x");
        config.abort_incomplete_multipart_upload_after_days = Some(0);
        assert!(
            error_of(&[config])
                .contains("abort_incomplete_multipart_upload_after_days must be at least 1")
        );
    }
}

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

use anyhow::Result;

use super::MAX_REPLICATION_RULES;
use super::rule::ReplicationRule;
use crate::s3_client::MAX_RULE_ID_LENGTH;

/// Validate a replication configuration before it is sent to S3, so that
/// structural problems surface as clear messages instead of an opaque
/// `MalformedXML` error.
pub(super) fn validate_replication_rules(
    source_bucket: &str,
    rules: &[ReplicationRule],
) -> Result<()> {
    if rules.is_empty() {
        anyhow::bail!("At least one replication rule is required");
    }
    if rules.len() > MAX_REPLICATION_RULES {
        anyhow::bail!(
            "Too many replication rules: {} given, S3 allows at most {MAX_REPLICATION_RULES}",
            rules.len()
        );
    }

    let mut ids = std::collections::HashSet::new();
    let mut priorities = std::collections::HashSet::new();
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

        if let Some(priority) = rule.priority
            && priority < 0
        {
            anyhow::bail!("{label}: the priority must be at least 0, got {priority}");
        }
        if let Some(priority) = rule.priority
            && !priorities.insert(priority)
        {
            anyhow::bail!("{label}: the priority {priority} is used by more than one rule");
        }

        if let Some(tags) = &rule.tags
            && tags.is_empty()
        {
            anyhow::bail!("{label}: tags must contain at least one tag");
        }

        let destination = rule.destination.bucket.trim();
        if destination.is_empty() {
            anyhow::bail!("{label}: the destination needs a bucket");
        }
        if same_bucket(source_bucket, destination) {
            anyhow::bail!(
                "{label}: the destination bucket must differ from the source bucket '{source_bucket}'"
            );
        }
    }

    Ok(())
}

/// Name a rule in messages: by ID when it has one, by position otherwise.
fn rule_label(rule: &ReplicationRule, index: usize) -> String {
    match &rule.id {
        Some(id) => format!("rule '{id}'"),
        None => format!("rule #{}", index + 1),
    }
}

/// Whether a destination names the source bucket, accepting bucket names and
/// ARN forms such as `arn:aws:s3:::backup`.
fn same_bucket(source: &str, destination: &str) -> bool {
    let destination = destination
        .strip_prefix("arn:aws:s3:::")
        .unwrap_or(destination);
    source == destination
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal valid rule: copy everything to the backup bucket.
    fn rule(id: &str) -> ReplicationRule {
        ReplicationRule {
            id: Some(id.to_string()),
            status: super::super::ReplicationRuleStatus::Enabled,
            priority: None,
            prefix: None,
            tags: None,
            delete_marker_replication: None,
            existing_object_replication: None,
            destination: super::super::ReplicationDestination {
                bucket: "arn:aws:s3:::backup".to_string(),
                storage_class: None,
            },
        }
    }

    fn error_of(source: &str, rules: &[ReplicationRule]) -> String {
        validate_replication_rules(source, rules)
            .unwrap_err()
            .to_string()
    }

    #[test]
    fn test_valid_configurations_pass() {
        assert!(validate_replication_rules("source", &[rule("a"), rule("b")]).is_ok());

        // Rules without an ID are fine, and so is a plain bucket name as the
        // destination.
        let mut anonymous = rule("x");
        anonymous.id = None;
        anonymous.destination.bucket = "backup".to_string();
        assert!(validate_replication_rules("source", &[anonymous]).is_ok());

        // ARN destinations never count as the source bucket itself.
        let self_arn = ReplicationRule {
            destination: super::super::ReplicationDestination {
                bucket: "arn:aws:s3:::source".to_string(),
                storage_class: None,
            },
            ..rule("self")
        };
        let error = error_of("source", &[self_arn]);
        assert!(
            error.contains("must differ from the source bucket 'source'"),
            "{error}"
        );
    }

    #[test]
    fn test_rejects_empty_and_oversized_configurations() {
        assert!(error_of("source", &[]).contains("At least one replication rule"));

        let too_many = vec![rule("same"); MAX_REPLICATION_RULES + 1];
        assert!(error_of("source", &too_many).contains("at most 1000"));
    }

    #[test]
    fn test_rejects_invalid_ids_and_priorities() {
        let empty = rule("");
        assert!(
            error_of("source", std::slice::from_ref(&empty)).contains("the id must not be empty")
        );

        let long = rule(&"i".repeat(MAX_RULE_ID_LENGTH + 1));
        assert!(error_of("source", std::slice::from_ref(&long)).contains("256 characters"));

        assert!(
            error_of("source", &[rule("dup"), rule("dup")]).contains("used by more than one rule")
        );

        let mut low = rule("low");
        low.priority = Some(-1);
        assert!(error_of("source", &[low]).contains("the priority must be at least 0, got -1"));

        let mut first = rule("first");
        first.priority = Some(3);
        let mut second = rule("second");
        second.priority = Some(3);
        let error = error_of("source", &[first, second]);
        assert!(
            error.contains("the priority 3 is used by more than one rule"),
            "{error}"
        );
    }

    #[test]
    fn test_rejects_invalid_destinations() {
        let mut none = rule("x");
        none.destination.bucket = String::new();
        assert!(error_of("source", &[none]).contains("the destination needs a bucket"));

        let mut blank = rule("x");
        blank.destination.bucket = "   ".to_string();
        assert!(error_of("source", &[blank]).contains("the destination needs a bucket"));

        let mut itself = rule("x");
        itself.destination.bucket = "source".to_string();
        assert!(error_of("source", &[itself]).contains("must differ from the source bucket"));

        let mut empty_tags = rule("x");
        empty_tags.tags = Some(std::collections::BTreeMap::new());
        assert!(error_of("source", &[empty_tags]).contains("tags must contain at least one tag"));
    }
}

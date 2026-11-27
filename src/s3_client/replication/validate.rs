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
use super::get_bucket_replication::rule_label;
use super::rule::{ReplicationRule, selects_by_single_tag};
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
    // MinIO and RustFS count a rule without a priority as 0 and refuse a
    // configuration whose priorities are not unique, so every rule needs
    // its own.
    let mut priorities: std::collections::HashMap<i32, (String, bool)> =
        std::collections::HashMap::new();
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

        let priority = rule.priority.unwrap_or(0);
        if priority < 0 {
            anyhow::bail!("{label}: the priority must be at least 0, got {priority}");
        }
        if let Some((other, other_explicit)) =
            priorities.insert(priority, (label.clone(), rule.priority.is_some()))
        {
            let counts_as = if rule.priority.is_some() && other_explicit {
                String::new()
            } else {
                " (a rule without a priority counts as 0)".to_string()
            };
            anyhow::bail!(
                "{label}: the priority {priority} is already used by {other}{counts_as}; MinIO and RustFS refuse priorities that are not unique"
            );
        }

        if let Some(tags) = &rule.tags
            && tags.is_empty()
        {
            anyhow::bail!("{label}: tags must contain at least one tag");
        }

        // MinIO and RustFS refuse delete marker replication on a rule whose
        // filter is a single tag.
        if rule.delete_marker_replication == Some(true) && selects_by_single_tag(rule) {
            anyhow::bail!(
                "{label}: delete_marker_replication cannot be enabled on a rule that selects by a single tag; MinIO and RustFS refuse it. Leave delete_marker_replication off, or drop the tags from the rule"
            );
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
        assert!(validate_replication_rules("source", &[rule("a")]).is_ok());

        // A rule without a priority counts as 0, which is unique as long as
        // no other rule claims 0.
        let mut explicit = rule("b");
        explicit.priority = Some(1);
        assert!(validate_replication_rules("source", &[rule("a"), explicit]).is_ok());

        // Rules without an ID are fine, and so is a plain bucket name as the
        // destination.
        let mut anonymous = rule("x");
        anonymous.id = None;
        anonymous.destination.bucket = "backup".to_string();
        assert!(validate_replication_rules("source", &[anonymous]).is_ok());

        // An ARN that names the source bucket counts as the source bucket.
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

        // A target ARN of MinIO or RustFS names the deployment, so a bucket
        // of the same name on another deployment is a valid destination.
        let mut same_name_elsewhere = rule("same-name");
        same_name_elsewhere.destination.bucket = "arn:rustfs:replication::peer:source".to_string();
        assert!(validate_replication_rules("source", &[same_name_elsewhere]).is_ok());
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
            error.contains(
                "rule 'second': the priority 3 is already used by rule 'first'; MinIO and RustFS refuse priorities that are not unique"
            ),
            "{error}"
        );
    }

    #[test]
    fn test_rejects_priorities_that_collide_once_unset_ones_count_as_zero() {
        // Two rules without a priority both count as 0.
        let error = error_of("source", &[rule("a"), rule("b")]);
        assert!(
            error.contains(
                "rule 'b': the priority 0 is already used by rule 'a' (a rule without a priority counts as 0)"
            ),
            "{error}"
        );

        // An unset priority collides with an explicit 0 as well, whichever
        // way round they come.
        let mut explicit_zero = rule("zero");
        explicit_zero.priority = Some(0);
        let error = error_of("source", &[explicit_zero.clone(), rule("unset")]);
        assert!(
            error.contains("the priority 0 is already used by rule 'zero' (a rule without a priority counts as 0)"),
            "{error}"
        );
        let error = error_of("source", &[rule("unset"), explicit_zero]);
        assert!(
            error.contains("the priority 0 is already used by rule 'unset' (a rule without a priority counts as 0)"),
            "{error}"
        );
    }

    #[test]
    fn test_rejects_delete_marker_replication_on_a_single_tag_rule() {
        let tag = std::collections::BTreeMap::from([("env".to_string(), "prod".to_string())]);

        // A lone tag goes on the wire as a plain Tag filter.
        let mut single_tag = rule("x");
        single_tag.tags = Some(tag.clone());
        single_tag.delete_marker_replication = Some(true);
        assert!(error_of("source", &[single_tag]).contains(
            "delete_marker_replication cannot be enabled on a rule that selects by a single tag"
        ));

        // Off, or not set at all, is fine.
        let mut off = rule("x");
        off.tags = Some(tag.clone());
        off.delete_marker_replication = Some(false);
        assert!(validate_replication_rules("source", &[off]).is_ok());
        let mut unset = rule("x");
        unset.tags = Some(tag.clone());
        assert!(validate_replication_rules("source", &[unset]).is_ok());

        // A prefix beside the tag, or several tags, travel inside an And
        // filter, which is not a plain Tag filter.
        let mut prefixed = rule("x");
        prefixed.prefix = Some("logs/".to_string());
        prefixed.tags = Some(tag);
        prefixed.delete_marker_replication = Some(true);
        assert!(validate_replication_rules("source", &[prefixed]).is_ok());
        let mut several = rule("x");
        several.tags = Some(std::collections::BTreeMap::from([
            ("a".to_string(), "1".to_string()),
            ("b".to_string(), "2".to_string()),
        ]));
        several.delete_marker_replication = Some(true);
        assert!(validate_replication_rules("source", &[several]).is_ok());
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

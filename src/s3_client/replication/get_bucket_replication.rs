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
use aws_sdk_s3::error::ProvideErrorMetadata;
use serde::{Deserialize, Serialize};
use tracing::debug;

use super::super::S3Client;
use super::rule::ReplicationRule;

/// Error code S3 answers with when a bucket has no replication
/// configuration.
const NO_REPLICATION_ERROR_CODE: &str = "NoSuchReplicationConfiguration";

/// Result payload for reading a bucket's replication configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BucketReplicationConfig {
    pub bucket: String,
    /// IAM role S3 assumes when copying objects; `None` when the bucket has
    /// no replication configuration.
    pub role: Option<String>,
    /// Empty when the bucket has no replication configuration.
    pub rules: Vec<ReplicationRule>,
    /// Settings the backend reports that this crate does not model; they are
    /// dropped when the configuration is written back unchanged.
    pub warnings: Vec<String>,
}

impl S3Client {
    /// Read a bucket's replication configuration.
    pub async fn get_bucket_replication(
        &self,
        bucket_name: &str,
    ) -> Result<BucketReplicationConfig> {
        debug!(
            "Fetching the replication configuration of bucket '{}'",
            bucket_name
        );

        let response = match self
            .client
            .get_bucket_replication()
            .bucket(bucket_name)
            .send()
            .await
        {
            Ok(response) => response,
            // A bucket without a replication configuration answers with a
            // dedicated error code; report that as "no rules" instead of a
            // failure.
            Err(err)
                if err.as_service_error().and_then(|e| e.code())
                    == Some(NO_REPLICATION_ERROR_CODE) =>
            {
                return Ok(BucketReplicationConfig {
                    bucket: bucket_name.to_string(),
                    role: None,
                    rules: Vec::new(),
                    warnings: Vec::new(),
                });
            }
            Err(err) => {
                return Err(err).context(format!(
                    "Failed to get the replication configuration of bucket '{bucket_name}'"
                ));
            }
        };

        // An answer without a configuration must not be mistaken for a
        // bucket without replication: the server reported something else.
        let configuration = response
            .replication_configuration()
            .context("The storage backend answered without a replication configuration")?;

        let role = Some(configuration.role())
            .filter(|role| !role.is_empty())
            .map(str::to_string);
        let mut rules = Vec::new();
        let mut warnings = Vec::new();
        for (index, rule) in configuration.rules().iter().enumerate() {
            let (mapped, rule_warnings) = ReplicationRule::from_sdk(rule);
            let label = match &mapped.id {
                Some(id) => format!("rule '{id}'"),
                None => format!("rule #{}", index + 1),
            };
            warnings.extend(
                rule_warnings
                    .into_iter()
                    .map(|warning| format!("{label}: {warning}")),
            );
            rules.push(mapped);
        }

        debug!(
            "Bucket '{}' has {} replication rule(s)",
            bucket_name,
            rules.len()
        );

        Ok(BucketReplicationConfig {
            bucket: bucket_name.to_string(),
            role,
            rules,
            warnings,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bucket_replication_config_serialization() {
        let config = BucketReplicationConfig {
            bucket: "source".to_string(),
            role: Some("arn:aws:iam::123456789012:role/replicator".to_string()),
            rules: vec![ReplicationRule {
                id: Some("backup".to_string()),
                status: crate::s3_client::ReplicationRuleStatus::Enabled,
                priority: Some(1),
                prefix: None,
                tags: None,
                delete_marker_replication: Some(true),
                existing_object_replication: None,
                destination: crate::s3_client::ReplicationDestination {
                    bucket: "arn:aws:s3:::backup".to_string(),
                    storage_class: None,
                },
            }],
            warnings: vec!["rule 'backup': the destination carries replication metrics this tool does not model; they are dropped when the configuration is written back".to_string()],
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: BucketReplicationConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.bucket, "source");
        assert_eq!(deserialized.rules.len(), 1);
        assert_eq!(deserialized.warnings.len(), 1);
    }

    #[test]
    fn test_bucket_replication_without_config_serialization() {
        // A bucket without replication is a config without rules or role.
        let json = r#"{"bucket": "source", "role": null, "rules": [], "warnings": []}"#;
        let deserialized: BucketReplicationConfig = serde_json::from_str(json).unwrap();
        assert!(deserialized.role.is_none());
        assert!(deserialized.rules.is_empty());
    }
}

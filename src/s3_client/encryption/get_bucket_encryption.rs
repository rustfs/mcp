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
use super::ServerSideEncryptionRule;

/// Result payload for reading a bucket's default encryption configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BucketEncryptionConfig {
    pub bucket: String,
    /// Empty when the bucket has no default encryption configuration.
    pub rules: Vec<ServerSideEncryptionRule>,
}

impl S3Client {
    /// Read a bucket's default encryption configuration.
    ///
    /// A bucket without a configuration is reported as having no rules.
    pub async fn get_bucket_encryption(&self, bucket_name: &str) -> Result<BucketEncryptionConfig> {
        debug!(
            "Fetching encryption configuration for bucket '{}'",
            bucket_name
        );

        let response = match self
            .client
            .get_bucket_encryption()
            .bucket(bucket_name)
            .send()
            .await
        {
            Ok(response) => response,
            // A bucket without a default encryption configuration answers with a
            // dedicated error code; report that as "no rules" instead of a failure.
            Err(err)
                if err.as_service_error().and_then(|e| e.code())
                    == Some("ServerSideEncryptionConfigurationNotFoundError") =>
            {
                return Ok(BucketEncryptionConfig {
                    bucket: bucket_name.to_string(),
                    rules: Vec::new(),
                });
            }
            Err(err) => {
                return Err(err).context(format!(
                    "Failed to get the encryption configuration of bucket '{bucket_name}'"
                ));
            }
        };

        // A rule without a default encryption block has no counterpart in
        // `ServerSideEncryptionRule` and is skipped.
        let rules: Vec<ServerSideEncryptionRule> = response
            .server_side_encryption_configuration()
            .map(aws_sdk_s3::types::ServerSideEncryptionConfiguration::rules)
            .unwrap_or_default()
            .iter()
            .filter_map(ServerSideEncryptionRule::from_sdk)
            .collect();

        debug!(
            "Bucket '{}' has {} default encryption rule(s)",
            bucket_name,
            rules.len()
        );

        Ok(BucketEncryptionConfig {
            bucket: bucket_name.to_string(),
            rules,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bucket_encryption_config_serialization() {
        let result = BucketEncryptionConfig {
            bucket: "test-bucket".to_string(),
            rules: vec![ServerSideEncryptionRule {
                apply_server_side_encryption_by_default:
                    super::super::ServerSideEncryptionByDefault {
                        sse_algorithm: "aws:kms".to_string(),
                        kms_key_id: Some("arn:aws:kms:us-east-1:111122223333:key/abc".to_string()),
                    },
                bucket_key_enabled: Some(true),
            }],
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: BucketEncryptionConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.bucket, "test-bucket");
        assert_eq!(deserialized.rules.len(), 1);
        assert_eq!(
            deserialized.rules[0]
                .apply_server_side_encryption_by_default
                .sse_algorithm,
            "aws:kms"
        );
        assert_eq!(deserialized.rules[0].bucket_key_enabled, Some(true));

        let unconfigured: BucketEncryptionConfig =
            serde_json::from_str(r#"{"bucket": "test-bucket", "rules": []}"#).unwrap();
        assert!(unconfigured.rules.is_empty());
    }
}

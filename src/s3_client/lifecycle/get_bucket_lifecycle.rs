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
use super::rule::LifecycleRule;

/// Error code S3 answers with when a bucket has no lifecycle configuration.
const NO_LIFECYCLE_ERROR_CODE: &str = "NoSuchLifecycleConfiguration";

/// Result payload for reading a bucket's lifecycle configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BucketLifecycleConfig {
    pub bucket: String,
    /// Empty when the bucket has no lifecycle configuration.
    pub rules: Vec<LifecycleRule>,
}

impl S3Client {
    /// Read a bucket's lifecycle rules.
    pub async fn get_bucket_lifecycle(&self, bucket_name: &str) -> Result<BucketLifecycleConfig> {
        debug!(
            "Fetching the lifecycle configuration of bucket '{}'",
            bucket_name
        );

        let response = match self
            .client
            .get_bucket_lifecycle_configuration()
            .bucket(bucket_name)
            .send()
            .await
        {
            Ok(response) => response,
            // A bucket without a lifecycle configuration answers with a
            // dedicated error code; report that as "no rules" instead of a
            // failure.
            Err(err)
                if err.as_service_error().and_then(|e| e.code())
                    == Some(NO_LIFECYCLE_ERROR_CODE) =>
            {
                return Ok(BucketLifecycleConfig {
                    bucket: bucket_name.to_string(),
                    rules: Vec::new(),
                });
            }
            Err(err) => {
                return Err(err).context(format!(
                    "Failed to get the lifecycle configuration of bucket '{bucket_name}'"
                ));
            }
        };

        Ok(BucketLifecycleConfig {
            bucket: bucket_name.to_string(),
            rules: response
                .rules()
                .iter()
                .map(LifecycleRule::from_sdk)
                .collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bucket_lifecycle_config_serialization() {
        let config = BucketLifecycleConfig {
            bucket: "logs".to_string(),
            rules: vec![LifecycleRule {
                id: Some("expire".to_string()),
                status: super::super::LifecycleRuleStatus::Enabled,
                prefix: None,
                tags: None,
                object_size_greater_than: None,
                object_size_less_than: None,
                expiration: Some(super::super::LifecycleExpiration {
                    days: Some(30),
                    date: None,
                    expired_object_delete_marker: None,
                }),
                transitions: Vec::new(),
                noncurrent_version_transitions: Vec::new(),
                noncurrent_version_expiration: None,
                abort_incomplete_multipart_upload_after_days: None,
            }],
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: BucketLifecycleConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.bucket, "logs");
        assert_eq!(deserialized.rules.len(), 1);
        assert_eq!(deserialized.rules[0].id.as_deref(), Some("expire"));
    }
}

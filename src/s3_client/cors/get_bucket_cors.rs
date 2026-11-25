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
use super::CorsRule;

/// Result payload for reading a bucket's CORS configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BucketCorsConfig {
    pub bucket: String,
    /// Empty when the bucket has no CORS configuration.
    pub rules: Vec<CorsRule>,
}

impl S3Client {
    /// Read a bucket's CORS configuration.
    ///
    /// A bucket without a CORS configuration is returned as a configuration
    /// with no rules: S3 signals that state with a dedicated error code
    /// instead of with an empty response.
    pub async fn get_bucket_cors(&self, bucket_name: &str) -> Result<BucketCorsConfig> {
        debug!("Fetching CORS configuration for bucket '{}'", bucket_name);

        let response = match self
            .client
            .get_bucket_cors()
            .bucket(bucket_name)
            .send()
            .await
        {
            Ok(response) => response,
            // A bucket without a CORS configuration answers with a dedicated
            // error code; report that as "no rules" instead of a failure.
            Err(err)
                if err.as_service_error().and_then(|e| e.code())
                    == Some("NoSuchCORSConfiguration") =>
            {
                return Ok(BucketCorsConfig {
                    bucket: bucket_name.to_string(),
                    rules: Vec::new(),
                });
            }
            Err(err) => {
                return Err(err).context(format!(
                    "Failed to get the CORS configuration of bucket '{bucket_name}'"
                ));
            }
        };

        let rules: Vec<CorsRule> = response
            .cors_rules()
            .iter()
            .map(CorsRule::from_sdk)
            .collect();

        debug!(
            "Found {} CORS rule(s) for bucket '{}'",
            rules.len(),
            bucket_name
        );

        Ok(BucketCorsConfig {
            bucket: bucket_name.to_string(),
            rules,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bucket_cors_config_serialization() {
        let config = BucketCorsConfig {
            bucket: "test-bucket".to_string(),
            rules: vec![CorsRule {
                id: Some("app-uploads".to_string()),
                allowed_methods: vec!["PUT".to_string()],
                allowed_origins: vec!["https://app.example.com".to_string()],
                allowed_headers: Vec::new(),
                expose_headers: vec!["ETag".to_string()],
                max_age_seconds: Some(3000),
            }],
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: BucketCorsConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.bucket, "test-bucket");
        assert_eq!(deserialized.rules, config.rules);
    }

    #[test]
    fn test_bucket_cors_config_without_rules_serialization() {
        // The "not configured" state is an empty rule list.
        let json = r#"{"bucket": "test-bucket", "rules": []}"#;
        let deserialized: BucketCorsConfig = serde_json::from_str(json).unwrap();

        assert_eq!(deserialized.bucket, "test-bucket");
        assert!(deserialized.rules.is_empty());
    }
}

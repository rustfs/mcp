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
use tracing::info;

use super::super::S3Client;
use super::{CorsRule, validate_cors_rules};

impl S3Client {
    /// Replace a bucket's CORS configuration with the given rules.
    pub async fn put_bucket_cors(&self, bucket_name: &str, rules: &[CorsRule]) -> Result<()> {
        info!(
            "Setting the CORS configuration of bucket '{}' to {} rule(s)",
            bucket_name,
            rules.len()
        );

        validate_cors_rules(rules)?;

        let configuration = sdk_configuration(rules)?;

        self.client
            .put_bucket_cors()
            .bucket(bucket_name)
            .cors_configuration(configuration)
            .send()
            .await
            .context(format!(
                "Failed to set the CORS configuration of bucket '{bucket_name}'"
            ))?;

        info!(
            "CORS configuration of bucket '{}' replaced with {} rule(s)",
            bucket_name,
            rules.len()
        );

        Ok(())
    }
}

/// Build the request body of `put_bucket_cors` from the given rules.
fn sdk_configuration(rules: &[CorsRule]) -> Result<aws_sdk_s3::types::CorsConfiguration> {
    let sdk_rules = rules
        .iter()
        .map(CorsRule::to_sdk)
        .collect::<Result<Vec<_>>>()?;

    aws_sdk_s3::types::CorsConfiguration::builder()
        .set_cors_rules(Some(sdk_rules))
        .build()
        .context("Failed to build the CORS configuration")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sdk_configuration_keeps_every_rule() {
        let rules = [
            CorsRule {
                id: Some("app-uploads".to_string()),
                allowed_methods: vec!["GET".to_string(), "PUT".to_string(), "POST".to_string()],
                allowed_origins: vec!["https://app.example.com".to_string()],
                allowed_headers: vec!["Content-Type".to_string()],
                expose_headers: vec!["ETag".to_string()],
                max_age_seconds: Some(3000),
            },
            CorsRule {
                id: None,
                allowed_methods: vec!["GET".to_string()],
                allowed_origins: vec!["*".to_string()],
                allowed_headers: Vec::new(),
                expose_headers: Vec::new(),
                max_age_seconds: None,
            },
        ];

        // Reading the request body back with the getter's mapping must
        // return the rules exactly as they were given.
        let configuration = sdk_configuration(&rules).unwrap();
        let sent: Vec<CorsRule> = configuration
            .cors_rules()
            .iter()
            .map(CorsRule::from_sdk)
            .collect();
        assert_eq!(sent, rules);
    }
}

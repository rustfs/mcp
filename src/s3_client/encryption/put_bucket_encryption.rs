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
use super::{ServerSideEncryptionRule, validate_encryption_rule};

/// The request body of `PutBucketEncryption`: exactly the given rule.
fn encryption_configuration(
    rule: &ServerSideEncryptionRule,
) -> Result<aws_sdk_s3::types::ServerSideEncryptionConfiguration> {
    aws_sdk_s3::types::ServerSideEncryptionConfiguration::builder()
        .rules(rule.to_sdk()?)
        .build()
        .context("Failed to build the encryption configuration")
}

impl S3Client {
    /// Replace a bucket's default encryption configuration with one rule.
    ///
    /// The rule only applies to objects written from now on; existing objects
    /// keep the encryption they were stored with.
    pub async fn put_bucket_encryption(
        &self,
        bucket_name: &str,
        rule: ServerSideEncryptionRule,
    ) -> Result<()> {
        info!(
            "Setting the default encryption of bucket '{}' to {}",
            bucket_name, rule.apply_server_side_encryption_by_default.sse_algorithm
        );

        validate_encryption_rule(&rule)?;

        let configuration = encryption_configuration(&rule)?;

        self.client
            .put_bucket_encryption()
            .bucket(bucket_name)
            .server_side_encryption_configuration(configuration)
            .send()
            .await
            .context(format!(
                "Failed to set the encryption configuration of bucket '{bucket_name}'"
            ))?;

        info!(
            "Default encryption of bucket '{}' set to {}",
            bucket_name, rule.apply_server_side_encryption_by_default.sse_algorithm
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encryption_configuration_holds_the_single_rule() {
        let rule = ServerSideEncryptionRule {
            apply_server_side_encryption_by_default: super::super::ServerSideEncryptionByDefault {
                sse_algorithm: "aws:kms".to_string(),
                kms_key_id: Some("arn:aws:kms:us-east-1:111122223333:key/abc".to_string()),
            },
            bucket_key_enabled: Some(true),
        };

        let configuration = encryption_configuration(&rule).unwrap();

        let rules = configuration.rules();
        assert_eq!(rules.len(), 1);
        let default = rules[0].apply_server_side_encryption_by_default().unwrap();
        assert_eq!(default.sse_algorithm().as_str(), "aws:kms");
        assert_eq!(
            default.kms_master_key_id(),
            Some("arn:aws:kms:us-east-1:111122223333:key/abc")
        );
        assert_eq!(rules[0].bucket_key_enabled(), Some(true));
    }

    #[test]
    fn test_encryption_configuration_rejects_unknown_algorithm() {
        let rule = ServerSideEncryptionRule {
            apply_server_side_encryption_by_default: super::super::ServerSideEncryptionByDefault {
                sse_algorithm: "aes256".to_string(),
                kms_key_id: None,
            },
            bucket_key_enabled: None,
        };

        assert!(encryption_configuration(&rule).is_err());
    }
}

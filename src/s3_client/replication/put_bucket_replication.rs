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
use super::rule::ReplicationRule;
use super::validate::validate_replication_rules;

impl S3Client {
    /// Replace a bucket's replication configuration with `rules`, copied by
    /// assuming the IAM role `role`.
    pub async fn put_bucket_replication(
        &self,
        bucket_name: &str,
        role: &str,
        rules: &[ReplicationRule],
    ) -> Result<()> {
        info!(
            "Setting {} replication rule(s) on bucket '{}'",
            rules.len(),
            bucket_name
        );

        validate_replication_rules(bucket_name, rules)?;

        // Some S3 implementations accept a replication configuration for a
        // bucket that does not exist and only apply it once the bucket is
        // created. Confirm the bucket first so a typo is not reported as a
        // successful update.
        self.client
            .head_bucket()
            .bucket(bucket_name)
            .send()
            .await
            .with_context(|| {
                format!(
                    "Failed to set the replication configuration of bucket '{bucket_name}': \
                     the bucket does not exist or is not accessible"
                )
            })?;

        let configuration = aws_sdk_s3::types::ReplicationConfiguration::builder()
            .role(role)
            .set_rules(Some(
                rules
                    .iter()
                    .map(ReplicationRule::to_sdk)
                    .collect::<Result<Vec<_>>>()?,
            ))
            .build()
            .context("Failed to build the replication configuration")?;

        self.client
            .put_bucket_replication()
            .bucket(bucket_name)
            .replication_configuration(configuration)
            .send()
            .await
            .context(format!(
                "Failed to set the replication configuration of bucket '{bucket_name}'"
            ))?;

        info!(
            "Replication configuration of bucket '{}' updated",
            bucket_name
        );

        Ok(())
    }
}

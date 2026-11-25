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
use super::rule::LifecycleRule;
use super::validate::validate_lifecycle_rules;

impl S3Client {
    /// Replace a bucket's lifecycle configuration with `rules`.
    pub async fn put_bucket_lifecycle(
        &self,
        bucket_name: &str,
        rules: &[LifecycleRule],
    ) -> Result<()> {
        info!(
            "Setting {} lifecycle rule(s) on bucket '{}'",
            rules.len(),
            bucket_name
        );

        validate_lifecycle_rules(rules)?;

        // Some S3 implementations accept a lifecycle configuration for a
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
                    "Failed to set the lifecycle configuration of bucket '{bucket_name}': \
                     the bucket does not exist or is not accessible"
                )
            })?;

        let configuration = aws_sdk_s3::types::BucketLifecycleConfiguration::builder()
            .set_rules(Some(
                rules
                    .iter()
                    .map(LifecycleRule::to_sdk)
                    .collect::<Result<Vec<_>>>()?,
            ))
            .build()
            .context("Failed to build the lifecycle configuration")?;

        self.client
            .put_bucket_lifecycle_configuration()
            .bucket(bucket_name)
            .lifecycle_configuration(configuration)
            .send()
            .await
            .context(format!(
                "Failed to set the lifecycle configuration of bucket '{bucket_name}'"
            ))?;

        info!(
            "Lifecycle configuration of bucket '{}' updated",
            bucket_name
        );

        Ok(())
    }
}

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
use super::BucketVersioningStatus;

impl S3Client {
    /// Enable or suspend versioning for a bucket.
    ///
    /// Suspending keeps the existing versions; it only stops new ones from
    /// being created.
    pub async fn put_bucket_versioning(
        &self,
        bucket_name: &str,
        status: BucketVersioningStatus,
    ) -> Result<()> {
        info!(
            "Setting bucket versioning for '{}' to {}",
            bucket_name,
            status.as_str()
        );

        let configuration = aws_sdk_s3::types::VersioningConfiguration::builder()
            .status(status.to_sdk())
            .build();

        self.client
            .put_bucket_versioning()
            .bucket(bucket_name)
            .versioning_configuration(configuration)
            .send()
            .await
            .context(format!(
                "Failed to set versioning for bucket '{bucket_name}' to {}",
                status.as_str()
            ))?;

        info!(
            "Bucket versioning for '{}' set to {}",
            bucket_name,
            status.as_str()
        );

        Ok(())
    }
}

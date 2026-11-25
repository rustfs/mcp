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
use serde::{Deserialize, Serialize};
use tracing::debug;

use super::super::S3Client;
use super::BucketVersioningStatus;

/// Result payload for reading a bucket's versioning configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BucketVersioningConfig {
    pub bucket: String,
    /// `None` when versioning has never been enabled for the bucket.
    pub status: Option<BucketVersioningStatus>,
}

impl S3Client {
    /// Read a bucket's versioning configuration.
    pub async fn get_bucket_versioning(&self, bucket_name: &str) -> Result<BucketVersioningConfig> {
        debug!(
            "Fetching versioning configuration for bucket '{}'",
            bucket_name
        );

        let response = self
            .client
            .get_bucket_versioning()
            .bucket(bucket_name)
            .send()
            .await
            .context(format!(
                "Failed to get the versioning configuration of bucket '{bucket_name}'"
            ))?;

        Ok(BucketVersioningConfig {
            bucket: bucket_name.to_string(),
            status: response.status().and_then(BucketVersioningStatus::from_sdk),
        })
    }
}

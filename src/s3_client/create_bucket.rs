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

use super::S3Client;
use super::types::BucketInfo;

impl S3Client {
    /// Create a bucket.
    pub async fn create_bucket(&self, bucket_name: &str) -> Result<BucketInfo> {
        self.create_bucket_with(bucket_name, false).await
    }

    /// Create a bucket with Object Lock enabled, which also turns on
    /// versioning. Object Lock can only be enabled when the bucket is
    /// created, and it cannot be turned off afterwards.
    pub async fn create_bucket_with_object_lock(&self, bucket_name: &str) -> Result<BucketInfo> {
        self.create_bucket_with(bucket_name, true).await
    }

    async fn create_bucket_with(
        &self,
        bucket_name: &str,
        object_lock_enabled: bool,
    ) -> Result<BucketInfo> {
        info!(
            "Creating S3 bucket '{}': object lock {}",
            bucket_name, object_lock_enabled
        );

        let mut request = self.client.create_bucket().bucket(bucket_name);

        if object_lock_enabled {
            request = request.object_lock_enabled_for_bucket(true);
        }

        request
            .send()
            .await
            .context(format!("Failed to create S3 bucket: {bucket_name}"))?;

        info!(
            "Bucket '{}' created successfully (object lock: {})",
            bucket_name, object_lock_enabled
        );
        Ok(BucketInfo {
            name: bucket_name.to_string(),
            creation_date: None, // Creation date not returned by create_bucket
        })
    }
}

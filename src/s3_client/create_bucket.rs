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
        info!("Creating S3 bucket: {}", bucket_name);

        self.client
            .create_bucket()
            .bucket(bucket_name)
            .send()
            .await
            .context(format!("Failed to create S3 bucket: {bucket_name}"))?;

        info!("Bucket '{}' created successfully", bucket_name);
        Ok(BucketInfo {
            name: bucket_name.to_string(),
            creation_date: None, // Creation date not returned by create_bucket
        })
    }
}

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
use tracing::debug;

use super::S3Client;
use super::types::BucketInfo;

impl S3Client {
    /// List all accessible buckets.
    pub async fn list_buckets(&self) -> Result<Vec<BucketInfo>> {
        debug!("Listing S3 buckets");

        let response = self
            .client
            .list_buckets()
            .send()
            .await
            .context("Failed to list S3 buckets")?;

        let buckets: Vec<BucketInfo> = response
            .buckets()
            .iter()
            .map(|bucket| {
                let name = bucket.name().unwrap_or("unknown").to_string();
                let creation_date = bucket.creation_date().map(|dt| {
                    dt.fmt(aws_sdk_s3::primitives::DateTimeFormat::DateTime)
                        .unwrap()
                });

                BucketInfo {
                    name,
                    creation_date,
                }
            })
            .collect();

        debug!("Found {} buckets", buckets.len());
        Ok(buckets)
    }
}
